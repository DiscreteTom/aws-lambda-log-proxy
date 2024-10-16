//! See ./suppress.README.md for more information.

use aws_lambda_log_proxy::{LogProxy, Processor, Sink, SinkHandle, Timestamp};
use std::time::Duration;
use tokio::time::sleep;

// SinkHandle is clone-able so our processor is also clone-able.
#[derive(Clone)]
pub struct MyProcessor(SinkHandle);

impl Processor for MyProcessor {
  async fn process(&mut self, line: String, timestamp: Timestamp) {
    sleep(Duration::from_secs(1)).await; // simulate processing time
    self.0.write_line(line, timestamp).await;
  }
  async fn truncate(&mut self) {
    sleep(Duration::from_secs(1)).await; // simulate processing time
    self.0.flush().await;
  }
}

#[tokio::main]
async fn main() {
  LogProxy::new()
    .processor(MyProcessor(Sink::stdout().spawn()))
    .start()
    .await;
}
