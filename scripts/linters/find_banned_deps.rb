# frozen_string_literal: true

# This script checks all `Cargo.toml` and `*.rs` files for banned dependencies,
# fails the GitHub Continuous Integration dependencies check if any banned
# dependencies are found, and reports any violations in the Continuous
# Integration output. If additional exceptions need to be made, please add
# these to the appropriate list (i.e., `toml_list` for `Cargo.toml` files and
# `rs_list` for Rust source code files).

require 'toml-rb'

toml_list = {
  # Check all `Cargo.toml` files for `time` crate
  'time' => []
}
rs_list = {
  # Create exceptions for `forest_shim` and `forest_interpreter` (only places
  # `fvm` & `fvm3` are allowed).
  'fvm' => ['src/interpreter', 'src/shim'],
  'fvm3' => ['src/interpreter', 'src/shim'],
  'fvm_shared' => ['src/interpreter', 'src/shim', 'src/json/tests/'],
  'fvm_shared3' => ['src/interpreter', 'src/shim']
}

violations = Hash.new { |h, k| h[k] = [] }

# Iterate over TOML file(s) in the repository
Dir.glob('**/*.toml').each do |file|
  toml = TomlRB.load_file(file)

  toml['dependencies']&.each do |dep|
    toml_list.each_pair do |crate, exceptions|
      violations[dep[0]] << file if dep[0] == crate && !(exceptions.include? file)
    end
  end

  toml['dev-dependencies']&.each do |dep|
    toml_list.each_pair do |crate, exceptions|
      violations[dep[0]] << file if dep[0] == crate && !(exceptions.include? file)
    end
  end
end

# Iterate over all Rust source code files in the repository
Dir.glob('**/*.rs').each do |filename|
  File.foreach(filename) do |file|
    rs_list.each_pair do |crate, exceptions|
      pattern = "#{crate}::"
      match = file.match pattern
      violations[crate] << filename if !(exceptions.any? { |exception| filename.include? exception }) && !match.nil?
    end
  end
end

result = violations.sort.map { |dep, file| [dep, file.sort.uniq] }.each do |dep, file|
  puts "[#{dep}] is being used in [#{file * ', '}], but this crate is banned!"
end

exit result.empty?
