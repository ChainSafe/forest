# frozen_string_literal: true

require 'toml-rb'
require 'set'

exit_code = 0

def get_pattern(crate_raw)
  crate = crate_raw.gsub(/-/, '_')
  Regexp.new("(\\buse\\s#{crate}\\b)|(\\b#{crate}::)")
end

# Special cases to suppress false positives.
def excluded?(crates, crate)
  # `quickcheck` is required implicitly by `quickcheck_macros`
  crate == 'quickcheck' && crates.include?('quickcheck_macros')
end

Dir.glob('**/*.toml').each do |file|
  crate_dir = File.dirname(file)
  toml = TomlRB.load_file(file)
  crates = Set.new
  toml['dependencies']&.each do |crate_name, _|
    crates.add crate_name
  end
  toml['dev-dependencies']&.each do |crate_name, _|
    crates.add crate_name
  end
  if toml['workspace']
    toml['workspace']['dependencies']&.each do |crate_name, _|
      crates.add crate_name
    end
  end

  # Load all the source code from the crate into an in-memory array
  # to improve performance.
  source_code = Dir.glob("#{crate_dir}/**/*.rs").map { |rs| File.read(rs) }
  crates.each do |crate|
    pattern = get_pattern(crate)
    unless source_code.any? { |line| line.match?(pattern) } || excluded?(crates, crate)
      puts "Protentially unused: #{crate} in #{crate_dir}"
      exit_code = 1
    end
  end
end

exit exit_code
