use crate::{Processor, Sink};

/// Process log lines with [`Self::transformer`]
/// and write them to [`Self::sink`].
/// To create this, use [`ProcessorBuilder::sink`].
pub struct SimpleProcessor {
  /// See [`ProcessorBuilder::transformer`].
  pub transformer: Box<dyn FnMut(String) -> Option<String> + Send>,
  /// See [`ProcessorBuilder::sink`].
  pub sink: Sink,
}

impl Processor for SimpleProcessor {
  /// Process a log line with [`Self::transformer`] and write it to [`Self::sink`].
  /// `'\n'` will be appended to `line`.
  /// Return `true` if the line is written to the sink (maybe an empty line).
  /// Return `false` if the line is ignored.
  async fn process(&mut self, line: String, timestamp: i64) -> bool {
    if let Some(transformed) = (self.transformer)(line) {
      self.sink.write_line(transformed, timestamp).await;
      true
    } else {
      false
    }
  }

  /// Flush [`Self::sink`].
  async fn flush(&mut self) {
    self.sink.flush().await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::SimpleProcessorBuilder;

  #[tokio::test]
  async fn test_processor_process_default() {
    let sink = Sink::new(tokio_test::io::Builder::new().write(b"hello\n").build());
    let mut processor = SimpleProcessorBuilder::default().sink(sink);
    processor.process("hello".to_string(), 0).await;
  }

  #[tokio::test]
  async fn test_processor_process_with_transformer() {
    let sink = Sink::new(
      tokio_test::io::Builder::new()
        .write(b"hello")
        .write(b"\n")
        .build(),
    );
    let mut processor = SimpleProcessorBuilder::default()
      .ignore(|line| line == "world")
      .sink(sink);
    processor.process("hello".to_string(), 0).await;
    processor.process("world".to_string(), 0).await;
  }
}
