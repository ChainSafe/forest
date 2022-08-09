# Nightly snapshot uploads
## Prerequisites
* Linux server with decent specs. Battle-tested on:
```
Fedora Linux 36 (Cloud Edition) x86_64
8 vCPUs
16GB / 320GB Disk
```
* Docker: https://docs.docker.com/get-docker/
* Slack webhook: follow the instructions [here](https://api.slack.com/messaging/webhooks) to set up notifications.

## Installation
* Put Digital Ocean Spaces password and slack hook in `.env`
* Run `./boot-service`
