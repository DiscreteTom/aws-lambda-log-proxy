mod processor;

pub use processor::*;

use aws_lambda_runtime_proxy::{LambdaRuntimeApiClient, Proxy};
use std::process::Stdio;
use tokio::{
  io::{AsyncBufReadExt, AsyncRead, BufReader},
  sync::{mpsc, oneshot},
};

/// # Examples
/// Simple creation:
/// ```
/// use aws_lambda_log_proxy::{LogProxy, Sink};
///
/// LogProxy::new().stdout(|p| p.sink(Sink::stdout()));
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
/// let proxy: LogProxy<_, ()> = LogProxy {
///   stdout: Some(MyProcessor),
///   ..Default::default()
/// };
/// ```
pub struct LogProxy<StdoutProcessor, StderrProcessor> {
  /// The processor for the handler process's `stdout`.
  /// Defaults to [`None`].
  pub stdout: Option<StdoutProcessor>,
  /// The processor for the handler process's `stderr`.
  /// Defaults to [`None`].
  pub stderr: Option<StderrProcessor>,
  /// See [`Self::buffer_size`].
  pub buffer_size: usize,
  /// See [`Self::disable_lambda_telemetry_log_fd_for_handler`].
  pub disable_lambda_telemetry_log_fd_for_handler: bool,
}

impl<StdoutProcessor, StderrProcessor> Default for LogProxy<StdoutProcessor, StderrProcessor> {
  fn default() -> Self {
    Self {
      stdout: None,
      stderr: None,
      buffer_size: 256,
      disable_lambda_telemetry_log_fd_for_handler: false,
    }
  }
}

impl LogProxy<(), ()> {
  /// Create a new `LogProxy<(), ()>` with default settings.
  pub fn new() -> Self {
    Self::default()
  }
}

impl<StdoutProcessor, StderrProcessor> LogProxy<StdoutProcessor, StderrProcessor> {
  /// Set [`Self::stdout`] to a [`SimpleProcessor`] via [`SimpleProcessorBuilder`].
  /// # Examples
  /// ```
  /// use aws_lambda_log_proxy::{LogProxy, Sink};
  ///
  /// let sink = Sink::stdout();
  /// LogProxy::new().stdout(|p| p.sink(sink));
  /// ```
  pub fn stdout(
    self,
    builder: impl FnOnce(SimpleProcessorBuilder) -> SimpleProcessor,
  ) -> LogProxy<SimpleProcessor, StderrProcessor> {
    LogProxy {
      stdout: Some(builder(SimpleProcessorBuilder::default())),
      stderr: self.stderr,
      buffer_size: self.buffer_size,
      disable_lambda_telemetry_log_fd_for_handler: self.disable_lambda_telemetry_log_fd_for_handler,
    }
  }

