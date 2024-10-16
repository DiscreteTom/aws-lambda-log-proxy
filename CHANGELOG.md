# CHANGELOG

## v0.4.0

- **_Breaking Change_**: `LogProxy::processor` is now `P` instead of `Option<P>`.
- **_Breaking Change_**: rename `LogProxy::processor` to `LogProxy::simple`, add a new `LogProxy::processor` to support custom processors.
- **_Breaking Change_**: `Processor::process` now returns `()` instead of a bool.
- **_Breaking Change_**: remove `Processor::next`. You can register your log proxy process as an external extension to achieve the same effect.
- **_Breaking Change_**: remove `Processor::flush`.
- **_Breaking Change_**: change the type of `timestamp` in `Processor::process` to `Timestamp`.
- **_Breaking Change_**: remove `MockProcessor`, use `()` instead.
- **_Breaking Change_**: make `SimpleProcessor` generic, add `SimpleProcessor::new`, prevent users from creating a `SimpleProcessor` directly.
- **_Breaking Change_**: make `SimpleProcessorBuilder` generic, use `SimpleProcessorBuilder::new` to create a `SimpleProcessorBuilder`, use `SimpleProcessorBuilder::build` to create a `SimpleProcessor`, remove `SimpleProcessorBuilder::ignore/filter`.
- Feat: add `is_emf` util function.
- Feat: apply `tracing`.
- Feat: add `MockSink`.

## v0.3.0

- **_Breaking Change_**: the proxy will read from `stdin` and won't spawn the handler process.
  - Users should use wrapper scripts to redirect the output of the handler process to the proxy process using pipes (`|`).
  - This is to use output redirection (`2>&1`) provided by the system to ensure the log order across `stdout` and `stderr` is correct.
- Feat: add optional `Processor::next` and `Processor::truncate` to indicate the start and the end of the current invocation.

## v0.2.1

- Fix: fix the issue that the log proxy can only write one line per invocation.

## v0.2.0

- **_Breaking Change_**: rename `LogProxy.disable_lambda_telemetry_log_fd` to `LogProxy.disable_lambda_telemetry_log_fd_for_handler`.
- **_Breaking Change_**: rename `Processor` and `ProcessorBuilder` into `SimpleProcessor` and `SimpleProcessorBuilder`.
- **_Breaking Change_**: add trait `Processor`, make `LogProxy` generic.
- **_Breaking Change_**: `Processor::process` need a timestamp as the second argument.
- **_Breaking Change_**: `SimpleProcessor` need a `SinkHandle` instead of a `Sink`.
  - Use `Sink::spawn` to create a `SinkHandle`.
  - Move `write_line` and `flush` to `SinkHandle`.
  - Sink will use a queue to optimize the performance. The queue can be configured by `Sink::buffer_size`.
- Feat: add `LogProxy::new`.
- Feat: add `LogProxy::buffer_size`, store lines in a buffer and record the in-buffer timestamp.
- ~~Feat: add `LogProxy::suppression_timeout_ms` to customize the suppression timeout.~~
  - Removed since it's not working for some runtime (e.g. NodeJS). The handler process might be blocked when runtime API response is suppressed, thus there is no new log lines passed to the log proxy.
- Feat: implement `Default` for `OutputFormat`.
- Feat: add `MockProcessor`.
- Feat: `Processor.process` will return whether the line is written to the sink.
- Fix: remove `'\r'` in line endings before passed to processor.
- Fix: ignore empty lines before passed to processor.
- Fix: apply tokio biased select to ensure logs are processed before the next invocation.
- Perf: reduce async write calls and flush calls.
- Perf: apply AWS Lambda Runtime Proxy v0.2.1.

## v0.1.1

- Feat: add `OutputFormat`, `Sink::format`, `Sink::lambda_telemetry_log_fd`.
- Fix: check reader buffers before forwarding `invocation/next` to ensure logs are processed.

## v0.1.0

The initial release.
