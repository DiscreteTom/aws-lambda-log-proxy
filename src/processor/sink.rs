use std::sync::Arc;
use tokio::{
  io::{AsyncWrite, AsyncWriteExt},
  sync::Mutex,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OutputFormat {
  /// For this variant, log lines are written as bytes and a newline (`'\n'`) will be appended.
  #[default]
  Standard,
  /// For this variant, log lines will be appended with a newline (`'\n'`) and written using the telemetry log format.
  /// See https://github.com/aws/aws-lambda-nodejs-runtime-interface-client/blob/2ce88619fd176a5823bc5f38c5484d1cbdf95717/src/LogPatch.js#L90-L101.
  TelemetryLogFd,
}

/// A sink to write log lines to.
/// # Caveats
/// To prevent interleaved output, you should clone a sink
/// instead of creating a new one if you want to write to the same sink.
/// # Examples
/// ```
/// use aws_lambda_log_proxy::Sink;
///
/// let sink = Sink::stdout();
/// ```
#[derive(Clone)]
pub struct Sink {
  // use arc mutex to prevent interleaved output
  writer: Arc<Mutex<dyn AsyncWrite + Send + Unpin>>,
  format: OutputFormat,
}

impl Sink {
  /// Create a new [`Standard`](OutputFormat::Standard) sink from an [`AsyncWrite`] implementor.
  pub fn new(s: impl AsyncWrite + Send + Unpin + 'static) -> Self {
    Sink {
      writer: Arc::new(Mutex::new(s)),
      format: OutputFormat::Standard,
    }
  }

  /// Create a new [`stdout`](tokio::io::stdout) sink.
  pub fn stdout() -> Self {
    Sink::new(tokio::io::stdout())
  }

  /// Create a new [`stderr`](tokio::io::stderr) sink.
  pub fn stderr() -> Self {
    Sink::new(tokio::io::stderr())
  }

  /// Set the output format of the sink.
  pub fn format(mut self, kind: OutputFormat) -> Self {
    self.format = kind;
    self
  }

  #[cfg(target_os = "linux")]
  /// Create a new sink from the `_LAMBDA_TELEMETRY_LOG_FD` environment variable.
  pub fn lambda_telemetry_log_fd() -> Result<Self, Error> {
    std::env::var("_LAMBDA_TELEMETRY_LOG_FD")
      .map_err(|e| Error::VarError(e))
      .and_then(|fd| {
        let fd = fd.parse().map_err(|e| Error::ParseIntError(e))?;
        Ok(
          Sink::new(unsafe { <tokio::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd) })
            .format(OutputFormat::TelemetryLogFd),
        )
      })
  }

  /// Write a string to the sink with a newline(`'\n'`) appended.
  /// The `timestamp` will be used if the [`Self::format`] is [`OutputFormat::TelemetryLogFd`].
  pub async fn write_line(&self, s: String, timestamp: i64) {
    let mut line = s.into_bytes();
    line.push(b'\n');
    match self.format {
      OutputFormat::Standard => self.writer.lock().await.write_all(&line).await.unwrap(),
      OutputFormat::TelemetryLogFd => {
        let mut content = build_telemetry_log_fd_format_header(&line, timestamp);
        content.append(&mut line);
        self.writer.lock().await.write_all(&content).await.unwrap()
      }
    }
  }

  /// Flush the sink. Wait until all buffered data is written to the underlying writer.
  pub async fn flush(&self) {
    self.writer.lock().await.flush().await.unwrap()
  }
}

fn build_telemetry_log_fd_format_header(line: &[u8], timestamp: i64) -> Vec<u8> {
  // create a 16 bytes buffer to store type and length
  let mut buf = vec![0; 16];
  // the first 4 bytes are 0xa55a0003
  // TODO: what about the level mask? See https://github.com/aws/aws-lambda-nodejs-runtime-interface-client/blob/2ce88619fd176a5823bc5f38c5484d1cbdf95717/src/LogPatch.js#L113
  buf[0..4].copy_from_slice(&0xa55a0003u32.to_be_bytes());
  // the second 4 bytes are the length of the message
  buf[4..8].copy_from_slice(&(line.len() as u32).to_be_bytes());
  // the next 8 bytes are the UNIX timestamp of the message with microseconds precision.
  buf[8..16].copy_from_slice(&timestamp.to_be_bytes());
  buf
}

#[derive(Debug)]
pub enum Error {
  VarError(std::env::VarError),
  ParseIntError(std::num::ParseIntError),
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_sink_format() {
    let builder = Sink::new(Vec::new());
    assert_eq!(builder.format, OutputFormat::Standard);
  }

  #[test]
  fn sink_stdout_format() {
    let sb = Sink::stdout();
    assert_eq!(sb.format, OutputFormat::Standard);
  }

  #[test]
  fn sink_stderr_format() {
    let sb = Sink::stderr();
    assert_eq!(sb.format, OutputFormat::Standard);
  }

  #[test]
  fn sink_format() {
    let sink = Sink::stdout().format(OutputFormat::TelemetryLogFd);
    assert_eq!(sink.format, OutputFormat::TelemetryLogFd);
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn sink_builder_lambda_telemetry_log_fd() {
    std::env::set_var("_LAMBDA_TELEMETRY_LOG_FD", "1");
    let sink = Sink::lambda_telemetry_log_fd().unwrap();
    assert_eq!(sink.format, OutputFormat::TelemetryLogFd);
    std::env::remove_var("_LAMBDA_TELEMETRY_LOG_FD");

    // missing env var
    let result = Sink::lambda_telemetry_log_fd();
    assert!(matches!(result, Err(Error::VarError(_))));

    // invalid fd
    std::env::set_var("_LAMBDA_TELEMETRY_LOG_FD", "invalid");
    let result = Sink::lambda_telemetry_log_fd();
    assert!(matches!(result, Err(Error::ParseIntError(_))));
    std::env::remove_var("_LAMBDA_TELEMETRY_LOG_FD");
  }

  #[tokio::test]
  async fn sink_write_line() {
    // standard format
    Sink::new(tokio_test::io::Builder::new().write(b"hello\n").build())
      .write_line("hello".to_string(), 0)
      .await;

    // telemetry log format header
    assert_eq!(
      build_telemetry_log_fd_format_header("hello\n".as_bytes(), 0),
      &[0xa5, 0x5a, 0x00, 0x03, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0]
    );

    // telemetry log format
    Sink::new(
      tokio_test::io::Builder::new()
        .write(b"\xa5\x5a\x00\x03\x00\x00\x00\x06\x00\x00\x00\x00\x00\x00\x00\x00hello\n")
        .build(),
    )
    .format(OutputFormat::TelemetryLogFd)
    .write_line("hello".to_string(), 0)
    .await;
  }
}
