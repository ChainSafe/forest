#!/usr/bin/env ruby

# Validate if we received valid docker image string.

regex = /\A(?:[a-zA-Z0-9.-]+(?::[0-9]+)?\/)?(?:[a-z0-9]+(?:[._-][a-z0-9]+)*(?:\/[a-z0-9]+(?:[._-][a-z0-9]+)*)*)(?::[a-zA-Z0-9_][a-zA-Z0-9._-]{0,127})?(?:@sha256:[a-fA-F0-9]{64})?\z/
cleaned = ENV['FOREST_IMAGE_INPUT'].to_s.strip

if cleaned =~ regex
  File.open(ENV['GITHUB_ENV'], 'a') { |f| f.puts "FOREST_IMAGE=#{cleaned}" }
else
  warn "❌ Invalid image: #{ENV['FOREST_IMAGE_INPUT'].inspect}"
  exit 1
end