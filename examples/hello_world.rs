use aws_lambda_log_proxy::{LogProxy, SinkBuilder};

#[tokio::main]
async fn main() {
  // Create a sink to write log lines to stdout.
  let sink = SinkBuilder::default().stdout().spawn();

  LogProxy::default()
    .stdout(|processor| {
      processor
        // only keep lines that contains "hello"
        .filter(|line| line.contains("hello"))
        // clone the sink instead of creating a new one
        // to prevent interleaved output
        .sink(sink.clone())
    })
    .stderr(|processor| {
      processor
        // prepend "ERROR: " to each line
        .transformer(|line| Some(format!("ERROR: {line}")))
        // also write to the `stdout` sink
        .sink(sink)
    })
    .start()
    .await;
}
