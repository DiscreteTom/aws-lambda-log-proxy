use super::Timestamp;
use crate::{Processor, SinkHandle};

/// Process log lines with [`Self::transformer`]
/// and write them to [`Self::sink`].
///
/// You can use [`SimpleProcessorBuilder`](crate::SimpleProcessorBuilder) to create this.
pub struct SimpleProcessor<T> {
  /// See [`SimpleProcessorBuilder::transformer`](crate::SimpleProcessorBuilder::transformer).
  pub transformer: T,
  /// See [`SimpleProcessorBuilder::sink`](crate::SimpleProcessorBuilder::sink).
  pub sink: SinkHandle,

  need_flush: bool,
}

impl<T> SimpleProcessor<T> {
  pub fn new(transformer: T, sink: SinkHandle) -> Self {
    Self {
      transformer,
      sink,
      need_flush: false,
    }
  }
}

impl<T: FnMut(String) -> Option<String> + Send + 'static> Processor for SimpleProcessor<T> {
  async fn process(&mut self, line: String, timestamp: Timestamp) {
    if let Some(transformed) = (self.transformer)(line) {
      self.sink.write_line(transformed, timestamp).await;
      self.need_flush = true;
    }
  }

  async fn truncate(&mut self) {
    if self.need_flush {
      self.sink.flush().await;
      self.need_flush = false;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{SimpleProcessorBuilder, Sink};
  use chrono::DateTime;

  #[tokio::test]
  async fn test_processor_process_default() {
    let sink = Sink::new(tokio_test::io::Builder::new().write(b"hello\n").build()).spawn();
    let mut processor = SimpleProcessorBuilder::new().sink(sink).build();
    processor
      .process("hello".to_string(), mock_timestamp())
      .await;
    processor.truncate().await;
  }

  #[tokio::test]
  async fn test_processor_process_with_transformer() {
    let sink = Sink::new(
      tokio_test::io::Builder::new()
        .write(b"hello")
        .write(b"\n")
        .build(),
    )
    .spawn();
    let mut processor = SimpleProcessorBuilder::new()
      .transformer(|line| (line == "world").then_some(line))
      .sink(sink)
      .build();
    processor
      .process("hello".to_string(), mock_timestamp())
      .await;
    processor
      .process("world".to_string(), mock_timestamp())
      .await;
    processor.truncate().await;
  }

  fn mock_timestamp() -> Timestamp {
    DateTime::from_timestamp(0, 0).unwrap()
  }
}
