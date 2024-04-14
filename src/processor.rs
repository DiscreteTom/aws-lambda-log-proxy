mod builder;
mod simple;
mod sink;

use std::future::Future;

pub use builder::*;
pub use simple::*;
pub use sink::*;

pub trait Processor: Send + 'static {
  fn process(&mut self, line: String, timestamp: i64) -> impl Future<Output = bool> + Send;
  fn flush(&mut self) -> impl Future<Output = ()> + Send;
}

impl Processor for () {
  fn process(&mut self, _line: String, _timestamp: i64) -> impl Future<Output = bool> {
    async { false }
  }

  fn flush(&mut self) -> impl Future<Output = ()> {
    async {}
  }
}
