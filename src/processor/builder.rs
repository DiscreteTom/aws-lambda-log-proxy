use crate::{SimpleProcessor, SinkHandle};

pub struct SimpleProcessorBuilder<T, S> {
  /// See [`Self::transformer`].
  pub transformer: T,
  /// See [`Self::sink`].
  pub sink: S,
}

impl SimpleProcessorBuilder<fn(String) -> Option<String>, ()> {
  pub fn new() -> Self {
    Self {
      transformer: Some,
      sink: (),
    }
  }
}

impl Default for SimpleProcessorBuilder<fn(String) -> Option<String>, ()> {
  fn default() -> Self {
    Self::new()
  }
}

impl<T, S> SimpleProcessorBuilder<T, S> {
  /// Set the log line transformer.
  /// If the transformer returns [`None`], the line will be ignored.
  /// The default transformer will just return `Some(line)`.
  pub fn transformer<N: FnMut(String) -> Option<String> + Send + 'static>(
    self,
    transformer: N,
  ) -> SimpleProcessorBuilder<N, S> {
    SimpleProcessorBuilder {
      transformer,
      sink: self.sink,
    }
  }

  /// Set the sink to write to.
  pub fn sink(self, sink: SinkHandle) -> SimpleProcessorBuilder<T, SinkHandle> {
    SimpleProcessorBuilder {
      transformer: self.transformer,
      sink,
    }
  }
}

impl<T: FnMut(String) -> Option<String> + Send + 'static> SimpleProcessorBuilder<T, SinkHandle> {
  /// Create a new [`SimpleProcessor`] with the given `sink` and [`Self::transformer`].
  pub fn build(self) -> SimpleProcessor<T> {
    SimpleProcessor::new(self.transformer, self.sink)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_transformer() {
    let text = "test".to_string();
    let processor = SimpleProcessorBuilder::default();
    let transformed = (processor.transformer)(text.clone());
    // default transformer should return the input line as is
    assert_eq!(transformed, Some(text));
  }
}
