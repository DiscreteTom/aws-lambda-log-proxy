# AWS Lambda Log Proxy

[![Crates.io Version](https://img.shields.io/crates/v/aws-lambda-log-proxy?style=flat-square)](https://crates.io/crates/aws-lambda-log-proxy)
![license](https://img.shields.io/github/license/DiscreteTom/aws-lambda-log-proxy?style=flat-square)

![log-flow](./img/log-flow.png)

Filter / transform / forward logs from AWS Lambda functions before they are sent to CloudWatch Logs.

> [!NOTE]
> This is a library for developers. If you are looking for a binary executable, see [AWS Lambda Log Filter](https://github.com/DiscreteTom/aws-lambda-log-filter).

> [!CAUTION]
> Possible data loss if you write tons of logs and return immediately. See [possible data loss](#possible-data-loss) below.

## Usage

### Installation

```bash
cargo add aws-lambda-log-proxy
```

### Examples

See [examples](./examples) for more details.

A real world case: [AWS Lambda Log Filter](https://github.com/DiscreteTom/aws-lambda-log-filter).

### [Documentation](https://docs.rs/aws-lambda-log-proxy/latest)

## FAQ

### Why We Need This?

We need a solution to realize the following features **_without modifying the code_** of your existing AWS Lambda functions:

- Reduce the volume of logs to lower the costs.
- Forward logs to other destinations (like Elasticsearch) in real time.
- But keep the [EMF](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/CloudWatch_Embedded_Metric_Format_Specification.html) lines untouched so Amazon CloudWatch can still retrieve the metrics.

### How It Works?

You need a [wrapper script](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-modify.html) to start the handler process (with the `_LAMBDA_TELEMETRY_LOG_FD` environment variable removed) and pipe the logs to the proxy. The proxy will filter / transform / forward the logs before they are sent to CloudWatch Logs.

We use [AWS Lambda Runtime Proxy](https://github.com/DiscreteTom/aws-lambda-runtime-proxy) to intercept AWS Lambda runtime API requests so we can suppress the [`invocation/next`](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-api.html#runtimes-api-next) request to process logs as much as possible. You need to override the `AWS_LAMBDA_RUNTIME_API` environment variable for your handler function to point to the runtime proxy in the wrapper script.

### Why Not Just Grep

Actually you could just create a shell scripts with the content `env -u _LAMBDA_TELEMETRY_LOG_FD "$@" 2>&1 | grep --line-buffered xxx` and use a [wrapper script](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-modify.html#runtime-wrapper) to realize a simple filter. However we need this lib to realize the following features:

- Suppress the [`invocation/next`](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-api.html#runtimes-api-next) request to process logs as much as possible. This is very important if you want to stream the logs to external services in real time.
- Identify the log lines with [EMF](https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/CloudWatch_Embedded_Metric_Format_Specification.html) so we can keep them untouched.
- Implement custom filters / transformers / forwarders in Rust.

### Possible Data Loss

Though we tried our best to suppress the [`invocation/next`](https://docs.aws.amazon.com/lambda/latest/dg/runtimes-api.html#runtimes-api-next) request to process logs as much as possible, if you write tons of logs (more than thousands of lines) and return immediately, there might be some logs not processed.

As a best practice, it is your responsibility to do a thorough benchmark test against your own use case to ensure the logs are processed as expected.

## [CHANGELOG](./CHANGELOG.md)
