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
  fn process(&mut self, line: String, timestamp: i64) -> impl Future<Output = bool> + Send;
  fn flush(&mut self) -> impl Future<Output = ()> + Send;
}
