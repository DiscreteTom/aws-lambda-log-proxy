use std::{
  env, io,
  num::ParseIntError,
  pin::Pin,
  task::{Context, Poll},
};

use super::Timestamp;
use tokio::{
  io::{AsyncWrite, AsyncWriteExt, Stderr, Stdout},
  sync::{mpsc, oneshot},
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
/// To prevent interleaved output, you should clone a [`SinkHandle`]
/// instead of creating a new one if you want to write to the same sink.
/// # Examples
/// ```
/// use aws_lambda_log_proxy::{Sink, SinkHandle};
///
/// #[tokio::main]
/// async fn main() {
///   let sink: SinkHandle = Sink::stdout().spawn();
///   let sink2 = sink.clone();
/// }
/// ```
pub struct Sink<T: AsyncWrite + Send + Unpin + 'static> {
  writer: T,
  format: OutputFormat,
  buffer_size: usize,
}

impl<T: AsyncWrite + Send + Unpin + 'static> Sink<T> {
  /// Create a new [`Standard`](OutputFormat::Standard) sink from an [`AsyncWrite`] implementor.
  pub fn new(writer: T) -> Self {
    Sink {
      writer,
      format: OutputFormat::Standard,
      buffer_size: 16,
    }
  }

  /// Set the output format of the sink.
  pub fn format(mut self, kind: OutputFormat) -> Self {
    self.format = kind;
    self
  }

  /// Set the buffer size of the sink.
  /// The default buffer size is `16` lines.
  pub fn buffer_size(mut self, size: usize) -> Self {
    self.buffer_size = size;
    self
  }

  /// Spawn the sink and return a [`SinkHandle`] to write to it.
  pub fn spawn(self) -> SinkHandle {
    let (action_tx, mut action_rx) = mpsc::channel(self.buffer_size);

    let mut writer = self.writer;
    tokio::spawn(async move {
      while let Some(action) = action_rx.recv().await {
        match action {
          Action::Write(bytes) => writer.write_all(&bytes).await.unwrap(),
          Action::Flush(ack_tx) => {
            writer.flush().await.unwrap();
            ack_tx.send(()).unwrap();
          }
        }
      }
    });

    SinkHandle {
      action_tx,
      format: self.format,
    }
  }
}

impl Sink<Stdout> {
  /// Create a new [`stdout`](tokio::io::stdout) sink.
  pub fn stdout() -> Self {
    Sink::new(tokio::io::stdout())
  }
}

impl Sink<Stderr> {
  /// Create a new [`stderr`](tokio::io::stderr) sink.
  pub fn stderr() -> Self {
    Sink::new(tokio::io::stderr())
  }
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MockSink;

impl AsyncWrite for MockSink {
  fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, _: &[u8]) -> Poll<io::Result<usize>> {
    Poll::Ready(Ok(0))
  }

  fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
    Poll::Ready(Ok(()))
  }

  fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
    Poll::Ready(Ok(()))
  }
}

impl Sink<MockSink> {
  /// Create a new [`MockSink`] for testing.
  pub fn mock() -> Self {
    Sink::new(MockSink)
  }
}

#[cfg(target_os = "linux")]
impl Sink<tokio::fs::File> {
  /// Create a new sink from the `_LAMBDA_TELEMETRY_LOG_FD` environment variable.
  pub fn lambda_telemetry_log_fd() -> Result<Self, Error> {
    env::var("_LAMBDA_TELEMETRY_LOG_FD")
      .map_err(Error::VarError)
      .and_then(|fd| {
        let fd = fd.parse().map_err(Error::ParseIntError)?;
        Ok(
          Sink::new(unsafe { <tokio::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd) })
            .format(OutputFormat::TelemetryLogFd),
        )
      })
  }
}

enum Action {
  Write(Vec<u8>),
  Flush(oneshot::Sender<()>),
}

/// See [`Sink`].
///
/// This is cheap to clone.
#[derive(Clone)]
pub struct SinkHandle {
  action_tx: mpsc::Sender<Action>,
  format: OutputFormat,
}

