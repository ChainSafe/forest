# Nightly snapshot uploads

This service will continuously verify that Forest can export snapshots. Every
time the Forest docker image is changed (or at least twice a day), this service
will sync to calibnet and export a new snapshot. If the previous snapshot is
more than a day old, the new snapshot is uploaded to Digital Ocean Spaces.

## Prerequisites
* Linux server with decent specs. Battle-tested on:
```
Fedora Linux 36 (Cloud Edition) x86_64
8 vCPUs
16GB / 320GB Disk
```
* Docker: https://docs.docker.com/get-docker/
* The `screen` program.
* Slack webhook: follow the instructions [here](https://api.slack.com/messaging/webhooks) to set up notifications.

## Installation
* Put Digital Ocean Spaces password and slack hook in `.env`
* Run `./boot-service`
