#!/bin/bash

env AWS_LAMBDA_RUNTIME_API=127.0.0.1:3000 env -u _LAMBDA_TELEMETRY_LOG_FD "$@" 2>&1 | /opt/hello_world