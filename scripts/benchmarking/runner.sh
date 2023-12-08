#!/bin/bash
# This script is run the benchmark script.

docker build --tag forest_bench .
docker run -it forest_bench