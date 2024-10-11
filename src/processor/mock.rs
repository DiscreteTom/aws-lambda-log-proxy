use super::Timestamp;
use crate::Processor;

// the mock processor will discard all logs
impl Processor for () {
  async fn process(&mut self, _line: String, _timestamp: Timestamp) {}

  async fn truncate(&mut self) {}
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::DateTime;

  #[tokio::test]
  async fn mock_processor() {
    let mut processor = ();
    processor
      .process("hello".to_string(), mock_timestamp())
      .await;
    processor.truncate().await;
  }

  fn mock_timestamp() -> Timestamp {
    DateTime::from_timestamp(0, 0).unwrap()
  }
}
