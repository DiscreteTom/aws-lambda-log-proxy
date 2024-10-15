//! The SimpleProcessor can only have one sink.
//! But with custom processor you can have conditional sink.

use aws_lambda_log_proxy::{LogProxy, Processor, Sink, SinkHandle, Timestamp};

// SinkHandle is clone-able so our processor is also clone-able.
#[derive(Clone)]
pub struct MyProcessor {
  stdout_sink: SinkHandle,
  stderr_sink: SinkHandle,
}

impl Default for MyProcessor {
  fn default() -> Self {
    Self {
      stdout_sink: Sink::stdout().spawn(),
      stderr_sink: Sink::stderr().spawn(),
    }
  }
}

impl Processor for MyProcessor {
  async fn process(&mut self, line: String, timestamp: Timestamp) {
    if line.starts_with("out") {
      self.stdout_sink.write_line(line, timestamp).await;
    } else {
      self.stderr_sink.write_line(line, timestamp).await;
    }
  }
  async fn truncate(&mut self) {
    self.stdout_sink.flush().await;
    self.stderr_sink.flush().await;
  }
}

#[tokio::main]
async fn main() {
  LogProxy::new()
    .processor(MyProcessor::default())
    .start()
    .await;
}
