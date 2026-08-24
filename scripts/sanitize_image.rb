#!/usr/bin/env ruby
# frozen_string_literal: true

# Validate if we received valid docker image string.

regex = %r{
  \A
  (?:[a-zA-Z0-9.-]+(?::[0-9]+)?/)?          # registry host, optionally with a port
  [a-z0-9]+(?:[._-][a-z0-9]+)*              # first path component
  (?:/[a-z0-9]+(?:[._-][a-z0-9]+)*)*        # further path components
  (?::[a-zA-Z0-9_][a-zA-Z0-9._-]{0,127})?   # tag
  (?:@sha256:[a-fA-F0-9]{64})?              # digest
  \z
}x

cleaned = ENV['FOREST_IMAGE_INPUT'].to_s.strip

if cleaned.match?(regex)
  File.open(ENV.fetch('GITHUB_ENV'), 'a') { |f| f.puts "FOREST_IMAGE=#{cleaned}" }
else
  warn "❌ Invalid image: #{ENV['FOREST_IMAGE_INPUT'].inspect}"
  exit 1
end
