# frozen_string_literal: true
require_relative 'slack_client'
require_relative 'docker_utils'
require 'date'
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

CHAIN_NAME = ARGV[0]
raise 'No chain name supplied. Please provide chain identifier, e.g. calibnet or mainnet' if ARGV.empty?

# Current datetime, to append to the log files
DATE = Time.new.strftime '%FT%H:%M:%S'
LOG_EXPORT = "#{CHAIN_NAME}_#{DATE}_export"

loop do
  client = SlackClient.new CHANNEL, SLACK_TOKEN

  # Find the snapshot with the most recent modification date
  LATEST = Dir.glob("#{BASE_FOLDER}/s3/#{CHAIN_NAME}/*").max_by {|f| File.mtime(f)}

  # Check if the date of the most recent snapshot is today
  if Time.new.to_date() == File.stat(LATEST).mtime.to_date()
    # We already have a snapshot for today. Do nothing.
    puts "No snapshot required for #{CHAIN_NAME}"
  else
    puts "New snapshot required"

    # Sync and export snapshot
    snapshot_uploaded = system("bash upload_snapshot.sh #{CHAIN_NAME} #{LATEST} > #{LOG_EXPORT} 2>&1")

    client = SlackClient.new CHANNEL, SLACK_TOKEN

    if snapshot_uploaded
      client.post_message "✅ Snapshot uploaded for #{CHAIN_NAME}. 🌲🌳🌲🌳🌲"
    else
      client.post_message "⛔ Snapshot failed for #{CHAIN_NAME}. 🔥🌲🔥 "
      client.attach_files(LOG_EXPORT)
    end

    logger.info 'Sync check finished'
  end

  # Loop such that a new snapshot will be updated once per day.
  sleep(1.hour)
end
