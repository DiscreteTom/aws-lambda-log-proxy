#[cfg(feature = "emf")]
mod emf;
mod processor;

use aws_lambda_runtime_proxy::{LambdaRuntimeApiClient, MockLambdaRuntimeApiServer};
use chrono::Utc;
use tokio::{
  io::{stdin, AsyncBufReadExt, AsyncRead, BufReader},
  sync::{mpsc, oneshot},
};
use tracing::{debug, trace};

#[cfg(feature = "emf")]
pub use emf::*;
pub use processor::*;

/// # Examples
/// Simple creation:
/// ```
/// use aws_lambda_log_proxy::{LogProxy, Sink};
///
/// # async fn t1() {
/// LogProxy::new().simple(|p| p.sink(Sink::stdout().spawn())).start().await;
/// # }
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
/// # async fn t1() {
/// LogProxy::new().processor(MyProcessor).buffer_size(1024).port(1234).start().await;
/// # }
/// ```
pub struct LogProxy<P> {
  /// The processor for stdin.
  pub processor: P,
  /// See [`Self::buffer_size`].
  pub buffer_size: usize,
  /// See [`Self::port`].
  pub port: u16,
}

impl<P: Default> Default for LogProxy<P> {
  fn default() -> Self {
    Self {
      processor: Default::default(),
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
  /// Set [`Self::processor`] to a custom processor.
  pub fn processor<T>(self, processor: T) -> LogProxy<T> {
    LogProxy {
      processor,
      buffer_size: self.buffer_size,
      port: self.port,
    }
  }

  /// Set [`Self::processor`] to a [`SimpleProcessor`] via [`SimpleProcessorBuilder`].
  /// # Examples
  /// ```
  /// use aws_lambda_log_proxy::{LogProxy, Sink};
  ///
  /// # async fn t1() {
  /// LogProxy::new().simple(|p| p.sink(Sink::stdout().spawn()));
  /// # }
  /// ```
  pub fn simple(
    self,
    builder: impl FnOnce(SimpleProcessorBuilder) -> SimpleProcessor,
  ) -> LogProxy<SimpleProcessor> {
    self.processor(builder(SimpleProcessorBuilder::default()))
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
    debug!(port = %self.port, buffer_size = %self.buffer_size, "Starting log proxy");

    let checker_tx = spawn_reader(stdin(), self.processor, self.buffer_size);

    MockLambdaRuntimeApiServer::bind(self.port)
      .await
      .unwrap()
      .serve(move |req| {
        let checker_tx = checker_tx.clone();
        async move {
          let is_invocation_next = req.uri().path() == "/2018-06-01/runtime/invocation/next";

          if is_invocation_next {
            // in lambda, send `invocation/next` will freeze current execution environment,
            // unprocessed logs might be lost,
            // so before proceeding, wait for the processors to finish processing the logs

            // send checkers to reader threads
            let (ack_tx, ack_rx) = oneshot::channel();
            checker_tx.send(ack_tx).await.unwrap();
            // wait for the checker to finish
            debug!("Waiting for the processor to finish processing logs");
            ack_rx.await.unwrap();
            debug!("Processor finished processing logs");
          }

          // forward the request to the real lambda runtime API, consume the request
          LambdaRuntimeApiClient::new()
            .await
            .unwrap()
            .forward(req)
            .await
        }
      })
      .await
  }
}

fn spawn_reader<F: AsyncRead + Send + 'static, P: Processor + 'static>(
  file: F,
  mut processor: P,
  buffer_size: usize,
) -> mpsc::Sender<oneshot::Sender<()>>
where
  BufReader<F>: Unpin,
{
  let (checker_tx, mut checker_rx) = mpsc::channel::<oneshot::Sender<()>>(1);
  let (buffer_tx, mut buffer_rx) = mpsc::channel(buffer_size);

  // the reader thread, read from the file then push into the buffer.
  // we use a separate thread to read from the file to get an accurate timestamp.
  tokio::spawn(async move {
    let mut lines = BufReader::new(file).lines();
    while let Ok(Some(line)) = lines.next_line().await {
      trace!(line = %line, "Read line");
      // `next_line` already removes '\n' and '\r', so we only need to check if the line is empty.
      // only push into buffer if the line is not empty
      if !line.is_empty() {
        // put line in a queue, record the timestamp
        buffer_tx.send((line, Utc::now())).await.unwrap();
      }
    }
    debug!("Reader thread finished");
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

  checker_tx
}

#[cfg(test)]
mod tests {
  use super::*;

  macro_rules! assert_unit {
    ($unit:expr) => {
      let _: () = $unit;
    };
  }

  #[test]
  fn test_log_proxy_default() {
    let proxy = LogProxy::new();
    assert_unit!(proxy.processor);
    assert_eq!(proxy.buffer_size, 256);
    assert_eq!(proxy.port, 3000);
  }

  #[tokio::test]
  async fn test_log_proxy_simple() {
    let sink = Sink::stdout().spawn();
    let proxy = LogProxy::new().simple(|p| p.sink(sink));
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
    let proxy: LogProxy<SimpleProcessor> = LogProxy::new().simple(|p| p.sink(sink.clone()));
    proxy.start().await;
  }
}
