# Benchmark

## Deploy

```bash
# run in the root of the project
RUSTFLAGS="-C link-arg=-s" cargo build --release --target x86_64-unknown-linux-musl --example hello_world
mkdir -p benchmark/layer
cp target/x86_64-unknown-linux-musl/release/examples/hello_world benchmark/layer/
cp benchmark/scripts/entry.sh benchmark/layer/

cd benchmark
sam build
sam deploy # maybe add '-g' for the first time
cd ..
```

In one line

```bash
RUSTFLAGS="-C link-arg=-s" cargo build --release --target x86_64-unknown-linux-musl --example hello_world && mkdir -p benchmark/layer && cp target/x86_64-unknown-linux-musl/release/examples/hello_world benchmark/layer/ && cp benchmark/scripts/entry.sh benchmark/layer/ && cd benchmark && sam build && sam deploy && cd ..
```

## Test

The SAM will deploy the stack with an API. Test it with [`plow`](https://github.com/six-ddc/plow):

```bash
# e.g. disable the layer, test 1000 requests
plow -n 1000 https://abcdefgh.execute-api.us-east-1.amazonaws.com/Prod/disabled
# e.g. enable the layer, test 1000 requests
plow -n 1000 https://abcdefgh.execute-api.us-east-1.amazonaws.com/Prod/enabled
```
