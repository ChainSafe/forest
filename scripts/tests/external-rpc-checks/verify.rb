# frozen_string_literal: true

# Confirms the back-fill indexed the epochs the checks query. Runs in a container
# so the node's RPC port never has to be published.

require 'json'
require 'net/http'

url = URI("http://#{ENV.fetch('FOREST_RPC_URL')}")

ARGV.each do |argument|
  epoch = Integer(argument)
  request = Net::HTTP::Post.new(url, 'Content-Type' => 'application/json')
  request.body = {
    jsonrpc: '2.0',
    id: 1,
    method: 'eth_getBlockByNumber',
    params: ["0x#{epoch.to_s(16)}", false]
  }.to_json

  response = Net::HTTP.start(url.hostname, url.port) { |http| http.request(request) }
  body = JSON.parse(response.body)
  abort "epoch #{epoch} is not indexed: #{response.code} #{response.body}" unless body.dig('result', 'number')

  puts "epoch #{epoch} is indexed"
end
