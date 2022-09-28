# Nightly snapshot uploads

This service will continuously verify that Forest can export snapshots. Once per
day, this service will sync to calibnet and export a new snapshot. If the
previous snapshot is more than a day old, the new snapshot is uploaded to
Digital Ocean Spaces.

## Prerequisites
* Linux server with decent specs. Battle-tested on:
```
Fedora Linux 36 (Cloud Edition) x86_64
8 vCPUs
16GB / 320GB Disk
```
* Docker: https://docs.docker.com/get-docker/
* Slack api token.

## Installation
* Put Digital Ocean Spaces password and slack api token in `.env`
* Run `./boot-service`
