#!/bin/bash

echo "$@"

exec /opt/aws-lambda-log-filter "$@"