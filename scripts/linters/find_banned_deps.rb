# frozen_string_literal: true

require 'toml-rb'

# Iterate over all TOML files in the repository
Dir.glob('**/*.toml').each do |file|
  toml = TomlRB.load_file(file)

  toml['dependencies']&.each do |dep|
    # Check all `Cargo.toml` files for `time` crate
    puts "[#{dep[0]}] is being used in [#{file}], but this crate is banned!" if dep[0] == 'time'
    # Create an exception for `forest_shim` (only place `fvm` & `fvm3` are allowed).
    next unless file != 'utils/forest_shim/Cargo.toml'

    puts "[#{dep[0]}] is being used in [#{file}], but this crate is banned!" if dep[0] == 'fvm' || dep[0] == 'fvm3'
  end
end
