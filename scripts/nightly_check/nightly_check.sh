#!/usr/bin/env bash

# Exit codes
RET_SETUP_FAILED=1
RET_CHECK_FAILED=2
RET_HOOK_NOT_SET=3

# Hook is needed to send notifications to Slack channel.
# https://api.slack.com/messaging/webhooks
# It should not be kept in source code.
if [ -z "$SLACK_HOOK" ]; then
  echo "Slack hook not set!"
  exit "$RET_HOOK_NOT_SET"
fi

# Root directory for nightly checks
export CHECK_DIR="$HOME"/nightly_check
# Directory where the snapshots are kept
export SNAPSHOT_DIR="$CHECK_DIR"/snapshots
# Directory where the nightly check logs are kept
export LOG_DIR="$CHECK_DIR"/logs
# Directory where the check scripts (including this one) are kept
export SCRIPTS_DIR="$CHECK_DIR"/scripts

DATE=$(date +"%FT%H:%M:%S")
mkdir -p "$LOG_DIR"
export LOG_FILE_BUILD="$LOG_DIR/forest_build_$DATE.log"
export LOG_FILE_RUN="$LOG_DIR/forest_run_$DATE.log"
export LOG_FILE_CHECK="$LOG_DIR/forest_check_$DATE.log"


function send_success_notification() {
  curl -X POST -H 'Content-type: application/json' --data "{\"text\":\"✅ Nightly check successfully passed! 💪🌲!\n $(tail -n20 "$1")\"}" "$SLACK_HOOK"
}

function send_failure_notification() {
  curl -X POST -H 'Content-type: application/json' --data "{\"text\":\"❌  Nightly check miserably failed!\n $(tail -n20 "$1")\"}" "$SLACK_HOOK"
}

function cleanup() {
  echo "🧹Cleaning the environment..."
  pgrep forest | xargs kill
  rm -rf "$HOME"/.forest
  rm -rf "$CHECK_DIR"/forest
}

echo "Preparing the test..."
cleanup
bash "$SCRIPTS_DIR"/nightly_check_prepare.sh > "$LOG_FILE_BUILD" 2>&1 || {
  send_failure_notification "$(<"$LOG_FILE_BUILD")"
  cleanup
  exit "$RET_SETUP_FAILED"
}

echo "Running the health check..."
if bash "$SCRIPTS_DIR"/health_check.sh > "$LOG_FILE_CHECK" 2>&1
then
  send_success_notification "$(<"$LOG_FILE_CHECK")"
else
  send_failure_notification "$(<"$LOG_FILE_CHECK")"
  cleanup
  exit "$RET_CHECK_FAILED"
fi

cleanup
