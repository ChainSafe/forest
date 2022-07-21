#!/usr/bin/env bash

# Input: Forest hostname

# Exit codes
RET_CHECK_FAILED=1
RET_HOOK_NOT_SET=2
RET_HOSTNAME_NOT_SET=3

if [ $# -eq 0 ]; then
    echo "No arguments supplied. Need to provide Forest hostname, e.g. forest-mainnet."
    exit "$RET_HOSTNAME_NOT_SET"
else
    FOREST_HOSTNAME=$1
fi

# Hook is needed to send notifications to Slack channel.
# https://api.slack.com/messaging/webhooks
# It should not be kept in source code.
if [ -z "$SLACK_HOOK" ]; then
  echo "Slack hook not set!"
  exit "$RET_HOOK_NOT_SET"
fi

# Directory where the nightly check logs are kept
export SCRIPTS_DIR=/opt/scripts
export LOG_DIR=/opt/logs

DATE=$(date +"%FT%H:%M:%S")
mkdir -p "$LOG_DIR"
export LOG_FILE_CHECK="$LOG_DIR/forest_check_$DATE.log"

function send_success_notification() {
  curl -X POST -H 'Content-type: application/json' --data "{\"text\":\"✅ $FOREST_HOSTNAME check successfully passed! 💪🌲!\"}" "$SLACK_HOOK"
}

function send_failure_notification() {
  curl -X POST -H 'Content-type: application/json' --data "{\"text\":\"❌  $FOREST_HOSTNAME check miserably failed!\n $(tail -n20 "$LOG_FILE_CHECK")\"}" "$SLACK_HOOK"
}

echo "Running the health check..."
if bash "$SCRIPTS_DIR"/health_check.sh "$FOREST_HOSTNAME" > "$LOG_FILE_CHECK" 2>&1
then
  send_success_notification
else
  send_failure_notification
  exit "$RET_CHECK_FAILED"
fi
