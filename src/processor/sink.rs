use std::sync::Arc;
use tokio::{
  io::{AsyncWrite, AsyncWriteExt},
  sync::Mutex,
};

/// A sink to write log lines to.
/// # Caveats
/// To prevent interleaved output, you should clone a sink
/// instead of creating a new one if you want to write to the same sink.
pub struct Sink(Arc<Mutex<Box<dyn AsyncWrite + Send + Unpin>>>);

impl Clone for Sink {
  fn clone(&self) -> Self {
    Sink(Arc::clone(&self.0))
  }
}

impl Sink {
  /// Create a new sink from an [`AsyncWrite`] implementor.
  pub fn new(s: impl AsyncWrite + Send + Unpin + 'static) -> Self {
    Sink(Arc::new(Mutex::new(Box::new(s))))
  }

  /// Create a new `stdout` sink.
  pub fn stdout() -> Self {
    Sink::new(tokio::io::stdout())
  }

  /// Create a new `stderr` sink.
  pub fn stderr() -> Self {
    Sink::new(tokio::io::stderr())
  }

  // #[cfg(target_os = "linux")]
  // pub fn lambda_telemetry_log_sink() -> Result<Self, Error> {
  //   std::env::var("_LAMBDA_TELEMETRY_LOG_FD")
  //     .map_err(|e| Error::VarError(e))
  //     .and_then(|fd| {
  //       let fd = fd.parse().map_err(|e| Error::ParseIntError(e))?;
  //       Ok(Sink::new(unsafe {
  //         <tokio::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd)
  //       }))
  //     })

  //   // TODO: use specific format to write,
  //   // see https://github.com/aws/aws-lambda-nodejs-runtime-interface-client/blob/2ce88619fd176a5823bc5f38c5484d1cbdf95717/src/LogPatch.js#L90-L101
  // }

  /// Write a buffer to the sink.
  pub async fn write_all(&self, buf: &[u8]) {
    self.0.lock().await.write_all(buf).await.unwrap()
  }

  /// Flush the sink.
  pub async fn flush(&self) {
    self.0.lock().await.flush().await.unwrap()
  }
}

// pub enum Error {
//   VarError(std::env::VarError),
//   ParseIntError(std::num::ParseIntError),
// }
