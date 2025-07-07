# Metrics

## Requirements

1. `docker` & `docker compose` must be available in the `$PATH` (alternatively
   `podman` and `podman-compose` can be used)
2. Port `3000` for `grafana` has to be free.
3. Port `3100` for `loki` has to be free.
4. Variables set in `.env` file must be correct and all configured ports must be free. Sample configuration file is provided as `.env.example`.

## Run

To run the monitored node stack, use the provided Docker compose file. See the compose file for details on the node configuration.

```sh
$ sudo docker compose up --build --force-recreate -d
# or
$ podman-compose up --build --force-recreate -d
```

This will create a `grafana` container which is preloaded with `loki` data
source and dashboards which render metrics collected by the `prometheus`
container from the `forest` container. The time series database
managed by Prometheus will persist data to volume `monitoring_prometheus_data`.

Once the metrics stack is running, open up the `grafana` webapp to view the
predefined dashboards. Use the default Grafana credentials: `admin`/`admin`.

A sample dashboard is available at `Dashboards > Forest`. Loki metrics are available under `Drilldown > Logs` in the Grafana UI.
