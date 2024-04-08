# CHANGELOG

## v0.2.0

- **_Breaking Change_**: rename `LogProxy.disable_lambda_telemetry_log_fd` to `LogProxy.disable_lambda_telemetry_log_fd_for_handler`.
- Feat: implement `Default` for `OutputFormat`.
- Feat: `Processor.process` will return whether the line is written to the sink.
- Fix: remove `'\r'` in line endings before passed to processor.
- Fix: ignore empty lines before passed to processor.
- Fix: apply tokio biased select to ensure logs are processed before the next invocation.
- Perf: reduce async write calls.

## v0.1.1

- Feat: add `OutputFormat`, `Sink::format`, `Sink::lambda_telemetry_log_fd`.
- Fix: check reader buffers before forwarding `invocation/next` to ensure logs are processed.

## v0.1.0

The initial release.
