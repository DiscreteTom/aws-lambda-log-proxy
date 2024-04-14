use crate::Processor;

/// This will discard all log lines.
pub struct MockProcessor;

impl Processor for MockProcessor {
  async fn process(&mut self, _line: String, _timestamp: i64) -> bool {
    false
  }

  async fn flush(&mut self) {}
}
