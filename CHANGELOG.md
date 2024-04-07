# CHANGELOG

## v0.2.0

- **_Breaking Change_**: rewrite `Sink`, use `SinkBuilder` to build `Sink` instances.
- Feat: implement `Default` for `OutputFormat`.
- Perf: apply actor pattern to `Sink` and add action buffers.

## v0.1.1

- Feat: add `OutputFormat`, `Sink::format`, `Sink::lambda_telemetry_log_fd`.
- Fix: check reader buffers before forwarding `invocation/next` to ensure logs are processed.

## v0.1.0

The initial release.
