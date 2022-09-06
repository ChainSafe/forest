# frozen_string_literal: true

require_relative 'slack_client'
require_relative 'docker_utils'
require 'rubygems'
require 'logger'
require 'fileutils'
require 'active_support/time'

# Retrieves an environmental variable, failing if its not set or empty.
def get_and_assert_env_variable(name)
  var = ENV[name]
  raise "Please set #{name} environmental variable" if var.nil? || var.empty?

  var
end

BASE_FOLDER = get_and_assert_env_variable 'BASE_FOLDER'
SLACK_TOKEN = get_and_assert_env_variable 'SLACK_API_TOKEN'
CHANNEL = get_and_assert_env_variable 'SLACK_NOTIF_CHANNEL'
SCRIPTS_DIR = '/scripts'
LOG_DIR = './'

CHAIN_NAME = ARGV[0]
raise 'No chain name supplied. Please provide chain identifier, e.g. calibnet or mainnet' if ARGV.empty?

# Current datetime, to append to the log files
DATE = Time.new.strftime '%FT%H:%M:%S'
LOG_HEALTH = "#{LOG_DIR}/#{CHAIN_NAME}_#{DATE}_health"
LOG_FOREST = "#{LOG_DIR}/#{CHAIN_NAME}_#{DATE}_forest"
LOG_SYNC = "#{LOG_DIR}/#{CHAIN_NAME}_#{DATE}_sync"

# Create log directory
FileUtils.mkdir_p LOG_DIR

loop do
  client = SlackClient.new CHANNEL, SLACK_TOKEN

  LATEST = Dir.glob("#{BASE_FOLDER}/s3/#{CHAIN_NAME}/*").max_by {|f| File.mtime(f)}


  if Time.new.strftime('%F') == File.stat(LATEST).mtime.strftime('%F')
    # We already have a snapshot for today. Do nothing.
    #client.post_message "(temporary msg) No snapshot required for #{CHAIN_NAME}"
    puts "(temporary msg) No snapshot required for #{CHAIN_NAME}"
  else
    puts "New snapshot required"
    logger = Logger.new(LOG_SYNC)

    # Run the actual health check
    logger.info 'Syncing and exporting snapshot...'
    # snapshot_uploaded = system("bash #{SCRIPTS_DIR}/upload_snapshot.sh #{CHAIN_NAME} #{LATEST} > #{LOG_HEALTH} 2>&1")
    snapshot_uploaded = system("bash #{SCRIPTS_DIR}/upload_snapshot.sh #{CHAIN_NAME} #{LATEST}")
    logger.info 'Export and upload completed'

    # Save the log capture from the Forest container
    container_logs = DockerUtils.get_container_logs "snapshot_forest"
    File.write(LOG_FOREST, container_logs)

    client = SlackClient.new CHANNEL, SLACK_TOKEN

    if snapshot_uploaded
      # client.post_message "✅ Snapshot uploaded for #{CHAIN_NAME}. 🌲🌳🌲🌳🌲"
    else
      # client.post_message "⛔ Snapshot failed for #{CHAIN_NAME}. 🔥🌲🔥 "
    end
    # client.attach_files(LOG_HEALTH, LOG_FOREST, LOG_SYNC)

    logger.info 'Sync check finished'
  end

  sleep(4.hours)
end
