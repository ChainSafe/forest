# frozen_string_literal: true

# Picks the ${FOREST_CHAIN} snapshot to test against: the newest one published
# DAYS_AGO days ago (UTC), by the date in its name. Records its URL and head
# epoch (the number its name ends in) under /data for ./init.sh, plus the
# ${EPOCHS}-epoch range below that head for ./setup.sh's probe and ./init.sh's
# back-fill. Runs in the checks image, which has Ruby, so the Forest image
# needs no curl.

require 'date'
require 'json'
require 'net/http'

chain = ENV.fetch('FOREST_CHAIN') { abort 'FOREST_CHAIN is not set' }
epochs = Integer(ENV.fetch('EPOCHS') { abort 'EPOCHS is not set' })
LIST = URI("https://forest-archive.chainsafe.dev/list/#{chain}/latest-v2?format=json")

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
abort "no #{chain} snapshot published for #{day}" if url.nil?
epoch = Integer(url[/_height_(\d+)/, 1])

File.write('/data/snapshot-url', "#{url}\n")
File.write('/data/snapshot-epoch', "#{epoch}\n")
File.write('/data/check-range', "#{epoch - epochs} #{epoch - 1}\n")
puts "#{chain} snapshot for #{day}: #{url}"
