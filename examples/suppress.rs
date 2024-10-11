use aws_lambda_log_proxy::{LogProxy, Processor, Sink, SinkHandle, Timestamp};
use std::time::Duration;
use tokio::time::sleep;

// SinkHandle is clone-able so our processor is also clone-able.
#[derive(Clone)]
pub struct MyProcessor(SinkHandle);

impl Processor for MyProcessor {
  async fn process(&mut self, line: String, timestamp: Timestamp) -> bool {
    sleep(Duration::from_secs(1)).await;
    self.0.write_line(line, timestamp).await;
    true
  }
  async fn flush(&mut self) {
    sleep(Duration::from_secs(1)).await;
    self.0.flush().await;
  }
  async fn truncate(&mut self) {
    eprintln!("truncated")
  }
}

#[tokio::main]
async fn main() {
  let processor = MyProcessor(Sink::stdout().spawn());

  LogProxy::new().processor(processor).start().await;
}
