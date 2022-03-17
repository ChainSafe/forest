# Nightly check setup
## Prerequisites
* Linux server with decent specs. The nightly check was battle-tested on:
```
Ubuntu 20.04 focal
Intel Xeon E5-2686 v4 @ 8x 2.3GHz
32 GB RAM
```
* Manually compile Forest at least once to make sure all the dependencies are there. Ultimately this check should be done in a containerized environment. To minimize environment impact.
* Slack webhook: follow the instructions [here](https://api.slack.com/messaging/webhooks) to set up notifications.

### Temporary prerequisite (until Forest has full v15 support)
* Download manually mainnet snapshot for V14 and put it in `$HOME/nightly_check/snapshots`. 

## Installation
* Move all scripts from `scripts/nightly_check` to `$HOME/nightly_check/scripts`.
* Set up the cronjob `crontab -e` with
```shell
0 0 * * * SLACK_HOOK=<SLACK_HOOK> bash -l $HOME/nightly_check/scripts/nightly_check.sh
```
