# frozen_string_literal: true

# Picks the ${FOREST_CHAIN} snapshot published DAYS_AGO days ago (UTC), by the date in its name,
# and records under /data what the rest of the harness needs: its URL, its head epoch, and the
# chain plus the ${FOREST_CHECK_EPOCHS}-epoch range the checks cover.
# Runs in the checks image, which has Ruby, so the Forest image needs no curl.

require 'date'
require 'json'
require 'net/http'

SECONDS_IN_EPOCH = 30
EPOCHS_IN_DAY = 2880
SNAPSHOT_DEPTH = 2000
GENESIS = { 'calibnet' => 1_667_326_380, 'mainnet' => 1_598_306_400 }.freeze

def env(name)
  value = ENV[name].to_s
  abort "#{name} is not set" if value.empty?

  value
end

def env_int(name)
  Integer(env(name))
rescue ArgumentError => e
  abort e.message
end

chain = env('FOREST_CHAIN')
days_ago = env_int('DAYS_AGO')
genesis = GENESIS.fetch(chain) { abort "no genesis timestamp for #{chain}" }
LIST = URI("https://forest-archive.chainsafe.dev/list/#{chain}/latest-v2?format=json")

def fetch(uri, attempts: 3)
  reason = nil
  attempts.times do |i|
    sleep i
    response = Net::HTTP.get_response(uri)
    return response.body if response.is_a?(Net::HTTPSuccess)

    reason = "#{response.code} #{response.message}"
  rescue StandardError => e
    reason = e.message
  end
  abort "failed to fetch #{uri} after #{attempts} attempts: #{reason}"
end

day = Time.now.utc.to_date - days_ago
start = (Time.utc(day.year, day.month, day.day).to_i - genesis) / SECONDS_IN_EPOCH
finish = start + EPOCHS_IN_DAY - 1

snapshots = JSON.parse(fetch(LIST))['items'].filter_map do |item|
  height = item['url'][/_height_(\d+)/, 1]
  [item['url'], Integer(height)] if height
end
reachable = start...start + SNAPSHOT_DEPTH
url, epoch = snapshots.select { |_, height| reachable.cover?(height) }.max_by(&:last)
if url.nil?
  warn "no #{chain} snapshot heads within the first #{SNAPSHOT_DEPTH} epochs of #{day.iso8601}"
  exit 2
end

File.write('/data/snapshot-url', "#{url}\n")
File.write('/data/snapshot-epoch', "#{epoch}\n")
File.write('/data/check-target', "#{chain} #{start} #{finish}\n")
puts "#{chain} #{day.iso8601} is epochs #{start}..#{finish}; snapshot at #{epoch}: #{url}"
