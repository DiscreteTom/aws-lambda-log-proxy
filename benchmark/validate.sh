#!/bin/bash

params=(10 100 1000 10000 100000)
for param in "${params[@]}"
do
  echo "validating: LambdaLogProxy${param}EnabledFunction"
  for ((i=1; i<=10; i++))
  do
    # ensure 'done' is printed
    output=`sam remote invoke "LambdaLogProxy${param}EnabledFunction" --stack-name LambdaLogProxyBenchmark 2>&1 | tail -n 4`
    if echo "$output" | grep -q "done"; then
      echo -n "."
    else
      echo "validation failed, output of last 4 lines:"
      echo "$output"
      exit 1
    fi
  done
  echo "" # append newline
done


echo 'passed!'