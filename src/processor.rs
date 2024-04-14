mod builder;
mod mock;
mod simple;
mod sink;

pub use builder::*;
pub use mock::*;
pub use simple::*;
pub use sink::*;

use std::future::Future;

pub trait Processor: Send + 'static {
  /// Process a log line.
  /// `'\n'` will be appended to the `line`.
  /// Return `true` if the line is written to the underlying output stream (maybe an empty line, maybe need flush).
  /// Return `false` if the line is ignored.
  fn process(&mut self, line: String, timestamp: i64) -> impl Future<Output = bool> + Send;
  /// Flushes the underlying output stream, ensuring that all intermediately buffered contents reach their destination.
  fn flush(&mut self) -> impl Future<Output = ()> + Send;
}
