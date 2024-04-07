mod processor;

pub use processor::*;

use aws_lambda_runtime_proxy::Proxy;
use std::process::Stdio;
use tokio::{
  io::{AsyncBufReadExt, AsyncRead, BufReader, Lines},
  sync::{mpsc, oneshot, Mutex},
};

#[derive(Default)]
pub struct LogProxy {
  /// See [`Self::stdout`].
  pub stdout: Option<Processor>,
  /// See [`Self::stderr`].
  pub stderr: Option<Processor>,
  /// See [`Self::disable_lambda_telemetry_log_fd`].
  pub disable_lambda_telemetry_log_fd: bool,
}

impl LogProxy {
  /// Set the processor for `stdout`.
  /// By default there is no processor for `stdout`.
  /// # Examples
  /// ```
  /// use aws_lambda_log_proxy::{LogProxy, SinkBuilder};
  ///
  /// #[tokio::main]
  /// async fn main() {
  ///   let sink = SinkBuilder::default().stdout().spawn();
  ///   LogProxy::default().stdout(|p| p.sink(sink));
  /// }
  /// ```
  pub fn stdout(mut self, builder: impl FnOnce(ProcessorBuilder) -> Processor) -> Self {
    self.stdout = Some(builder(ProcessorBuilder::default()));
    self
  }

  /// Set the processor for `stderr`.
  /// By default there is no processor for `stderr`.
  /// # Examples
  /// ```
  /// use aws_lambda_log_proxy::{LogProxy, SinkBuilder};
  ///
  /// #[tokio::main]
  /// async fn main() {
  ///   let sink = SinkBuilder::default().stdout().spawn();
  ///   LogProxy::default().stderr(|p| p.sink(sink));
  /// }
  /// ```
  pub fn stderr(mut self, builder: impl FnOnce(ProcessorBuilder) -> Processor) -> Self {
    self.stderr = Some(builder(ProcessorBuilder::default()));
    self
  }

  /// Remove the `_LAMBDA_TELEMETRY_LOG_FD` environment variable for the handler process
  /// to prevent logs from being written to other file descriptors.
  pub fn disable_lambda_telemetry_log_fd(mut self, disable: bool) -> Self {
    self.disable_lambda_telemetry_log_fd = disable;
    self
  }

  /// Start the log proxy.
  /// This will block the current thread.
  pub async fn start(self) {
    let mut command = Proxy::default_command();

    // only pipe if there is a processor
    if self.stdout.is_some() {
      command.stdout(Stdio::piped());
    }
    if self.stderr.is_some() {
      command.stderr(Stdio::piped());
    }

    if self.disable_lambda_telemetry_log_fd {
      command.env_remove("_LAMBDA_TELEMETRY_LOG_FD");
    }

    let mut proxy = Proxy::default().command(command).spawn().await;

    let stdout_checker_tx = proxy
      .handler
      .stdout
      .take()
      .map(|file| spawn_reader(file, self.stdout.unwrap()));
    let stderr_checker_tx = proxy
      .handler
      .stderr
      .take()
      .map(|file| spawn_reader(file, self.stderr.unwrap()));

    let client = Mutex::new(proxy.client);
    proxy
      .server
      .serve(|req| async {
        if req.uri().path() == "/2018-06-01/runtime/invocation/next" {
          // in lambda, send `invocation/next` will freeze current execution environment,
          // unprocessed logs might be lost,
          // so before proceeding, wait for the processors to finish processing the logs

          // send checkers to reader threads
          let stdout_ack_rx = send_checker(&stdout_checker_tx).await;
          let stderr_ack_rx = send_checker(&stderr_checker_tx).await;

          // wait for the all checkers to finish
          wait_for_ack(stdout_ack_rx).await;
          wait_for_ack(stderr_ack_rx).await;
        }
        client.lock().await.send_request(req).await
      })
      .await
  }
}