impl SinkHandle {
  /// Write a string to the sink with a newline(`'\n'`) appended.
  /// The `timestamp` will be used if the [`Sink::format`] is [`OutputFormat::TelemetryLogFd`].
  /// Bytes are pushed into a queue and might not be written immediately.
  /// You can call [`Self::flush`] to ensure all buffered data is written to the underlying writer.
  pub async fn write_line(&self, s: String, timestamp: Timestamp) {
    let mut line = s.into_bytes();
    line.push(b'\n');

    let bytes = match self.format {
      OutputFormat::Standard => line,
      OutputFormat::TelemetryLogFd => {
        let mut content = build_telemetry_log_fd_format_header(&line, timestamp.timestamp_micros());
        content.append(&mut line);
        content
      }
    };

    self.action_tx.send(Action::Write(bytes)).await.unwrap();
  }

  /// Flush the sink. Wait until all buffered data is written to the underlying writer.
  pub async fn flush(&self) {
    let (ack_tx, ack_rx) = oneshot::channel();
    self.action_tx.send(Action::Flush(ack_tx)).await.unwrap();
    ack_rx.await.unwrap();
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
  VarError(env::VarError),
  ParseIntError(ParseIntError),
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::DateTime;
  use serial_test::serial;

  #[test]
  fn default_sink_format() {
    let builder = Sink::new(Vec::new());
    assert_eq!(builder.format, OutputFormat::Standard);
  }

  #[test]
  #[serial]
  fn sink_stdout_format() {
    let sb = Sink::stdout();
    assert_eq!(sb.format, OutputFormat::Standard);
  }

  #[test]
  #[serial]
  fn sink_stderr_format() {
    let sb = Sink::stderr();
    assert_eq!(sb.format, OutputFormat::Standard);
  }

  #[test]
  #[serial]
  fn sink_format() {
    let sink = Sink::stdout().format(OutputFormat::TelemetryLogFd);
    assert_eq!(sink.format, OutputFormat::TelemetryLogFd);
  }

  #[test]
  #[serial]
  fn sink_buffer_size() {
    let sink = Sink::stdout().buffer_size(32);
    assert_eq!(sink.buffer_size, 32);
  }

  #[cfg(target_os = "linux")]
  #[test]
  #[serial]
  fn sink_lambda_telemetry_log_fd() {
    use std::{fs::File, os::fd::IntoRawFd};

    let file = File::create("/dev/null").unwrap();
    let fd = file.into_raw_fd();
    env::set_var("_LAMBDA_TELEMETRY_LOG_FD", fd.to_string());
    let sink = Sink::lambda_telemetry_log_fd().unwrap();
    assert_eq!(sink.format, OutputFormat::TelemetryLogFd);
    env::remove_var("_LAMBDA_TELEMETRY_LOG_FD");

    // missing env var
    let result = Sink::lambda_telemetry_log_fd();
    assert!(matches!(result, Err(Error::VarError(_))));

    // invalid fd
    env::set_var("_LAMBDA_TELEMETRY_LOG_FD", "invalid");
    let result = Sink::lambda_telemetry_log_fd();
    assert!(matches!(result, Err(Error::ParseIntError(_))));
    env::remove_var("_LAMBDA_TELEMETRY_LOG_FD");
  }

  #[tokio::test]
  async fn sink_write_line() {
    // standard format
    Sink::new(tokio_test::io::Builder::new().write(b"hello\n").build())
      .spawn()
      .write_line("hello".to_string(), mock_timestamp())
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
    .spawn()
    .write_line("hello".to_string(), mock_timestamp())
    .await;
  }

  #[tokio::test]
  async fn sink_flush() {
    let sink = Sink::new(tokio_test::io::Builder::new().write(b"hello\n").build()).spawn();
    sink.write_line("hello".to_string(), mock_timestamp()).await;
    sink.flush().await;
  }

  #[tokio::test]
  async fn sink_handle_clone_able() {
    let sink = Sink::new(
      tokio_test::io::Builder::new()
        .write(b"hello\nworld\n")
        .build(),
    )
    .spawn();
    let sink2 = sink.clone();
    sink.write_line("hello".to_string(), mock_timestamp()).await;
    sink2
      .write_line("world".to_string(), mock_timestamp())
      .await;
  }

  fn mock_timestamp() -> Timestamp {
    DateTime::from_timestamp(0, 0).unwrap()
  }
}
