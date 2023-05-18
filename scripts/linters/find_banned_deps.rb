# frozen_string_literal: true

require 'toml-rb'

# Iterate over all TOML files in the repository
Dir.glob('**/*.toml').each do |file|
  # Create an exception for `forest_shim` (only place `fvm` & `fvm3` are allowed).
  next unless file != 'utils/forest_shim/Cargo.toml'

  toml = TomlRB.load_file(file)

  toml['dependencies']&.each do |dep|
    puts "[#{dep[0]}] is being used in [#{file}], but this crate is banned!" if dep[0] == 'fvm' || dep[0] == 'fvm3'
  end
end