fn spawn_reader<T: AsyncRead + Send + 'static>(
  file: T,
  mut processor: Processor,
) -> mpsc::Sender<oneshot::Sender<()>>
where
  BufReader<T>: Unpin,
{
  let (checker_tx, mut checker_rx) = mpsc::channel::<oneshot::Sender<()>>(1);

  tokio::spawn(async move {
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    let mut need_flush = false;
    loop {
      tokio::select! {
        // wait until there is at least one line in the buffer
        line = lines.next_line() => {
          processor.process(line.unwrap().unwrap()).await;
          need_flush = true;
        }
        // the server thread requests to check if the processor has finished processing the logs.
        // this is a fallback in case the server thread got `invocation/next` while
        // there are just new lines not processed by the previous branch
        ack_tx = checker_rx.recv() => {
          // check if there are lines in the buffer
          while let Some(index) = next_newline_index(&mut lines) {
            // next line exists, process it
            let mut line = String::from_utf8(lines.get_ref().buffer()[..index].to_vec()).expect("invalid utf-8");
            if line.ends_with('\r') {
              line.pop(); // remove '\r'
            }
            lines.get_mut().consume(index + 1); // index + 1 is the count
            processor.process(line).await;
            need_flush = true;
          }

          // flush the processor since the execution environment might be frozen
          if need_flush {
            processor.flush().await;
            need_flush = false;
          }

          // stop suppressing the server thread
          ack_tx.unwrap().send(()).unwrap();
        }
      }
    }
  });

  checker_tx
}

fn next_newline_index<T: AsyncRead + Send + 'static>(
  lines: &mut Lines<BufReader<T>>,
) -> Option<usize>
where
  BufReader<T>: Unpin,
{
  // TODO: check by char instead of by byte?
  lines.get_ref().buffer().iter().position(|&b| b == b'\n')
}

async fn send_checker(
  checker_tx: &Option<mpsc::Sender<oneshot::Sender<()>>>,
) -> Option<oneshot::Receiver<()>> {
  match checker_tx {
    Some(checker_tx) => {
      let (ack_tx, ack_rx) = oneshot::channel();
      checker_tx.send(ack_tx).await.unwrap();
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_log_proxy_default() {
    let proxy = LogProxy::default();
    assert!(proxy.stdout.is_none());
    assert!(proxy.stderr.is_none());
    assert!(!proxy.disable_lambda_telemetry_log_fd);
  }

  #[tokio::test]
  async fn test_log_proxy_stdout() {
    let sink = SinkBuilder::default().stdout().spawn();
    let proxy = LogProxy::default().stdout(|p| p.sink(sink));
    assert!(proxy.stdout.is_some());
    assert!(proxy.stderr.is_none());
    assert!(!proxy.disable_lambda_telemetry_log_fd);
  }

  #[tokio::test]
  async fn test_log_proxy_stderr() {
    let sink = SinkBuilder::default().stdout().spawn();
    let proxy = LogProxy::default().stderr(|p| p.sink(sink));
    assert!(proxy.stdout.is_none());
    assert!(proxy.stderr.is_some());
    assert!(!proxy.disable_lambda_telemetry_log_fd);
  }

  #[test]
  fn test_log_proxy_disable_lambda_telemetry_log_fd() {
    let proxy = LogProxy::default().disable_lambda_telemetry_log_fd(true);
    assert!(proxy.stdout.is_none());
    assert!(proxy.stderr.is_none());
    assert!(proxy.disable_lambda_telemetry_log_fd);
  }

  #[tokio::test]
  async fn test_has_newline_in_buffer() {
    let mut lines = BufReader::new("\nhello\nworld\n\n".as_bytes()).lines();
    lines.get_mut().fill_buf().await.unwrap();
    assert_eq!(next_newline_index(&mut lines), Some(0));
    assert_eq!(lines.next_line().await.unwrap(), Some("".into()));
    assert_eq!(next_newline_index(&mut lines), Some(5));
    assert_eq!(lines.next_line().await.unwrap(), Some("hello".into()));
    assert_eq!(next_newline_index(&mut lines), Some(5));
    assert_eq!(lines.next_line().await.unwrap(), Some("world".into()));
    assert_eq!(next_newline_index(&mut lines), Some(0));
    assert_eq!(lines.next_line().await.unwrap(), Some("".into()));
    assert_eq!(next_newline_index(&mut lines), None);
  }
}
