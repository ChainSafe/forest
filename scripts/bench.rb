#!/usr/bin/env ruby

# frozen_string_literal: true

# Script to test various configurations that can impact performance of the node

require "fileutils"
require "open3"
require "optparse"
require "pathname"
require "pp"
require "tmpdir"
require "toml-rb"

# Defines some hardcoded constants

Snapshot = "2322240_2022_11_09T06_00_00Z.car"

# This is just for capturing the snapshot height
Snapshot_regex = /(?<height>\d+)_.*/

Heights_to_validate = 2000

Minute = 60
Hour = Minute * Minute

Benchmark_suite = [
  {
    :name => "baseline",
    :build_command => "cargo build --release",
    :import_command => "./target/release/forest --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import",
    :validate_command => "./target/release/forest --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import --skip-load --height %{h}",
    :config => {
      "rocks_db" => {
        "enable_statistics" => true,
      },
    },
  },
  # {
  #   :name => "baseline-with-jemalloc",
  #   :build_command => "cargo build --release --features 'rocksdb/jemalloc'",
  #   :import_command => "./target/release/forest --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import",
  #   :validate_command => "./target/release/forest --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import --skip-load --height %{h}",
  #   :config => {},
  # },
  {
    :name => "aggresive-rocksdb",
    :build_command => "cargo build --release",
    :import_command => "./target/release/forest --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import",
    :validate_command => "./target/release/forest --config %{c} --target-peer-count 50 --encrypt-keystore false --import-snapshot %{s} --halt-after-import --skip-load --height %{h}",
    :config => {
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

$tmp_dir = nil

def tmp_dir()
  if !$tmp_dir
    $tmp_dir = Dir.mktmpdir("forest-benchs-")
  end
  $tmp_dir
end

def get_forest_version()
  version = exec_command("./target/release/forest --version", quiet: true)
  version.chomp
end

def get_default_config()
  toml_str = exec_command("./target/release/forest-cli config dump", quiet: true)

  default = TomlRB.parse(toml_str)
  default["client"]["data_dir"] = tmp_dir()
  default
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

def hr(seconds)
  seconds = seconds < Minute ? seconds.ceil(1) : seconds.ceil(0)
  time = Time.at(seconds)
  durfmt = "#{seconds>Hour ? '%-Hh' : ''}#{seconds<Minute ? '' : '%-Mm'}%-S#{seconds<Minute ? '.%1L' : ''}s"
  time.strftime(durfmt)
end

def exec_command(command, quiet: false, merge: false, dry_run: false)
  t0 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
  if dry_run
    puts "$ #{command}"
  else
    # TODO: handle merge?
    opts = merge ? { :err => [:child, :out] } : {}
    Open3.popen2("#{command}", {}) { |i, o|
      i.close
      if quiet
        return o.read
      else
        puts "$ #{command}"
        o.each_line do |l|
          print l
        end
      end
    }
  end
  t1 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
  elapsed_time = t1 - t0
end

def config_path(bench)
  "#{tmp_dir()}/#{bench[:name]}.toml"
end

def build_config_file(bench)
  default = get_default_config()
  bench_config = bench[:config]
  # TODO: Find a better way to merge (conserve the default keys)
  default.merge!(bench_config)

  toml_str = TomlRB.dump(default)
  File.open("#{config_path(bench)}", "w") { |file| file.write(toml_str) }
end

def build_substitution_hash(bench)
  height = Snapshot.match(Snapshot_regex).named_captures["height"].to_i
  start = height - Heights_to_validate

  # Escape spaces if any
  config_path = config_path(bench).gsub(/\s/, '\\ ')
  snapshot_path = "#{get_snapshot_dir()}/#{Snapshot}".gsub(/\s/, '\\ ')

  { c: config_path, s: snapshot_path, h: start }
end

def run_benchmarks(benchs, options)
  benchs.each { |bench|
    puts "Running bench: #{bench[:name]}"

    dry_run = options[:dry_run]

    # TODO: cargo clean before
    exec_command(bench[:build_command], quiet: false, dry_run: dry_run)

    # Clean db
    db_dir = get_db_dir()
    puts "Wiping #{db_dir}"
    if !dry_run
      FileUtils.rm_rf(db_dir, :secure => true)
    end

    # Build bench artefacts
    #puts "Snapshot dir: #{get_snapshot_dir()}/#{Snapshot}"
    build_config_file(bench)
    params = build_substitution_hash(bench)

    # Run forest benchmark
    puts "Version: #{get_forest_version()}"

    import_command = bench[:import_command] % params
    import_time = exec_command(import_command, quiet: false, dry_run: dry_run)
    puts "Took: #{hr(import_time)}"

    validate_command = bench[:validate_command] % params
    validate_time = exec_command(validate_command, quiet: false, dry_run: dry_run)
    puts "Took: #{hr(validate_time)}"

    # TODO: retrieve stats

    puts "\n"
  }
end

# TODO: read script arguments and do some filtering otherwise run them all

options = {}
OptionParser.new do |opts|
  opts.banner = "Usage: bench.rb [options]"
  opts.on('--dry-run', 'Only prints the commands that will be run') { |v| options[:dry_run] = v }
end.parse!

run_benchmarks(Benchmark_suite, options)

# pp get_default_config()

puts "Done."
