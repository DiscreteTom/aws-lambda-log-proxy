use aws_lambda_log_proxy::{LogProxy, Sink};

#[tokio::main]
async fn main() {
  let sink = Sink::stdout();

  LogProxy::default()
    .stdout(|p| p.filter(|l| l.starts_with("allow")).sink(sink.clone()))
    .stderr(|p| p.filter(|l| l.starts_with("allow")).sink(sink))
    .start()
    .await;
}
