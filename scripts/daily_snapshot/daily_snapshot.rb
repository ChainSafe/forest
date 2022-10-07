# frozen_string_literal: true

require_relative 'ruby_common/slack_client'
require_relative 'ruby_common/docker_utils'
require_relative 'snapshots_prune'

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

SNAPSHOTS_DIR = File.join(BASE_FOLDER, 's3', CHAIN_NAME)

loop do
  client = SlackClient.new CHANNEL, SLACK_TOKEN

  # Find the snapshot with the most recent modification date
  latest = Dir.glob(File.join(SNAPSHOTS_DIR, '/*.car')).max_by { |f| File.mtime(f) }

  # Check if the date of the most recent snapshot is today
  if Time.new.to_date == File.stat(latest).mtime.to_date
    # We already have a snapshot for today. Do nothing.
    puts "No snapshot required for #{CHAIN_NAME}"
  else
    puts 'New snapshot required'

    # Sync and export snapshot
    snapshot_uploaded = system("bash upload_snapshot.sh #{CHAIN_NAME} #{latest} > #{LOG_EXPORT} 2>&1")

    if snapshot_uploaded
      client.post_message "✅ Snapshot uploaded for #{CHAIN_NAME}. 🌲🌳🌲🌳🌲"
    else
      client.post_message "⛔ Snapshot failed for #{CHAIN_NAME}. 🔥🌲🔥 "
    end

    # attach the log file and print the contents to STDOUT
    client.attach_files(LOG_EXPORT)
    puts "Snapshot export log:\n#{File.read(LOG_EXPORT)}"

    # Prune snapshots
    pruned = prune_snapshots(SNAPSHOTS_DIR)
    client.attach_comment("Pruned snapshots: `#{pruned.join(', ')}`") unless pruned.empty?
  end

  # Loop such that a new snapshot will be updated once per day.
  sleep(1.hour)
end
