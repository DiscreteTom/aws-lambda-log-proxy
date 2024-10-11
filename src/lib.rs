mod processor;

use aws_lambda_runtime_proxy::{LambdaRuntimeApiClient, MockLambdaRuntimeApiServer};
use chrono::Utc;
use http::{HeaderMap, HeaderValue};
use tokio::{
  io::{stdin, AsyncBufReadExt, AsyncRead, BufReader},
  sync::{mpsc, oneshot},
};

pub use processor::*;

/// # Examples
/// Simple creation:
/// ```
/// use aws_lambda_log_proxy::{LogProxy, Sink};
///
/// #[tokio::main]
/// async fn main() {
///   let proxy = LogProxy::new().processor(|p| p.sink(Sink::stdout().spawn()));
///   // start the proxy
///   // proxy.start().await;
/// }
/// ```
/// Custom creation:
/// ```
/// use aws_lambda_log_proxy::{LogProxy, Processor};
///
/// pub struct MyProcessor;
///
/// impl Processor for MyProcessor {
///   async fn process(&mut self, _line: String, _timestamp: i64) -> bool {
///     false
///   }
///
///   async fn flush(&mut self) {}
/// }
///
/// let proxy = LogProxy {
///   processor: Some(MyProcessor),
///   ..Default::default()
/// };
/// ```
pub struct LogProxy<P> {
  /// The processor for stdin.
  /// Defaults to [`None`].
  pub processor: Option<P>,
  /// See [`Self::buffer_size`].
  pub buffer_size: usize,
  /// See [`Self::port`].
  pub port: u16,
}

impl<P> Default for LogProxy<P> {
  fn default() -> Self {
    Self {
      processor: None,
      buffer_size: 256,
      port: 3000,
    }
  }
}

impl LogProxy<()> {
  /// Create a new instance with the default properties
  /// and a mock processor which will discard all logs.
  pub fn new() -> Self {
    Self::default()
  }
}

impl<P> LogProxy<P> {
  /// Set [`Self::processor`] to a [`SimpleProcessor`] via [`SimpleProcessorBuilder`].
  /// # Examples
  /// ```
  /// use aws_lambda_log_proxy::{LogProxy, Sink};
  ///
  /// #[tokio::main]
  /// async fn main() {
  ///   LogProxy::new().processor(|p| p.sink(Sink::stdout().spawn()));
  /// }
  /// ```
  pub fn processor(
    self,
    builder: impl FnOnce(SimpleProcessorBuilder) -> SimpleProcessor,
  ) -> LogProxy<SimpleProcessor> {
    LogProxy {
      processor: Some(builder(SimpleProcessorBuilder::default())),
      buffer_size: self.buffer_size,
      port: self.port,
    }
  }

  /// Set how many lines can be buffered if the processing is slow.
  /// If the handler process writes too many lines then return the response immediately,
  /// the suppression of `invocation/next`
  /// might not working, maybe some logs will be processed in the next invocation.
  /// Increase this value should help to prevent logs from being lost.
  /// The default value is `256`.
  pub fn buffer_size(mut self, buffer_size: usize) -> Self {
    self.buffer_size = buffer_size;
    self
  }

  /// Set the port for the log proxy.
  /// The default value is `3000`.
  pub fn port(mut self, port: u16) -> Self {
    self.port = port;
    self
  }

  /// Start the log proxy.
  /// This will block the current thread.
  pub async fn start(self)
  where
    P: Processor,
  {
    let tx = self
      .processor
      .map(|p| spawn_reader(stdin(), p, self.buffer_size));

    MockLambdaRuntimeApiServer::bind(self.port)
      .await
      .unwrap()
      .serve(move |req| {
        let tx = tx.clone();
        async move {
          match tx {
            // if no processor, just forward the request
            None => {
              LambdaRuntimeApiClient::new()
                .await
                .unwrap()
                .forward(req)
                .await
            }
            Some((checker_tx, next_tx)) => {
              let is_invocation_next = req.uri().path() == "/2018-06-01/runtime/invocation/next";

              if is_invocation_next {
                // in lambda, send `invocation/next` will freeze current execution environment,
                // unprocessed logs might be lost,
                // so before proceeding, wait for the processors to finish processing the logs

                // send checkers to reader threads
                let (ack_tx, ack_rx) = oneshot::channel();
                checker_tx.send(ack_tx).await.unwrap();
                // wait for the checker to finish
                ack_rx.await.unwrap();
              }

              // forward the request to the real lambda runtime API, consume the request
              let res = LambdaRuntimeApiClient::new()
                .await
                .unwrap()
                .forward(req)
                .await
                .unwrap();

              if is_invocation_next {
                let (ack_tx, ack_rx) = oneshot::channel();
                next_tx.send((ack_tx, res.headers().clone())).await.unwrap();
                // wait for the checker to finish
                ack_rx.await.unwrap();
              }

              Ok(res)
            }
          }
        }
      })
      .await
  }
}

