use aws_lambda_runtime_proxy::Proxy;
use std::{process::Stdio, sync::Arc};
use tokio::{
  io::{self, AsyncBufReadExt, AsyncRead, BufReader},
  sync::Mutex,
};

pub struct LogProxy {
  pub stdout_processor: Box<dyn Fn(&str) + Send>,
  pub stderr_processor: Box<dyn Fn(&str) + Send>,
}

impl LogProxy {
  pub async fn start(self) {
    // build the handler process command, pipe stdout and stderr
    let mut command = Proxy::default_command();
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

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

    Self::spawn_reader(
      proxy.handler.stdout.take().unwrap(),
      &mutex,
      self.stdout_processor,
    );
    Self::spawn_reader(
      proxy.handler.stderr.take().unwrap(),
      &mutex,
      self.stderr_processor,
    );

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
    fd: T,
    mutex: &Arc<Mutex<()>>,
    processor: Box<dyn Fn(&str) + Send>,
  ) where
    BufReader<T>: Unpin,
  {
    let mutex = mutex.clone();

    tokio::spawn(async move {
      let reader = io::BufReader::new(fd);
      let mut lines = reader.lines();

      loop {
        // wait until there is at least one line in the buffer
        let line = lines.next_line().await.unwrap().unwrap();

        // lock the mutex to suppress the call to invocation/next
        let _ = mutex.lock().await;

        // process the first line
        processor(&line);

        // check if there are more lines in the buffer
        while lines.get_ref().buffer().contains(/* '\n' */ &10) {
          processor(&line);
        }

        // now there is no more lines in the buffer, release the mutex
      }
    });
  }
}
