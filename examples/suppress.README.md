# Suppress

## Deploy

```bash
cargo build --examples --release && mkdir -p examples/layer && echo '#!/bin/bash' > examples/layer/entry.sh && echo 'exec /opt/suppress "$@"' >> examples/layer/entry.sh && chmod +x examples/layer/entry.sh && cp target/release/examples/suppress examples/layer && cd examples && sam build -t suppress.yaml && sam deploy -t suppress.yaml --stack-name SuppressedLambdaLogProxyTest --resolve-s3 --capabilities CAPABILITY_IAM && cd ..
```

## Clean

```bash
cd examples && sam delete --stack-name SuppressedLambdaLogProxyTest --no-prompts && cd .. && rm -rf examples/layer && rm -rf examples/.aws-sam
```
