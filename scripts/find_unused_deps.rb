# frozen_string_literal: true

require 'toml-rb'
require 'set'

exit_code = 0

def get_pattern(crate_raw)
  crate = crate_raw.gsub(/-/, '_')
  Regexp.new("(\\buse\\s#{crate}\\b)|(\\b#{crate}::)")
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
  crates.each do |crate|
    used = false
    pattern = get_pattern(crate)
    Dir.glob("#{crate_dir}/**/*.rs").each do |rs|
      used |= File.read(rs).match?(pattern)
    end
    unless used
      puts "Protentially unused: #{crate} in #{crate_dir}"
      exit_code = 1
    end
  end
end

exit exit_code
