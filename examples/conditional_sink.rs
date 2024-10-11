//! The SimpleProcessor can only have one sink.
//! But with custom processor you can have conditional sink.

use aws_lambda_log_proxy::{LogProxy, Processor, Sink, SinkHandle, Timestamp};

// SinkHandle is clone-able so our processor is also clone-able.
#[derive(Clone)]
pub struct MyProcessor {
  pub stdout_sink: SinkHandle,
  pub stderr_sink: SinkHandle,
}

impl Processor for MyProcessor {
  async fn process(&mut self, line: String, timestamp: Timestamp) -> bool {
    if line.starts_with("out") {
      self.stdout_sink.write_line(line, timestamp).await;
    } else {
      self.stderr_sink.write_line(line, timestamp).await;
    }
    true
  }
  async fn flush(&mut self) {
    self.stdout_sink.flush().await;
    self.stderr_sink.flush().await;
  }
}

#[tokio::main]
async fn main() {
  let processor = MyProcessor {
    stdout_sink: Sink::stdout().spawn(),
    stderr_sink: Sink::stderr().spawn(),
  };

  LogProxy::new().processor(processor).start().await;
}
