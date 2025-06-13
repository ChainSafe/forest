#!/usr/bin/env ruby
# frozen_string_literal: true

# This script was used to reproduce multiple panic bugs in Forest's `Filecoin.MpoolSelect` RPC method.
# It accepts JSON chain list from the standard input, i.e., the output of `forest-cli chain head --format json -n 150`
# and sends requests to the Filecoin node's RPC endpoint with the given tipsets.
#
# https://github.com/ChainSafe/forest/issues/4490.
# https://blog.rust-lang.org/2024/09/05/Rust-1.81.0/#new-sort-implementations

require 'json'
require 'http'

input = $stdin.read
data = JSON.parse(input)

data.map { |item| item['cids'].map { |cid| { '/' => cid } } }.each do |tipset|
  puts tipset.inspect
  HTTP.post(
    'http://localhost:2345/rpc/v0',
    headers: { 'Content-Type' => 'application/json' },
    json: {
      jsonrpc: '2.0',
      method: 'Filecoin.MpoolSelect',
      params: [tipset, 0.8],
      id: 1
    }
  )
end
