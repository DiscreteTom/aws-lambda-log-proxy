mod builder;
mod mock;
mod simple;
mod sink;

use chrono::{DateTime, Utc};
use std::future::Future;

pub use builder::*;
pub use simple::*;
pub use sink::*;

pub type Timestamp = DateTime<Utc>;

pub trait Processor: Send + 'static {
  /// Process a log line. The line is guaranteed to be non-empty and does not contain a newline character.
  ///
  /// The line may be an [EMF](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/CloudWatch_Embedded_Metric_Format_Specification.html) line.
  /// You can use the [`is_emf`](crate::is_emf) util function to check if it is.
  fn process(&mut self, line: String, timestamp: Timestamp) -> impl Future<Output = ()> + Send;

  /// Flushes the underlying output stream, ensuring that all intermediately buffered contents reach their destination.
  /// This method is called when the current invocation is about to end (the proxy got the request of `invocation/next`).
  fn truncate(&mut self) -> impl Future<Output = ()> + Send;
}
