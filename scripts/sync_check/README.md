# Nightly check setup
## Prerequisites
* Linux server with decent specs and docker-compose. Battle-tested on:
```
Fedora Linux 36 (Cloud Edition) x86_64
8 vCPUs
16GB / 320GB Disk
```
* `s3fs-fuse` installed,
* Slack webhook: follow the instructions [here](https://api.slack.com/messaging/webhooks) to set up notifications.

## Installation
* Download manually mainnet snapshot and put it in `$HOME/snapshots`. 
* Mount `forest-snapshots` Space. You can use the `mount_snapshot.sh` for this (make sure to setup the credentials beforehand).
* `git clone` the repository and go to this directory. Ensure the defaults in the `.env` file are correct, if not override them (you **must** provide a Slack hook if you want to send notifications).
* If it's the first time setting the `sync_check` you will need to import snapshots. Uncomment relevant lines in the `docker-compose.yml` file. Comment them back after a successful run.
* `docker-compose up -d` to construct the testing suite. It will be re-created in case of a reboot.
