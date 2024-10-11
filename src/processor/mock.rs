use crate::Processor;

// the mock processor will discard all logs
impl Processor for () {
  async fn process(&mut self, _line: String, _timestamp: i64) -> bool {
    false
  }

  async fn flush(&mut self) {}
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn mock_processor() {
    let mut processor = ();
    assert!(!(processor.process("hello".to_string(), 0).await));
    processor.flush().await;
  }
}
