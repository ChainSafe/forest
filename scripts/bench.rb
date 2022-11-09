#!/usr/bin/env ruby

# frozen_string_literal: true

# Script to test various configurations that can impact performance of the node

require "fileutils"
require "open3"
require "optparse"
require "pathname"
require "pp"
require "toml-rb"

# Defines some hardcoded constants

#Snapshot = "minimal_finality_stateroots_2304961_2022-11-03_06-00-30.car"
Snapshot = "2322240_2022_11_09T06_00_00Z.car"

# This is just for capturing the snapshot height
#Snapshot_regex = /minimal_finality_stateroots_(?<height>\d+)_.*/
Snapshot_regex = /(?<height>\d+)_.*/

Heights_to_validate = 2000

Benchmark_suite = [
  {
    "name" => "baseline",
    "build_command" => "cargo build --release",
    "import_command" =>   "./target/release/forest --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import",
    "validate_command" => "./target/release/forest --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import --skip-load --height %{h}",
    "config" => {
      "rocks_db" => {
        "enable_statistics" => true,
      },
    },
  },
  # {
  #   "name" => "baseline-with-jemalloc",
  #   "build_command" => "cargo build --release --features 'rocksdb/jemalloc'",
  #   "import_command" => "cargo run --release --bin forest -- --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import",
  #   "validate_command" => "cargo run --release --bin forest -- --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import --skip-load --height %{h}",
  #   "config" => {},
  # },
  {
    "name" => "aggresive-rocksdb",
    "build_command" => "cargo build --release",
    "import_command" => "cargo run --release --bin forest -- --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import",
    "validate_command" => "cargo run --release --bin forest -- --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import --skip-load --height %{h}",
    "config" => {
      "rocks_db" => {
        "write_buffer_size" => 1024 * 1024 * 1024, # 1Gb memtable, will create as large L0 sst files
        "max_open_files" => -1,
        "compaction_style" => "none",
        "compression_type" => "none",
        "enable_statistics" => true,
        "optimize_for_point_lookup" => 256,
      },
    },
  },
]

def get_forest_version()
  version = exec_command("./target/release/forest --version", quiet: true)
  version.chomp
end

def get_default_config()
  toml_str = exec_command("./target/release/forest-cli config dump", quiet: true)

  TomlRB.parse(toml_str)
end

def get_snapshot_dir()
  snapshot_dir = exec_command("./target/release/forest-cli snapshot dir", quiet: true)
  snapshot_dir.chomp
end

def get_db_dir()
  # TODO: expose chain as a script parameter, by default it should be mainnet
  config = get_default_config()
  data_dir = config.dig("client", "data_dir")

  "#{data_dir}/mainnet/db"
end

def exec_command(command, quiet: false, merge: false)
  # TODO: handle merge?
  opts = merge ? {:err => [:child, :out]} : {}
  Open3.popen2("#{command}", {}) { |i, o|
    i.close
    if quiet
      return o.read
    else
      puts "$ #{command}"
      o.each_line do |l|
        print l
      end
      return
    end
  }
end

def build_config_file(bench)
  default = get_default_config()
  bench_config = bench["config"]
  # TODO: Find a better way to merge (conserve the default keys)
  default.merge!(bench_config)

  # TODO: Write toml file in some temp dir?
  toml_str = TomlRB.dump(default)
  File.open("#{bench["name"]}.toml", "w") { |file| file.write(toml_str) }
end

def build_substitution_hash(bench)
  height = Snapshot.match(Snapshot_regex).named_captures["height"].to_i
  start = height - Heights_to_validate

  # Escape spaces if any
  config_path = "#{bench["name"]}.toml".gsub(/\s/, '\\ ')
  snapshot_path = "#{get_snapshot_dir()}/#{Snapshot}".gsub(/\s/, '\\ ')

  { c: config_path, s: snapshot_path, h: start }
end

def run_benchmarks(benchs = Benchmark_suite)
  benchs.each { |bench|
    puts "Running bench: #{bench["name"]}"

    # Clean db
    db_dir = get_db_dir()
    puts "wiping #{db_dir}"
    FileUtils.rm_rf(db_dir, :secure => true)

    # TODO: cargo clean before
    # TODO: we should disable incremental build as well
    exec_command(bench["build_command"])

    # Build bench artefacts
    #puts "#{get_snapshot_dir()}/#{Snapshot}"
    build_config_file(bench)
    params = build_substitution_hash(bench)

    # TODO: fetch snapshot?

    # Run forest benchmark
    puts "Version: #{get_forest_version()}"

    import_command = bench["import_command"] % params
    puts import_command
    #exec_command(import_command)

    validate_command = bench["validate_command"] % params
    puts validate_command
    #exec_command(validate_command)

    # TODO: retrieve stats

    puts "\n"
  }
end

# TODO: read script arguments and do some filtering otherwise run them all
run_benchmarks()

# pp get_default_config()

puts "Done."
