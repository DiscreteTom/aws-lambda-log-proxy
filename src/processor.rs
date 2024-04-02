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
  pub async fn process(&mut self, line: String) {
    if let Some(transformed) = (self.transformer)(line) {
      self.sink.write_all(transformed.as_bytes()).await;
    }
  }

  /// Flush [`Self::sink`].
  pub async fn flush(&mut self) {
    self.sink.flush().await;
  }
}
