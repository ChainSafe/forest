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
  # Error code 12 denotes a null round across implementations.
  # https://github.com/ChainSafe/forest/blob/8c980e679b6606c534c6eb5bd8aa03ff8cc6f5c0/src/rpc/methods/eth/errors.rs#L22
  null_round = body.dig('error', 'code') == 12

  unless body.dig('result', 'number') || null_round
    abort "epoch #{epoch} is not indexed: #{response.code} #{response.body}"
  end

  puts "epoch #{epoch} #{null_round ? 'was a null round' : 'is indexed'}"
end
