# Metrics

## Requirements

1. `forest` node running locally
2. `docker` & `docker-compose` must be available in the `$PATH`
  - https://docs.docker.com/engine/install/ubuntu/
  - https://docs.docker.com/compose/install/

## Run

To run the metrics stack, use the provided Docker compose file to instantiate the `prometheus` and `grafana` processes.

``` sh
# Set up directory to persist Prometheus data
$ sudo mkdir /var/lib/forest
$ sudo chmod -R 777 /var/lib/forest 
$ sudo docker-compose up --build --force-recreate -d
```

This will create a `grafana` process which is preloaded with dashboards which render metrics collected by the `prometheus` process from the `forest` node running locally. The time series database managed by Prometheus will persist data to mounted volume which maps to `/var/lib/forest`on the host.

Once the metrics stack is running, open up the `grafana` webapp to view the predefined dashboards.

## Reload Dashboards

``` sh
$ sudo docker-compose down
$ sudo docker-compose up --build --force-recreate -d
```