fn spawn_reader<F: AsyncRead + Send + 'static, P: Processor + 'static>(
  file: F,
  mut processor: P,
  buffer_size: usize,
) -> (
  mpsc::Sender<oneshot::Sender<()>>,
  mpsc::Sender<(oneshot::Sender<()>, HeaderMap<HeaderValue>)>,
)
where
  BufReader<F>: Unpin,
{
  let (checker_tx, mut checker_rx) = mpsc::channel::<oneshot::Sender<()>>(1);
  let (next_tx, mut next_rx) = mpsc::channel::<(oneshot::Sender<()>, HeaderMap<HeaderValue>)>(1);
  let (buffer_tx, mut buffer_rx) = mpsc::channel(buffer_size);

  // the reader thread, read from the file then push into the buffer
  tokio::spawn(async move {
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    while let Ok(Some(line)) = lines.next_line().await {
      // `next_line` already removes '\n' and '\r', so we only need to check if the line is empty.
      // only push into buffer if the line is not empty
      if !line.is_empty() {
        // put line in a queue, record the timestamp
        buffer_tx
          .send((line, Utc::now().timestamp_micros()))
          .await
          .unwrap();
      }
    }
  });

  // the processor thread
  tokio::spawn(async move {
    let mut need_flush = false;

    loop {
      tokio::select! {
        // enable `biased` to make sure we always try to recv from buffer before accept the server thread checker
        biased;

        res = buffer_rx.recv() => {
          let (line, timestamp) = res.unwrap();
          need_flush = processor.process(line, timestamp).await || need_flush;
          // we don't need to flush here.
          // if we are writing to stdout, it is already line-buffered and will flush by line.
          // if we are writing to telemetry log fd, timestamp is appended so we don't need to flush it immediately.
        }
        next = next_rx.recv() => {
          let (ack, headers) = next.unwrap();
          processor.next(headers).await;
          ack.send(()).unwrap();
        }
        // the server thread requests to check if the processor has finished processing the logs.
        checker = checker_rx.recv() => {
          // since we are using `biased` select, we don't need to check if there is a message in the buffer,
          // just stop suppressing the server thread if the branch is executed

          if need_flush {
            processor.flush().await;
            need_flush = false;
          }

          processor.truncate().await;

          checker.unwrap().send(()).unwrap();
        }
      }
    }
  });

  (checker_tx, next_tx)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_log_proxy_default() {
    let proxy = LogProxy::new();
    assert!(proxy.processor.is_none());
    assert_eq!(proxy.buffer_size, 256);
    assert_eq!(proxy.port, 3000);
  }

  #[tokio::test]
  async fn test_log_proxy_processor() {
    let sink = Sink::stdout().spawn();
    let proxy = LogProxy::new().processor(|p| p.sink(sink));
    assert!(proxy.processor.is_some());
    assert_eq!(proxy.buffer_size, 256);
    assert_eq!(proxy.port, 3000);
  }

  #[test]
  fn test_log_proxy_buffer_size() {
    let proxy = LogProxy::new().buffer_size(512);
    assert_eq!(proxy.buffer_size, 512);
  }

  #[test]
  fn test_log_proxy_port() {
    let proxy = LogProxy::new().port(3001);
    assert_eq!(proxy.port, 3001);
  }

  // this is to check if the `start` can be called with different processors during the compile time
  // so don't run this test
  async fn _ensure_start_can_be_called() {
    // mock processor
    let proxy: LogProxy<()> = LogProxy::new();
    proxy.start().await;
    let sink = Sink::stdout().spawn();
    let proxy: LogProxy<SimpleProcessor> = LogProxy::new().processor(|p| p.sink(sink.clone()));
    proxy.start().await;
  }
}
