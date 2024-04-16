//! Even if our processor is slow, the log proxy can suppress the `invocation/next`
//! until the log line is processed.
//! In this example we will suppress the `invocation/next` for 2 seconds.
//! Synchronous invoke of the lambda function (e.g. API Gateway) will get the response
//! immediately but the lambda will run for 2 more seconds to process the log lines.

use aws_lambda_log_proxy::{LogProxy, Processor, Sink, SinkHandle};
use std::time::Duration;
use tokio::time::sleep;

// SinkHandle is clone-able so our processor is also clone-able.
#[derive(Clone)]
pub struct MyProcessor(SinkHandle);

impl Processor for MyProcessor {
  async fn process(&mut self, line: String, timestamp: i64) -> bool {
    sleep(Duration::from_secs(1)).await;
    self.0.write_line(line, timestamp).await;
    true
  }
  async fn flush(&mut self) {
    sleep(Duration::from_secs(1)).await;
    self.0.flush().await;
  }
}

#[tokio::main]
async fn main() {
  let processor = MyProcessor(Sink::stdout().spawn());

  LogProxy {
    stdout: Some(processor.clone()),
    stderr: Some(processor),
    ..Default::default()
  }
  .start()
  .await;
}