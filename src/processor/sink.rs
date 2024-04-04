use std::sync::Arc;
use tokio::{
  io::{AsyncWrite, AsyncWriteExt},
  sync::Mutex,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputFormat {
  /// For this variant, log lines are written as bytes and a newline (`'\n'`) will be appended.
  Standard,
  /// For this variant, log lines will be appended with a newline (`'\n'`) and written using the telemetry log format.
  /// See https://github.com/aws/aws-lambda-nodejs-runtime-interface-client/blob/2ce88619fd176a5823bc5f38c5484d1cbdf95717/src/LogPatch.js#L90-L101.
  TelemetryLogFd,
}

/// A sink to write log lines to.
/// # Caveats
/// To prevent interleaved output, you should clone a sink
/// instead of creating a new one if you want to write to the same sink.
pub struct Sink(Arc<Mutex<Box<dyn AsyncWrite + Send + Unpin>>>, OutputFormat);

impl Clone for Sink {
  fn clone(&self) -> Self {
    Sink(Arc::clone(&self.0), self.1.clone())
  }
}

impl Sink {
  /// Create a new [`Standard`](OutputFormat::Standard) sink from an [`AsyncWrite`] implementor.
  pub fn new(s: impl AsyncWrite + Send + Unpin + 'static) -> Self {
    Sink(Arc::new(Mutex::new(Box::new(s))), OutputFormat::Standard)
  }

  /// Create a new `stdout` sink.
  pub fn stdout() -> Self {
    Sink::new(tokio::io::stdout())
  }

  /// Create a new `stderr` sink.
  pub fn stderr() -> Self {
    Sink::new(tokio::io::stderr())
  }

  /// Set the output format of the sink.
  pub fn format(mut self, kind: OutputFormat) -> Self {
    self.1 = kind;
    self
  }

  #[cfg(target_os = "linux")]
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
    let mut f = self.0.lock().await;
    match self.1 {
      OutputFormat::Standard => {
        f.write_all(s.as_bytes()).await.unwrap();
        f.write_all(b"\n").await.unwrap();
      }
      OutputFormat::TelemetryLogFd => {
        // create a 16 bytes buffer to store type and length
        let mut buf = [0; 16];
        // the first 4 bytes are 0xa55a0003
        // TODO: what about the level mask?
        buf[0..4].copy_from_slice(&0xa55a0003u32.to_be_bytes());
        // the second 4 bytes are the length of the message
        let len = s.len() as u32 + 1; // 1 for the last newline
        buf[4..8].copy_from_slice(&len.to_be_bytes());
        // the next 8 bytes are the UNIX timestamp of the message with microseconds precision.
        let timestamp = chrono::Utc::now().timestamp_micros();
        buf[8..16].copy_from_slice(&timestamp.to_be_bytes());
        // write the buffer
        f.write_all(&buf).await.unwrap();
        f.write_all(s.as_bytes()).await.unwrap();
        f.write_all(b"\n").await.unwrap();
      }
    }
  }

  /// Flush the sink.
  pub async fn flush(&self) {
    self.0.lock().await.flush().await.unwrap()
  }
}

#[derive(Debug)]
pub enum Error {
  VarError(std::env::VarError),
  ParseIntError(std::num::ParseIntError),
}
