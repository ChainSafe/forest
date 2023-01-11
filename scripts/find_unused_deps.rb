# frozen_string_literal: true

require 'toml-rb'
require 'set'

def is_crate_used()
end

Dir.glob("#{File.dirname(File.expand_path $0)}/../**/*.toml").each do |file|
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
            if !used and File.read(rs).match?(Regexp.new("(\buse\\s#{crate}\\b)|(\\b#{crate}::)")) then
                used = true
            end
        end
        if !used then
            puts "Protentially unused: #{crate_raw} in #{crate_dir}"
        end
    end
    # if crates.size > 0 then
    #     break
    # end
end
