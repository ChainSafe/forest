# frozen_string_literal: true

require 'toml-rb'
require 'set'

exit_code = 0

Dir.glob("#{File.dirname(File.expand_path($PROGRAM_NAME))}/../**/*.toml").each do |file|
  crate_dir = File.dirname(file)
  toml = TomlRB.load_file(file)
  crates = Set.new
  toml['dependencies']&.each do |pair|
    crates.add pair[0]
  end
  toml['dev-dependencies']&.each do |pair|
    crates.add pair[0]
  end
  crates.each do |crate_raw|
    crate = crate_raw.gsub(/-/, '_')
    used = false
    Dir.glob("#{crate_dir}/**/*.rs").each do |rs|
      used = true if !used && File.read(rs).match?(Regexp.new("(\buse\\s#{crate}\\b)|(\\b#{crate}::)"))
    end
    unless used
      puts "Protentially unused: #{crate_raw} in #{crate_dir}"
      exit_code = 1
    end
  end
end

exit exit_code