  /// Set [`Self::stderr`] to a [`SimpleProcessor`] via [`SimpleProcessorBuilder`].
  /// # Examples
  /// ```
  /// use aws_lambda_log_proxy::{LogProxy, Sink};
  ///
  /// let sink = Sink::stdout();
  /// LogProxy::new().stderr(|p| p.sink(sink));
  /// ```
  pub fn stderr(
    self,
    builder: impl FnOnce(SimpleProcessorBuilder) -> SimpleProcessor,
  ) -> LogProxy<StdoutProcessor, SimpleProcessor> {
    LogProxy {
      stdout: self.stdout,
      stderr: Some(builder(SimpleProcessorBuilder::default())),
      buffer_size: self.buffer_size,
      disable_lambda_telemetry_log_fd_for_handler: self.disable_lambda_telemetry_log_fd_for_handler,
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

  /// Remove the `_LAMBDA_TELEMETRY_LOG_FD` environment variable for the handler process
  /// to prevent logs from being written to other file descriptors.
  pub fn disable_lambda_telemetry_log_fd_for_handler(mut self, disable: bool) -> Self {
    self.disable_lambda_telemetry_log_fd_for_handler = disable;
    self
  }

  /// Start the log proxy.
  /// This will block the current thread.
  pub async fn start(self)
  where
    StdoutProcessor: Processor,
    StderrProcessor: Processor,
  {
    let mut command = Proxy::default_command();

    // only pipe if there is a processor
    if self.stdout.is_some() {
      command.stdout(Stdio::piped());
    }
    if self.stderr.is_some() {
      command.stderr(Stdio::piped());
    }

    if self.disable_lambda_telemetry_log_fd_for_handler {
      command.env_remove("_LAMBDA_TELEMETRY_LOG_FD");
    }

    let mut proxy = Proxy::default().command(command).spawn().await;

    let stdout_checker_tx = proxy
      .handler
      .stdout
      .take()
      .map(|file| spawn_reader(file, self.stdout.unwrap(), self.buffer_size));
    let stderr_checker_tx = proxy
      .handler
      .stderr
      .take()
      .map(|file| spawn_reader(file, self.stderr.unwrap(), self.buffer_size));

    proxy
      .server
      .serve(move |req| {
        let stdout_checker_tx = stdout_checker_tx.clone();
        let stderr_checker_tx = stderr_checker_tx.clone();
        async move {
          if req.uri().path() == "/2018-06-01/runtime/invocation/next" {
            // in lambda, send `invocation/next` will freeze current execution environment,
            // unprocessed logs might be lost,
            // so before proceeding, wait for the processors to finish processing the logs

            // send checkers to reader threads
            let stdout_ack_rx = send_checker(&stdout_checker_tx);
            let stderr_ack_rx = send_checker(&stderr_checker_tx);

            // wait for the all checkers to finish
            wait_for_ack(stdout_ack_rx.await).await;
            wait_for_ack(stderr_ack_rx.await).await;
          }

          // forward the request to the real lambda runtime API, consume the request
          LambdaRuntimeApiClient::forward(req).await
        }
      })
      .await
  }
}

fn spawn_reader<F: AsyncRead + Send + 'static, P: Processor + 'static>(
  file: F,
  mut processor: P,
  buffer_size: usize,
) -> mpsc::Sender<Checker>
where
  BufReader<F>: Unpin,
{
  let (checker_tx, mut checker_rx) = mpsc::channel::<Checker>(1);
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
          .send((line, chrono::Utc::now().timestamp_micros()))
          .await
          .unwrap();
      }
    }
  });

  // the processor thread
  tokio::spawn(async move {
    loop {
      tokio::select! {
        // enable `biased` to make sure we always try to recv from buffer before accept the server thread checker
        biased;

        res = buffer_rx.recv() => {
          let (line, timestamp) = res.unwrap();
          if processor.process(line, timestamp).await {
            // only flush if the line is written to the sink
            // TODO: do we need to flush every time? or only flush in the next branch on the server thread checker?
            // we flush here to ensure the logs are written as soon as possible
            processor.flush().await;
          }
        }
        // the server thread requests to check if the processor has finished processing the logs.
        checker = checker_rx.recv() => {
          // since we are using `biased` select, we don't need to check if there is a message in the buffer,
          // just stop suppressing the server thread if the branch is executed
          checker.unwrap().ack_tx.send(()).unwrap();
        }
      }
    }
  });

  checker_tx
}

async fn send_checker(checker_tx: &Option<mpsc::Sender<Checker>>) -> Option<oneshot::Receiver<()>> {
  match checker_tx {
    Some(checker_tx) => {
      let (ack_tx, ack_rx) = oneshot::channel();
      checker_tx.send(Checker { ack_tx }).await.unwrap();
      Some(ack_rx)
    }
    None => None,
  }
}

async fn wait_for_ack(ack_rx: Option<oneshot::Receiver<()>>) {
  if let Some(ack_rx) = ack_rx {
    ack_rx.await.unwrap();
  }
}

struct Checker {
  ack_tx: oneshot::Sender<()>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_log_proxy_default() {
    let proxy = LogProxy::new();
    assert!(proxy.stdout.is_none());
    assert!(proxy.stderr.is_none());
    assert_eq!(proxy.buffer_size, 256);
    assert!(!proxy.disable_lambda_telemetry_log_fd_for_handler);
  }

  #[test]
  fn test_log_proxy_stdout() {
    let sink = Sink::stdout();
    let proxy = LogProxy::new().stdout(|p| p.sink(sink));
    assert!(proxy.stdout.is_some());
    assert!(proxy.stderr.is_none());
    assert_eq!(proxy.buffer_size, 256);
    assert!(!proxy.disable_lambda_telemetry_log_fd_for_handler);
  }

  #[test]
  fn test_log_proxy_stderr() {
    let sink = Sink::stdout();
    let proxy = LogProxy::new().stderr(|p| p.sink(sink));
    assert!(proxy.stdout.is_none());
    assert!(proxy.stderr.is_some());
    assert_eq!(proxy.buffer_size, 256);
    assert!(!proxy.disable_lambda_telemetry_log_fd_for_handler);
  }

  #[test]
  fn test_log_proxy_buffer_size() {
    let proxy = LogProxy::new().buffer_size(512);
    assert_eq!(proxy.buffer_size, 512);
  }

  #[test]
  fn test_log_proxy_disable_lambda_telemetry_log_fd() {
    let proxy = LogProxy::new().disable_lambda_telemetry_log_fd_for_handler(true);
    assert!(proxy.stdout.is_none());
    assert!(proxy.stderr.is_none());
    assert_eq!(proxy.buffer_size, 256);
    assert!(proxy.disable_lambda_telemetry_log_fd_for_handler);
  }
}
