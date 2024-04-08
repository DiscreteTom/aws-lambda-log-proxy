mod builder;
mod sink;

pub use builder::*;
pub use sink::*;

/// Process log lines with [`Self::transformer`]
/// and write them to [`Self::sink`].
/// To create this, use [`ProcessorBuilder::sink`].
pub struct Processor {
  /// See [`ProcessorBuilder::transformer`].
  transformer: Box<dyn FnMut(String) -> Option<String> + Send>,
  /// See [`ProcessorBuilder::sink`].
  sink: Sink,
}

impl Processor {
  /// Process a log line with [`Self::transformer`] and write it to [`Self::sink`].
  /// `'\n'` will be appended to `line`. Return `true` if the line is written to the sink.
  pub async fn process(&mut self, line: String) -> bool {
    if let Some(transformed) = (self.transformer)(line) {
      self.sink.write_line(transformed).await;
      true
    } else {
      false
    }
  }

  /// Flush [`Self::sink`].
  pub async fn flush(&mut self) {
    self.sink.flush().await;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_processor_process_default() {
    let sink = SinkBuilder::default()
      .writer(
        tokio_test::io::Builder::new()
          .write(b"hello")
          .write(b"\n")
          .build(),
      )
      .spawn();
    let mut processor = ProcessorBuilder::default().sink(sink);
    processor.process("hello".to_string()).await;
  }

  #[tokio::test]
  async fn test_processor_process_with_transformer() {
    let sink = SinkBuilder::default()
      .writer(
        tokio_test::io::Builder::new()
          .write(b"hello")
          .write(b"\n")
          .build(),
      )
      .spawn();
    let mut processor = ProcessorBuilder::default()
      .ignore(|line| line == "world")
      .sink(sink);
    processor.process("hello".to_string()).await;
    processor.process("world".to_string()).await;
  }
}
