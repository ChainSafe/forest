# frozen_string_literal: true

# Picks the ${FOREST_CHAIN} snapshot published DAYS_AGO days ago (UTC), by the date in its name,
# and records under /data what the rest of the harness needs: its URL, its head epoch, and the
# chain plus the ${FOREST_CHECK_EPOCHS}-epoch range the checks cover.
# Runs in the checks image, which has Ruby, so the Forest image needs no curl.

require 'date'
require 'json'
require 'net/http'

# Compose passes an unset host variable through as an empty string, which `ENV.fetch` accepts.
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
epochs = env_int('FOREST_CHECK_EPOCHS')
days_ago = env_int('DAYS_AGO')
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

day = (Time.now.utc.to_date - days_ago).iso8601
urls = JSON.parse(fetch(LIST))['items'].map { |item| item['url'] }
url = urls.find { |candidate| candidate.include?("_#{day}_") }
abort "no #{chain} snapshot published for #{day}" if url.nil?
epoch = Integer(url[/_height_(\d+)/, 1])

File.write('/data/snapshot-url', "#{url}\n")
File.write('/data/snapshot-epoch', "#{epoch}\n")
File.write('/data/check-target', "#{chain} #{epoch - epochs} #{epoch - 1}\n")
puts "#{chain} snapshot for #{day}: #{url}"
