use tokio::io::{AsyncWrite, AsyncWriteExt};

/// Process log lines with [`Self::transformer`]
/// and write them to [`Self::sink`].
pub struct Processor {
  /// See [`Self::transformer`].
  pub transformer: Box<dyn FnMut(String) -> Option<String> + Send>,
  /// See [`Self::sink`].
  pub sink: Box<dyn AsyncWrite + Send + Unpin>,
}

impl Default for Processor {
  fn default() -> Self {
    Processor {
      transformer: Box::new(|s| Some(s)),
      sink: Box::new(tokio::io::stdout()),
    }
  }
}

impl Processor {
  /// Set the log line transformer.
  /// If the transformer returns [`None`], the line will be ignored.
  /// The default transformer will return the input line as is.
  pub fn transformer(mut self, t: impl FnMut(String) -> Option<String> + Send + 'static) -> Self {
    self.transformer = Box::new(t);
    self
  }

  /// Set the transformer to a filter function.
  /// If the filter function returns true, the line will be ignored.
  pub fn filter(self, mut filter: impl FnMut(&str) -> bool + Send + 'static) -> Self {
    self.transformer(move |s| if filter(&s) { None } else { Some(s) })
  }

  // TODO: add wrap_json?
  // TODO: add emf_only_filter?

  /// Set the sink for the processor.
  /// The default sink is [`tokio::io::stdout()`].
  pub fn sink(mut self, s: impl AsyncWrite + Send + Unpin + 'static) -> Self {
    self.sink = Box::new(s);
    self
  }

  /// Set the sink to stdout.
  pub fn to_stdout(self) -> Self {
    self.sink(tokio::io::stdout())
  }

  /// Set the sink to stderr.
  pub fn to_stderr(self) -> Self {
    self.sink(tokio::io::stderr())
  }

  // TODO: add to_lambda_telemetry_log_fd?

  /// Process a log line with the transformer and write it to the sink.
  pub async fn process(&mut self, line: String) {
    if let Some(transformed) = (self.transformer)(line) {
      self.sink.write_all(transformed.as_bytes()).await.unwrap();
    }
  }
}
