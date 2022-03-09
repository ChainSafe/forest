# Metrics

## Requirements

1. `forest` node running locally
2. `docker` & `docker-compose` must be available in the `$PATH`
3. Ports `3000` (for `grafana`) and `9090` (for `prometheus`) have to be free.

## Run

To run the metrics stack, use the provided Docker compose file to spawn the `prometheus` and `grafana` containers.

``` sh
$ sudo docker-compose up --build --force-recreate -d
```

This will create a `grafana` container which is preloaded with dashboards which render metrics collected by the `prometheus` container from the `forest` node running locally. The time series database managed by Prometheus will persist data to volume `monitoring_prometheus_data`.

Once the metrics stack is running, open up the `grafana` webapp to view the predefined dashboards. Use the default Grafana credentials: `admin`/`admin`.

## Reload Dashboards

Assuming your user is in `docker` group.

``` sh
$ docker-compose up --build --force-recreate -d
```

