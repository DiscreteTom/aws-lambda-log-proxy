use aws_lambda_log_proxy::{LogProxy, Sink};

#[tokio::main]
async fn main() {
  LogProxy::new()
    .simple(|processor| {
      processor
        // only keep lines that contains "done"
        .filter(|line| line.contains("done"))
        // Create a sink to write log lines to stdout.
        .sink(Sink::stdout().spawn())
    })
    .start()
    .await;
}
