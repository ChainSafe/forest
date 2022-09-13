# frozen_string_literal: true

require 'cs_utils/slack_client'
require 'cs_utils/docker_utils'
require 'logger'
require 'fileutils'

# Retrieves an environmental variable, failing if its not set or empty.
def get_and_assert_env_variable(name)
  var = ENV[name]
  raise "Please set #{name} environmental variable" if var.nil? || var.empty?

  var
end

SLACK_TOKEN = get_and_assert_env_variable 'SLACK_API_TOKEN'
CHANNEL = get_and_assert_env_variable 'SLACK_NOTIF_CHANNEL'
SCRIPTS_DIR = get_and_assert_env_variable 'SCRIPTS_DIR'
LOG_DIR = get_and_assert_env_variable 'LOG_DIR'

hostname = ARGV[0]
raise 'No arguments supplied. Please provide Forest hostname, e.g. forest-mainnet' if ARGV.empty?

# Current datetime, to append to the log files
DATE = Time.new.strftime '%FT%H:%M:%S'
LOG_HEALTH = "#{LOG_DIR}/#{hostname}_#{DATE}_health"
LOG_FOREST = "#{LOG_DIR}/#{hostname}_#{DATE}_forest"
LOG_SYNC = "#{LOG_DIR}/#{hostname}_#{DATE}_sync"

# Create log directory
FileUtils.mkdir_p LOG_DIR

logger = Logger.new(LOG_SYNC)

# Run the actual health check
logger.info 'Running the health check...'
health_check_passed = system("bash #{SCRIPTS_DIR}/health_check.sh #{hostname} > #{LOG_HEALTH} 2>&1")
logger.info 'Health check finished'

# Save the log capture from the Forest container
container_logs = DockerUtils.get_container_logs hostname
File.write(LOG_FOREST, container_logs)

client = SlackClient.new CHANNEL, SLACK_TOKEN

if health_check_passed
  client.post_message "✅ Sync check for #{hostname} passed. 🌲🌳🌲🌳🌲"
else
  client.post_message "⛔ Sync check for #{hostname} fiascoed. 🔥🌲🔥 "
end
client.attach_files(LOG_HEALTH, LOG_SYNC, LOG_FOREST)

logger.info 'Sync check finished'
