# frozen_string_literal: true

require 'toml-rb'

# Versioned dependencies, mapping dependency to an array of files where it is declared
deps = Hash.new { |h, k| h[k] = [] }

# Iterate over all TOML files in the repository
Dir.glob('**/*.toml').each do |file|
  toml = TomlRB.load_file(file)

  # Add regular dependencies
  toml['dependencies']&.each do |dep|
    # add the dependency if it is not inheriting version from workspace
    deps[dep[0]] << file unless dep[1].include? 'workspace'
  end

  # Add dev dependencies
  toml['dev-dependencies']&.each do |dep|
    # add the dependency if it is not inheriting version from workspace
    deps[dep[0]] << file unless dep[1].include? 'workspace'
  end
end

result = deps
         .map { |dep, files| [dep, files.sort.uniq] }
         .select { |_, files| files.size > 1 }
         .each do |dep, files|
  puts "#{files.size} duplicates for [#{dep}] found for [#{files * ','}]"
end

exit result.empty? ? 0 : 1
