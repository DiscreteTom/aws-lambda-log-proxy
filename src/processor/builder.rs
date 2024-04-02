use crate::{Processor, Sink};

pub struct ProcessorBuilder {
  /// See [`Self::transformer`].
  pub transformer: Box<dyn FnMut(String) -> Option<String> + Send>,
}

impl Default for ProcessorBuilder {
  fn default() -> Self {
    ProcessorBuilder {
      transformer: Box::new(|s| Some(s)),
    }
  }
}

impl ProcessorBuilder {
  /// Set the log line transformer.
  /// If the transformer returns [`None`], the line will be ignored.
  /// The default transformer will return the input line as is.
  pub fn transformer(mut self, t: impl FnMut(String) -> Option<String> + Send + 'static) -> Self {
    self.transformer = Box::new(t);
    self
  }

  /// Set the transformer to a filter function.
  /// If the filter function returns `true`, the line will be dropped,
  /// otherwise it will be kept.
  pub fn ignore(self, mut filter: impl FnMut(&str) -> bool + Send + 'static) -> Self {
    self.transformer(move |s| if filter(&s) { None } else { Some(s) })
  }

  /// Set the transformer to a filter function.
  /// If the filter function returns `true`, the line will be kept,
  /// otherwise it will be dropped.
  pub fn filter(self, mut filter: impl FnMut(&str) -> bool + Send + 'static) -> Self {
    self.ignore(move |line| !filter(line))
  }

  /// Create a new [`Processor`] with the given `sink` and [`Self::transformer`].
  pub fn sink(self, sink: Sink) -> Processor {
    Processor {
      transformer: self.transformer,
      sink,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_transformer() {
    let text = "test".to_string();
    let mut processor = ProcessorBuilder::default();
    let transformed = (processor.transformer)(text.clone());
    // default transformer should return the input line as is
    assert_eq!(transformed, Some(text));
  }

  #[test]
  fn ignore_filter() {
    let text = "test".to_string();
    let mut processor = ProcessorBuilder::default().ignore(|_| true);
    let transformed = (processor.transformer)(text.clone());
    // ignore filter should return None
    assert_eq!(transformed, None);

    let mut processor = ProcessorBuilder::default().ignore(|_| false);
    let transformed = (processor.transformer)(text.clone());
    // ignore filter should return the input line as is
    assert_eq!(transformed, Some(text));
  }

  #[test]
  fn filter_filter() {
    let text = "test".to_string();
    let mut processor = ProcessorBuilder::default().filter(|_| false);
    let transformed = (processor.transformer)(text.clone());
    // filter filter should return None
    assert_eq!(transformed, None);

    let mut processor = ProcessorBuilder::default().filter(|_| true);
    let transformed = (processor.transformer)(text.clone());
    // filter filter should return the input line as is
    assert_eq!(transformed, Some(text));
  }
}
