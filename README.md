# AWS Lambda Log Proxy

[![Crates.io Version](https://img.shields.io/crates/v/aws-lambda-log-proxy?style=flat-square)](https://crates.io/crates/aws-lambda-log-proxy)
![license](https://img.shields.io/github/license/DiscreteTom/aws-lambda-log-proxy?style=flat-square)

![log-flow](./img/log-flow.png)

Filter or transform logs from AWS Lambda functions before they are sent to CloudWatch Logs.

> [!NOTE]
> This is a library for developers. If you are looking for a binary executable, see [AWS Lambda Log Filter](https://github.com/DiscreteTom/aws-lambda-log-filter).

> [!CAUTION]
> Possible data loss if you write tons of logs and return immediately. See [possible data loss](#possible-data-loss) below.

## Usage

### Installation

Add the following to the `dependencies` in your `Cargo.toml`:

```toml
aws-lambda-log-proxy = "0.2"
```

or run:

```bash
cargo add aws-lambda-log-proxy
```

### [Examples](./examples)

A real world case: [AWS Lambda Log Filter](https://github.com/DiscreteTom/aws-lambda-log-filter).

### [Documentation](https://docs.rs/aws-lambda-log-proxy/latest)

## FAQ

### Why We Need This?

We need a solution to realize the following features **_without modifying the existing code_** of AWS Lambda functions:

- Reduce the volume of logs to lower the costs.
- Wrap existing logs in JSON with customizable level and field name, so we can use the [built-in Lambda Log-Level Filtering](https://aws.amazon.com/blogs/compute/introducing-advanced-logging-controls-for-aws-lambda-functions/) to filter them.
- But keep the [EMF](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/CloudWatch_Embedded_Metric_Format_Specification.html) logs untouched so we can still retrieve the metrics.

### How It Works?

We use [AWS Lambda Runtime Proxy](https://github.com/DiscreteTom/aws-lambda-runtime-proxy) to spawn the handler process as a child process of the proxy (and optionally disable the `_LAMBDA_TELEMETRY_LOG_FD` environment variable) to intercept the logs, then suppress the [`invocation/next`](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-api.html#runtimes-api-next) request to process logs as much as possible.

### Why Not Just Grep

Actually you could just create a shell scripts with the content `exec "$@" 2>&1 | grep --line-buffered xxx` and use the [lambda runtime wrapper script](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-modify.html#runtime-wrapper) to realize a simple filter. However we need this lib to realize the following features:

- Override the `_LAMBDA_TELEMETRY_LOG_FD` environment variable to ensure the logs are printed to the stdout/stderr instead of the telemetry log file.
- Suppress the [`invocation/next`](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-api.html#runtimes-api-next) request to process logs as much as possible.

### Possible Data Loss

Though we tried our best to suppress the [`invocation/next`](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-api.html#runtimes-api-next) request to process logs as much as possible, if you write tons of logs (more than thousands of lines) and return immediately, there might be some logs not processed.

As a best practice, it is your responsibility to do a thorough benchmark test against your own use case to ensure the logs are processed as expected.

## [CHANGELOG](./CHANGELOG.md)
