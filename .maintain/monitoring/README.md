# Metrics

## Requirements

1. `forest` node running locally
2. `docker` & `docker-compose` must be available in the `$PATH` 

## Run

To run the metrics stack, use the provided Docker compose file to instantiate the `prometheus` and `grafana` processes.

``` sh
$ sudo docker-compose up --build --force-recreate -d
```

This will create a `grafana` process which is preloaded with dashboards which render metrics collected by the `prometheus` process from the `forest` node running locally.
Once the metrics stack is running, open up the `grafana` webapp to view the predefined dashboards.

