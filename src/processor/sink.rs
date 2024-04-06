use std::sync::Arc;
use tokio::{
  io::{AsyncWrite, AsyncWriteExt},
  sync::{Mutex, MutexGuard},
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

  /// Write a string to the sink then write a newline(`'\n'`).
  pub async fn write_line(&self, s: String) {
    let mut f = self.writer.lock().await;
    match self.format {
      OutputFormat::Standard => {}
      OutputFormat::TelemetryLogFd => {
        write_telemetry_log_fd_format_header(&mut f, &s, chrono::Utc::now().timestamp_micros())
          .await
      }
    }
    f.write_all(s.as_bytes()).await.unwrap();
    f.write_all(b"\n").await.unwrap();
  }

  /// Flush the sink.
  pub async fn flush(&self) {
    self.writer.lock().await.flush().await.unwrap()
  }
}

async fn write_telemetry_log_fd_format_header<'a>(
  f: &mut MutexGuard<'a, dyn AsyncWrite + Send + Unpin>,
  s: &str,
  timestamp: i64,
) {
  // create a 16 bytes buffer to store type and length
  let mut buf = [0; 16];
  // the first 4 bytes are 0xa55a0003
  // TODO: what about the level mask? See https://github.com/aws/aws-lambda-nodejs-runtime-interface-client/blob/2ce88619fd176a5823bc5f38c5484d1cbdf95717/src/LogPatch.js#L113
  buf[0..4].copy_from_slice(&0xa55a0003u32.to_be_bytes());
  // the second 4 bytes are the length of the message
  let len = s.len() as u32 + 1; // 1 for the last newline
  buf[4..8].copy_from_slice(&len.to_be_bytes());
  // the next 8 bytes are the UNIX timestamp of the message with microseconds precision.
  buf[8..16].copy_from_slice(&timestamp.to_be_bytes());
  // write the buffer
  f.write_all(&buf).await.unwrap();
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
  fn test_sink_new() {
    let sink = Sink::new(tokio::io::stdout());
    assert_eq!(sink.format, OutputFormat::Standard);
  }

  #[test]
  fn test_sink_stdout() {
    let sink = Sink::stdout();
    assert_eq!(sink.format, OutputFormat::Standard);
  }

  #[test]
  fn test_sink_stderr() {
    let sink = Sink::stderr();
    assert_eq!(sink.format, OutputFormat::Standard);
  }

  #[test]
  fn test_sink_format() {
    let sink = Sink::new(tokio::io::stdout()).format(OutputFormat::TelemetryLogFd);
    assert_eq!(sink.format, OutputFormat::TelemetryLogFd);
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn test_sink_lambda_telemetry_log_fd() {
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
  async fn test_sink_write_line() {
    // standard format
    Sink::new(
      tokio_test::io::Builder::new()
        .write(b"hello")
        .write(b"\n")
        .build(),
    )
    .write_line("hello".to_string())
    .await;

    // telemetry log format
    let sink = Sink::new(
      tokio_test::io::Builder::new()
        .write(&[0xa5, 0x5a, 0x00, 0x03])
        .write(&[0, 0, 0, 6]) // length is 6, 5 for "hello" and 1 for newline
        .write(&[0, 0, 0, 0, 0, 0, 0, 0])
        .build(),
    )
    .format(OutputFormat::TelemetryLogFd);
    write_telemetry_log_fd_format_header(&mut sink.writer.lock().await, "hello", 0).await;
  }
}
