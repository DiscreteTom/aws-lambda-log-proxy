mod builder;
mod mock;
mod simple;
mod sink;

use http::{HeaderMap, HeaderValue};
use std::future::Future;

pub use builder::*;
pub use simple::*;
pub use sink::*;

pub trait Processor: Send + 'static {
  /// Process a log line. The line is guaranteed to be non-empty and does not contain a newline character.
  ///
  /// Return `true` if the line is written to the underlying output stream (maybe an empty line, maybe need flush).
  /// Return `false` if the line is ignored.
  fn process(&mut self, line: String, timestamp: i64) -> impl Future<Output = bool> + Send;

  /// Flushes the underlying output stream, ensuring that all intermediately buffered contents reach their destination.
  fn flush(&mut self) -> impl Future<Output = ()> + Send;

  /// This method is called when a new invocation is about to start (the proxy got the response of `invocation/next`).
  /// See https://docs.aws.amazon.com/lambda/latest/dg/runtimes-api.html#runtimes-api-next for available headers.
  fn next(&mut self, _headers: HeaderMap<HeaderValue>) -> impl Future<Output = ()> + Send {
    // do nothing by default
    async {}
  }

  /// This method is called when the current invocation is about to end (the proxy got the request of `invocation/next`).
  fn truncate(&mut self) -> impl Future<Output = ()> + Send {
    // do nothing by default
    async {}
  }
}
