# Metrics

## Requirements

1. `forest` node running locally
2. `docker` & `docker-compose` must be available in the `$PATH` (alternatively
   `podman` and `podman-compose` can be used)
3. Port `3000` for `Grafana` has to be free.
4. Port `3100` for `Loki` has to be free.

## Run

To run the metrics stack, use the provided docker-compose file to spawn the
`prometheus`, `Loki` and `Grafana` containers.

```sh
$ sudo docker-compose up --build --force-recreate -d
# or
$ podman-compose up --build --force-recreate -d
```

This will create a `Grafana` container which is preloaded with `Loki` data
source and dashboards which render metrics collected by the `prometheus`
container from the `forest` node running locally. The time series database
managed by Prometheus will persist data to volume `monitoring_prometheus_data`.

Once the metrics stack is running, open up the `Grafana` webapp to view the
predefined dashboards. Use the default Grafana credentials: `admin`/`admin`.

To receive telemetry via `Loki`, run `forest-daemon` with `--Loki`, then go to
`Configuration/Data Sources` on `Grafana` webapp, select `Loki`, click on
`Explore`, and then you can run queries with `LogQL`. For details of `LogQL`
refer to its [documentation](https://Grafana.com/docs/Loki/latest/logql /).

## Reload Dashboards

Assuming your user is in `docker` group.

```sh
$ docker-compose up --build --force-recreate -d
# or
$ podman-compose up --build --force-recreate -d
```
