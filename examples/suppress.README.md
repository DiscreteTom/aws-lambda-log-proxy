# Suppress

Even if your log processor is slow, the log proxy can suppress the `invocation/next` until the log line is processed.

In this example we will suppress the `invocation/next` for 2 seconds. Synchronous invoker of the lambda function (API Gateway in this demo) will get the response immediately but the lambda will run for 2 more seconds to process the log lines.

## Deploy

```bash
rm -rf ./layer
mkdir -p ./layer

cargo build --examples --release
cp ../target/release/examples/suppress ./layer

echo '#!/bin/bash' > ./layer/entry.sh
echo 'env AWS_LAMBDA_RUNTIME_API=127.0.0.1:3000 env -u _LAMBDA_TELEMETRY_LOG_FD "$@" 2>&1 | /opt/suppress' >> ./layer/entry.sh
chmod +x ./layer/entry.sh

sam build -t suppress.yaml
sam deploy --stack-name SuppressedLambdaLogProxyTest --resolve-s3 --capabilities CAPABILITY_IAM
```

## Test

```bash
curl <api-url>
```

You should see the response immediately but the lambda will run for 2 more seconds to process the log lines.

## Clean

```bash
sam delete --stack-name SuppressedLambdaLogProxyTest --no-prompts
```
