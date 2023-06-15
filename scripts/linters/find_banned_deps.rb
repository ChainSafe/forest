# frozen_string_literal: true

# This script checks all `Cargo.toml` files for banned dependencies, fails
# the GitHub Continuous Integration dependencies check if any banned dependencies
# are found, and reports any violations in the Continuous Integration output. If
# additional exceptions need to be made, please add these to the `whitelist`.

require 'toml-rb'

whitelist = {
  # Check all `Cargo.toml` files for `time` crate
  'time' => [],
  # Create exceptions for `forest_shim` and `forest_interpreter` (only places
  # `fvm` & `fvm3` are allowed).
  'fvm' => ['utils/forest_shim/Cargo.toml', 'vm/interpreter/Cargo.toml'],
  'fvm3' => ['utils/forest_shim/Cargo.toml', 'vm/interpreter/Cargo.toml']
}

violations = Hash.new { |h, k| h[k] = [] }

# Iterate over all TOML files in the repository
Dir.glob('**/*.toml').each do |file|
  toml = TomlRB.load_file(file)

  toml['dependencies']&.each do |dep|
    whitelist.each_pair do |crate, exceptions|
      violations[dep[0]] << file if dep[0] == crate && !(exceptions.include? file)
    end
  end

  toml['dev-dependencies']&.each do |dep|
    whitelist.each_pair do |crate, exceptions|
      violations[dep[0]] << file if dep[0] == crate && !(exceptions.include? file)
    end
  end
end

result = violations.sort.map { |dep, file| [dep, file.sort.uniq] }.each do |dep, file|
  puts "[#{dep}] is being used in [#{file * ', '}], but this crate is banned!"
end

exit result.empty?
