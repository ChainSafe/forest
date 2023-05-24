# frozen_string_literal: true

require 'toml-rb'

whitelist = {
  # Check all `Cargo.toml` files for `time` crate
  ['time'] => []
  # The following code will be uncommented as part of `PR#2880: https://github.com/ChainSafe/forest/pull/2880`
  # # Create exceptions for `forest_shim` and `forest_interpreter` (only places `fvm` & `fvm3` are allowed).
  # %w[fvm fvm3] => ['utils/forest_shim/Cargo.toml', 'vm/interpreter/Cargo.toml']
}

# Iterate over all TOML files in the repository
Dir.glob('**/*.toml').each do |file|
  toml = TomlRB.load_file(file)

  toml['dependencies']&.each do |dep|
    whitelist.each_pair do |crates, exceptions|
      crates.each do |crate|
        if dep[0] == crate && !(exceptions.include? file)
          puts "[#{dep[0]}] is being used in [#{file}], but this crate is banned!"
        end
      end
    end
  end
end
