use aws_lambda_log_proxy::{LogProxy, Sink};

#[tokio::main]
async fn main() {
  LogProxy::new()
    .simple(|processor| {
      processor
        // only keep lines that contains "done"
        .transformer(|line| line.contains("done").then_some(line))
        // Create a sink to write log lines to stdout.
        .sink(Sink::stdout().spawn())
        .build()
    })
    .start()
    .await;
}
