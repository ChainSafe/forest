# frozen_string_literal: true

# Blocks until the node's head is past the given epoch: the snapshot heads mid-day, and the
# checks need every epoch of the day executed, with the last one's receipts stored by its child.
# Waits for that epoch rather than for the network head (`forest-cli sync wait`, /readyz)
# because the scheduled run starts hours after the day ended, and catching up to the network
# head first would roughly triple the sync before the checks.
# Gives up when the head stops advancing for STALL_SECONDS, which must outlast the cold start
# (peer discovery and header fetching took ~5 minutes locally); the workflow timeout is the
# hard cap.

require 'json'
require 'net/http'

POLL_SECONDS = 15
STALL_SECONDS = 20 * 60

$stdout.sync = true

abort "usage: #{File.basename($PROGRAM_NAME)} <epoch>" unless ARGV.size == 1
url = URI("http://#{ENV.fetch('FOREST_RPC_URL')}")
target = Integer(ARGV.first) + 1

# The head epoch, or nil if the node did not answer.
def head(url)
  request = Net::HTTP::Post.new(url, 'Content-Type' => 'application/json')
  request.body = { jsonrpc: '2.0', id: 1, method: 'Filecoin.ChainHead', params: [] }.to_json
  response = Net::HTTP.start(url.hostname, url.port) { |http| http.request(request) }
  JSON.parse(response.body).dig('result', 'Height') or raise "unexpected response: #{response.body}"
rescue StandardError => e
  puts "head unavailable: #{e.message}"
  nil
end

seen = nil
advanced_at = Time.now
until (epoch = head(url)) && epoch >= target
  if epoch
    puts "head at #{epoch}, #{target - epoch} epochs to go"
    advanced_at = Time.now if epoch != seen
    seen = epoch
  end
  abort "the head has not advanced for #{STALL_SECONDS}s" if Time.now - advanced_at > STALL_SECONDS
  sleep POLL_SECONDS
end
puts "head at #{epoch} is past #{target - 1}"
