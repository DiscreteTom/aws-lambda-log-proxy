use aws_lambda_log_proxy::{LogProxy, Sink};

#[tokio::main]
async fn main() {
  // Create a sink to write log lines to stdout.
  let sink = Sink::stdout().spawn();

  LogProxy::new()
    .stdout(|processor| {
      processor
        // only keep lines that contains "done"
        .filter(|line| line.contains("done"))
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
