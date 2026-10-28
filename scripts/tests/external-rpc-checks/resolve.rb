# frozen_string_literal: true

# Picks the newest ${FOREST_CHAIN} snapshot whose head lies within the first 2000 epochs of
# the UTC day: it holds the state the backfill needs to reach the day's start,
# and the node syncs the rest. Writes its URL, head epoch and the day's epoch range under /data.

require 'date'
require 'json'
require 'net/http'

SECONDS_IN_EPOCH = 30
EPOCHS_IN_DAY = 2880
SNAPSHOT_DEPTH = 2000
GENESIS = { 'calibnet' => 1_667_326_380, 'mainnet' => 1_598_306_400 }.freeze

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

chain = ENV.fetch('FOREST_CHAIN')
days_ago = Integer(ENV.fetch('DAYS_AGO'))
genesis = GENESIS.fetch(chain) { abort "no genesis timestamp for #{chain}" }

day = Time.now.utc.to_date - days_ago
start = (Time.utc(day.year, day.month, day.day).to_i - genesis) / SECONDS_IN_EPOCH
finish = start + EPOCHS_IN_DAY - 1

list = URI("https://forest-archive.chainsafe.dev/list/#{chain}/latest-v2?format=json")
snapshots = JSON.parse(fetch(list))['items'].filter_map do |item|
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
