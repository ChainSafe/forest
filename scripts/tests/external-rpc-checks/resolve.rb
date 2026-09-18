# frozen_string_literal: true

# Picks the calibnet snapshot to test against: the newest one published
# DAYS_AGO days ago (UTC), by the date in its name. Records its URL and head
# epoch (the number its name ends in) under /data for ./init.sh and ./setup.sh.
# Runs in the checks image, which has Ruby, so the Forest image needs no curl.

require 'date'
require 'json'
require 'net/http'

LIST = URI('https://forest-archive.chainsafe.dev/list/calibnet/latest-v2?format=json')

def fetch(uri, attempts: 3)
  attempts.times do |i|
    sleep i
    response = Net::HTTP.get_response(uri)
    return response.body if response.is_a?(Net::HTTPSuccess)
  rescue IOError, SystemCallError, Net::OpenTimeout, Net::ReadTimeout
    next
  end
  abort "failed to fetch #{uri} after #{attempts} attempts"
end

days_ago = Integer(ENV.fetch('DAYS_AGO') { abort 'DAYS_AGO is not set' })
day = (Time.now.utc.to_date - days_ago).iso8601
urls = JSON.parse(fetch(LIST))['items'].map { |item| item['url'] }
url = urls.find { |candidate| candidate.include?("_#{day}_") }
abort "no calibnet snapshot published for #{day}" if url.nil?

File.write('/data/snapshot-url', "#{url}\n")
File.write('/data/snapshot-epoch', "#{url[/_height_(\d+)/, 1]}\n")
puts "snapshot for #{day}: #{url}"
