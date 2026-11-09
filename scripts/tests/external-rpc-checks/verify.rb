# frozen_string_literal: true

# Confirms the node indexed the epochs the checks query: the tipset is served and the receipts
# of its messages are in the store, read back through the standard chain API.

require 'json'
require 'net/http'

url = URI("http://#{ENV.fetch('FOREST_RPC_URL')}")

def rpc(url, method, params)
  request = Net::HTTP::Post.new(url, 'Content-Type' => 'application/json')
  request.body = { jsonrpc: '2.0', id: 1, method: method, params: params }.to_json
  JSON.parse(Net::HTTP.start(url.hostname, url.port) { |http| http.request(request) }.body)
end

ARGV.each do |argument|
  epoch = Integer(argument)
  block = rpc(url, 'eth_getBlockByNumber', ["0x#{epoch.to_s(16)}", false])
  # Error code 12 denotes a null round across implementations.
  # https://github.com/ChainSafe/forest/blob/8c980e679b6606c534c6eb5bd8aa03ff8cc6f5c0/src/rpc/methods/eth/errors.rs#L22
  if block.dig('error', 'code') == 12
    puts "epoch #{epoch} was a null round"
    next
  end
  abort "epoch #{epoch} is not indexed: #{block}" unless block.dig('result', 'number')

  # The receipts of an epoch's messages are stored under its child tipset, the first non-null
  # tipset above it. Reading them back is the same check any client would make.
  child = rpc(url, 'Filecoin.ChainGetTipSetAfterHeight', [epoch + 1, []])
  abort "epoch #{epoch} has no tipset above it: #{child['error']}" if child['error']
  receipts = rpc(url, 'Filecoin.ChainGetParentReceipts', [child.dig('result', 'Cids', 0)])
  abort "epoch #{epoch} has no receipts: #{receipts['error']}" if receipts['error']
  puts "epoch #{epoch} is indexed"
end
