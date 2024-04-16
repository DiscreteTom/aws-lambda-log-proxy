mod processor;

pub use processor::*;

use aws_lambda_runtime_proxy::{LambdaRuntimeApiClient, Proxy};
use std::process::Stdio;
use tokio::{
  io::{AsyncBufReadExt, AsyncRead, BufReader},
  process::Command,
  sync::{mpsc, oneshot},
};

/// # Examples
/// Simple creation:
/// ```
/// use aws_lambda_log_proxy::{LogProxy, Sink};
///
/// #[tokio::main]
/// async fn main() {
///   let proxy = LogProxy::new().stdout(|p| p.sink(Sink::stdout().spawn()));
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

impl LogProxy<MockProcessor, MockProcessor> {
  /// Create a new [`LogProxy`] with default settings.
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
  /// #[tokio::main]
  /// async fn main() {
  ///   let sink = Sink::stdout().spawn();
  ///   LogProxy::new().stdout(|p| p.sink(sink));
  /// }
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
  /// #[tokio::main]
  /// async fn main() {
  ///   let sink = Sink::stdout().spawn();
  ///   LogProxy::new().stderr(|p| p.sink(sink));
  /// }
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
    let mut proxy = Proxy::default()
      .command(self.prepare_command(Proxy::default_command()))
      .spawn()
      .await;

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

  fn prepare_command(&self, mut command: Command) -> Command {
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

    command
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
    let mut need_flush = false;

    loop {
      tokio::select! {
        // enable `biased` to make sure we always try to recv from buffer before accept the server thread checker
        biased;

        res = buffer_rx.recv() => {
          let (line, timestamp) = res.unwrap();
          need_flush = need_flush || processor.process(line, timestamp).await;
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

  #[tokio::test]
  async fn test_log_proxy_stdout() {
    let sink = Sink::stdout().spawn();
    let proxy = LogProxy::new().stdout(|p| p.sink(sink));
    assert!(proxy.stdout.is_some());
    assert!(proxy.stderr.is_none());
    assert_eq!(proxy.buffer_size, 256);
    assert!(!proxy.disable_lambda_telemetry_log_fd_for_handler);
  }

  #[tokio::test]
  async fn test_log_proxy_stderr() {
    let sink = Sink::stdout().spawn();
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

  #[tokio::test]
  async fn test_prepare_command_default() {
    let proxy = LogProxy::new();
    let command = proxy.prepare_command(Command::new("true")).spawn().unwrap();
    assert!(command.stdout.is_none());
    assert!(command.stderr.is_none());
  }

  #[tokio::test]
  async fn test_prepare_command_env() {
    // by default the _LAMBDA_TELEMETRY_LOG_FD is not removed
    let proxy = LogProxy::new();
    let mut command = Command::new("bash");
    command
      .args(&["-c", "echo $_LAMBDA_TELEMETRY_LOG_FD"])
      .env("_LAMBDA_TELEMETRY_LOG_FD", "3");
    let command = proxy
      .prepare_command(command)
      .stdout(Stdio::piped())
      .spawn()
      .unwrap();
    assert!(BufReader::new(command.stdout.unwrap())
      .lines()
      .next_line()
      .await
      .unwrap()
      .unwrap()
      .contains("3"));

    // remove the _LAMBDA_TELEMETRY_LOG_FD
    let proxy = LogProxy::new().disable_lambda_telemetry_log_fd_for_handler(true);
    let mut command = Command::new("bash");
    command
      .args(&["-c", "echo $_LAMBDA_TELEMETRY_LOG_FD"])
      .env("_LAMBDA_TELEMETRY_LOG_FD", "3");
    let command = proxy
      .prepare_command(command)
      .stdout(Stdio::piped())
      .spawn()
      .unwrap();
    assert!(!BufReader::new(command.stdout.unwrap())
      .lines()
      .next_line()
      .await
      .unwrap()
      .unwrap()
      .contains("3"));
  }

  #[tokio::test]
  async fn test_processor_will_set_stdout_and_stderr() {
    let proxy = LogProxy {
      stdout: Some(MockProcessor),
      stderr: Some(MockProcessor),
      ..Default::default()
    };
    let command = proxy.prepare_command(Command::new("echo")).spawn().unwrap();
    assert!(command.stdout.is_some());
    assert!(command.stderr.is_some());
  }

  #[tokio::test]
  async fn test_wait_for_ack() {
    let (ack_tx, ack_rx) = oneshot::channel();
    tokio::spawn(async move {
      tokio::time::sleep(std::time::Duration::from_millis(10)).await;
      ack_tx.send(()).unwrap();
    });
    wait_for_ack(Some(ack_rx)).await;
  }

  #[tokio::test]
  async fn test_send_checker() {
    let (checker_tx, mut checker_rx) = mpsc::channel(1);
    tokio::spawn(async move {
      let ack_rx = send_checker(&Some(checker_tx)).await;
      wait_for_ack(ack_rx).await;
    });
    let ack_tx = checker_rx.recv().await.unwrap();
    ack_tx.ack_tx.send(()).unwrap();

    assert!(matches!(send_checker(&None).await, None));
  }

  // this is to check if the `start` can be called with different processors during the compile time
  // so don't run this test
  async fn _ensure_start_can_be_called() {
    // mock processor
    let proxy: LogProxy<MockProcessor, MockProcessor> = LogProxy::new();
    proxy.start().await;
    let sink = Sink::stdout().spawn();
    let proxy: LogProxy<SimpleProcessor, SimpleProcessor> = LogProxy::new()
      .stdout(|p| p.sink(sink.clone()))
      .stderr(|p| p.sink(sink));
    proxy.start().await;
  }
}
