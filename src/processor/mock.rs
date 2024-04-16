use crate::Processor;

/// This will discard all log lines.
pub struct MockProcessor;

impl Processor for MockProcessor {
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
    let mut processor = MockProcessor;
    assert_eq!(processor.process("hello".to_string(), 0).await, false);
    processor.flush().await;
  }
}
