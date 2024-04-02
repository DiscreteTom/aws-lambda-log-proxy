mod processor;

pub use processor::*;

use aws_lambda_runtime_proxy::Proxy;
use std::{process::Stdio, sync::Arc};
use tokio::{
  io::{self, AsyncBufReadExt, AsyncRead, BufReader},
  sync::Mutex,
};

pub struct LogProxy {
  /// See [`Self::stdout`].
  pub stdout: Option<Processor>,
  /// See [`Self::stderr`].
  pub stderr: Option<Processor>,
}

impl Default for LogProxy {
  fn default() -> Self {
    LogProxy {
      stdout: None,
      stderr: None,
    }
  }
}

impl LogProxy {
  /// Set the processor for stdout.
  /// By default there is no processor for stdout.
  /// # Examples
  /// ```
  /// use aws_lambda_log_proxy::LogProxy;
  ///
  /// LogProxy::default().stdout(|p| p.filter(|line| line.starts_with("a")));
  /// ```
  pub fn stdout(mut self, decorator: impl FnOnce(Processor) -> Processor) -> Self {
    self.stdout = Some(decorator(Processor::default()));
    self
  }

  /// Set the processor for stderr.
  /// By default there is no processor for stderr.
  /// # Examples
  /// ```
  /// use aws_lambda_log_proxy::LogProxy;
  ///
  /// LogProxy::default().stderr(|p| p.filter(|line| line.starts_with("a")));
  /// ```
  pub fn stderr(mut self, decorator: impl FnOnce(Processor) -> Processor) -> Self {
    self.stderr = Some(decorator(Processor::default()));
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

    // disable `_LAMBDA_TELEMETRY_LOG_FD` to ensure logs are not written into other fd.
    // this should be set especially for nodejs runtime
    if std::env::var("AWS_LAMBDA_LOG_FILTER_DISABLE_LAMBDA_TELEMETRY_LOG_FD")
      .map(|s| s == "true")
      .unwrap_or(false)
    {
      command.env_remove("_LAMBDA_TELEMETRY_LOG_FD");
    }

    let mut proxy = Proxy::default().command(command).spawn().await;

    // create a mutex to ensure logs are written before the proxy call invocation/next
    let mutex = Arc::new(Mutex::new(()));

    proxy
      .handler
      .stdout
      .take()
      .map(|file| Self::spawn_reader(file, &mutex, self.stdout.unwrap()));
    proxy
      .handler
      .stderr
      .take()
      .map(|file| Self::spawn_reader(file, &mutex, self.stderr.unwrap()));

    let client = Mutex::new(proxy.client);
    proxy
      .server
      .serve(|req| async {
        if req.uri().path() == "/2018-06-01/runtime/invocation/next" {
          // wait until there is no more lines in the buffer
          let _ = mutex.lock().await;
        }
        client.lock().await.send_request(req).await
      })
      .await
  }

  fn spawn_reader<T: AsyncRead + Send + 'static>(
    file: T,
    mutex: &Arc<Mutex<()>>,
    mut processor: Processor,
  ) where
    BufReader<T>: Unpin,
  {
    let mutex = mutex.clone();

    tokio::spawn(async move {
      let reader = io::BufReader::new(file);
      let mut lines = reader.lines();

      loop {
        // wait until there is at least one line in the buffer
        let line = lines.next_line().await.unwrap().unwrap();

        // lock the mutex to suppress the call to invocation/next
        let _ = mutex.lock().await;

        // process the first line
        processor.process(line).await;

        // check if there are more lines in the buffer
        while lines.get_ref().buffer().contains(/* '\n' */ &10) {
          // next line exists, process it
          let line = lines.next_line().await.unwrap().unwrap();
          processor.process(line).await;
        }

        // now there is no more lines in the buffer, release the mutex
      }
    });
  }
}
