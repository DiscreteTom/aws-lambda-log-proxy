use crate::{SimpleProcessor, SinkHandle};

pub struct SimpleProcessorBuilder<T, S> {
  /// See [`Self::transformer`].
  pub transformer: T,
  /// See [`Self::sink`].
  pub sink: S,
}

impl SimpleProcessorBuilder<fn(String) -> Option<String>, ()> {
  /// Create a new [`SimpleProcessorBuilder`] with the default transformer and no sink.
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

  macro_rules! assert_unit {
    ($unit:expr) => {
      let _: () = $unit;
    };
  }

  #[test]
  fn default_transformer() {
    let processor = SimpleProcessorBuilder::default();
    assert_unit!(processor.sink);

    let text = "test".to_string();
    let transformed = (processor.transformer)(text.clone());
    // default transformer should return the input line as is
    assert_eq!(transformed, Some(text));
  }
}
