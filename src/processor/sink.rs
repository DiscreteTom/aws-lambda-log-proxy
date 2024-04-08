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

enum Action {
  WriteLine(Vec<u8>),
  Flush(oneshot::Sender<()>),
}

/// See [`Sink`].
pub struct SinkBuilder<T> {
  writer: T,
  /// See [`Self::format`].
  format: OutputFormat,
  /// See [`Self::buffer_size`].
  buffer_size: usize,
}

impl Default for SinkBuilder<()> {
  fn default() -> Self {
    SinkBuilder {
      writer: (),
      format: OutputFormat::Standard,
      buffer_size: 64,
    }
  }
}

impl<T> SinkBuilder<T> {
  /// Set the output format of the sink. Defaults to [`OutputFormat::Standard`].
  pub fn format(mut self, kind: OutputFormat) -> Self {
    self.format = kind;
    self
  }

  /// Set the action buffer size of the sink. Defaults to `64`.
  pub fn buffer_size(mut self, size: usize) -> Self {
    self.buffer_size = size;
    self
  }

  /// Set the writer of the sink to an [`AsyncWrite`] implementor.
  pub fn writer<W: AsyncWrite + Send + Unpin + 'static>(self, writer: W) -> SinkBuilder<W> {
    SinkBuilder {
      writer,
      format: self.format,
      buffer_size: self.buffer_size,
    }
  }

  /// Set the writer to [`tokio::io::stdout`].
  pub fn stdout(self) -> SinkBuilder<Stdout> {
    self.writer(tokio::io::stdout())
  }

  /// Set the writer to [`tokio::io::stderr`].
  pub fn stderr(self) -> SinkBuilder<Stderr> {
    self.writer(tokio::io::stderr())
  }

  #[cfg(target_os = "linux")]
  /// Set the writer by the `_LAMBDA_TELEMETRY_LOG_FD` environment variable
  /// and set the output format to [`OutputFormat::TelemetryLogFd`].
  pub fn lambda_telemetry_log_fd(self) -> Result<SinkBuilder<tokio::fs::File>, Error> {
    std::env::var("_LAMBDA_TELEMETRY_LOG_FD")
      .map_err(|e| Error::VarError(e))
      .and_then(|fd| {
        let fd = fd.parse().map_err(|e| Error::ParseIntError(e))?;
        Ok(
          self
            .format(OutputFormat::TelemetryLogFd)
            .writer(unsafe { <tokio::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fd) }),
        )
      })
  }

  /// Build a sink and spawn the writer task.
  /// Make sure there is a tokio runtime running before calling this method.
  /// The writer task will be terminated when the sink is dropped.
  pub fn spawn(self) -> Sink
  where
    T: AsyncWrite + Send + Unpin + 'static,
  {
    let (action_tx, mut action_rx) = mpsc::channel(self.buffer_size);

    let format = self.format;
    let mut writer = self.writer;

    tokio::spawn(async move {
      while let Some(action) = action_rx.recv().await {
        match action {
          Action::WriteLine(line) => {
            writer.write_all(&line).await.unwrap();
          }
          Action::Flush(ack_tx) => {
            writer.flush().await.unwrap();
            ack_tx.send(()).unwrap();
          }
        }
      }
    });

    Sink { action_tx, format }
  }
}

/// A sink to write log lines to.
/// # Caveats
/// To prevent interleaved output, you should clone a sink
/// instead of creating a new one if you want to write to the same sink.
/// # Examples
/// ```
/// use aws_lambda_log_proxy::{SinkBuilder, Sink};
///
/// #[tokio::main]
/// async fn main() {
///   let sink: Sink = SinkBuilder::default().stdout().spawn();
/// }
/// ```
#[derive(Clone)]
pub struct Sink {
  action_tx: mpsc::Sender<Action>,
  format: OutputFormat,
}

impl Sink {
  /// Write a string to the sink with a newline(`'\n'`) appended.
  pub async fn write_line(&self, s: String) {
    let mut line = s.into_bytes();
    line.push(b'\n');
    match self.format {
      OutputFormat::Standard => self.action_tx.send(Action::WriteLine(line)).await.unwrap(),
      OutputFormat::TelemetryLogFd => {
        let mut content =
          build_telemetry_log_fd_format_header(&line, chrono::Utc::now().timestamp_micros());
        content.append(&mut line);
        self
          .action_tx
          .send(Action::WriteLine(content))
          .await
          .unwrap()
      }
    }
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
  VarError(std::env::VarError),
  ParseIntError(std::num::ParseIntError),
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_sink_builder() {
    let builder = SinkBuilder::default();
    assert_eq!(builder.writer, ());
    assert_eq!(builder.format, OutputFormat::Standard);
    assert_eq!(builder.buffer_size, 64);
  }

  #[test]
  fn sink_builder_format() {
    let builder = SinkBuilder::default().format(OutputFormat::TelemetryLogFd);
    assert_eq!(builder.format, OutputFormat::TelemetryLogFd);
  }

  #[test]
  fn sink_builder_buffer_size() {
    let builder = SinkBuilder::default().buffer_size(128);
    assert_eq!(builder.buffer_size, 128);
  }

  #[tokio::test]
  async fn sink_builder_writer() {
    let _: Sink = SinkBuilder::default().writer(Vec::new()).spawn();
  }

  #[tokio::test]
  async fn sink_builder_stdout() {
    let sb = SinkBuilder::default().stdout();
    assert_eq!(sb.format, OutputFormat::Standard);
    let _: Sink = sb.spawn();
  }

  #[tokio::test]
  async fn sink_builder_stderr() {
    let sb = SinkBuilder::default().stderr();
    assert_eq!(sb.format, OutputFormat::Standard);
    let _: Sink = sb.spawn();
  }

  #[cfg(target_os = "linux")]
  #[tokio::test]
  async fn sink_builder_lambda_telemetry_log_fd() {
    std::env::set_var("_LAMBDA_TELEMETRY_LOG_FD", "1");
    let sb = SinkBuilder::default().lambda_telemetry_log_fd().unwrap();
    assert_eq!(sb.format, OutputFormat::TelemetryLogFd);
    let _: Sink = sb.spawn();
    std::env::remove_var("_LAMBDA_TELEMETRY_LOG_FD");

    // missing env var
    let result = SinkBuilder::default().lambda_telemetry_log_fd();
    assert!(matches!(result, Err(Error::VarError(_))));

    // invalid fd
    std::env::set_var("_LAMBDA_TELEMETRY_LOG_FD", "invalid");
    let result = SinkBuilder::default().lambda_telemetry_log_fd();
    assert!(matches!(result, Err(Error::ParseIntError(_))));
    std::env::remove_var("_LAMBDA_TELEMETRY_LOG_FD");
  }

  #[tokio::test]
  async fn sink_write_line() {
    // standard format
    SinkBuilder::default()
      .writer(tokio_test::io::Builder::new().write(b"hello\n").build())
      .spawn()
      .write_line("hello".to_string())
      .await;

    // telemetry log format header
    assert_eq!(
      build_telemetry_log_fd_format_header("hello\n".as_bytes(), 0),
      &[0xa5, 0x5a, 0x00, 0x03, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0]
    );
  }
}
