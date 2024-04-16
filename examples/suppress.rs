//! Even if our processor is slow, the log proxy can suppress the `invocation/next`
//! until the log line is processed.
//! In this example we will suppress the `invocation/next` for 2 seconds.
//! Synchronous invoke of the lambda function (e.g. API Gateway) will get the response
//! immediately but the lambda will run for 2 more seconds to process the log lines.

use aws_lambda_log_proxy::{LogProxy, Processor};
use std::time::Duration;
use tokio::time::sleep;

pub struct MyProcessor;

impl Processor for MyProcessor {
  async fn process(&mut self, line: String, _timestamp: i64) -> bool {
    sleep(Duration::from_secs(1)).await;
    println!("{}", line);
    true
  }
  async fn flush(&mut self) {
    sleep(Duration::from_secs(1)).await;
  }
}

#[tokio::main]
async fn main() {
  LogProxy {
    stdout: Some(MyProcessor),
    stderr: Some(MyProcessor),
    ..Default::default()
  }
  .start()
  .await;
}
