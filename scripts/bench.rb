#!/usr/bin/env ruby

# frozen_string_literal: true

# Script to test various configurations that can impact performance of the node

require 'fileutils'
require 'open3'
require 'optparse'
require 'pathname'
require 'tmpdir'
require 'toml-rb'

# Defines some hardcoded constants

DEFAULT_SNAPSHOT = '2322240_2022_11_09T06_00_00Z.car'

# This is just for capturing the snapshot height
SNAPSHOT_REGEX = /(?<height>\d+)_.*/.freeze

HEIGHTS_TO_VALIDATE = 400

MINUTE = 60
HOUR = MINUTE * MINUTE

BENCHMARK_SUITE = [
  {
    name: 'baseline',
    config: {
      'rocks_db' => {
        'enable_statistics' => true
      }
    },
    build_command: [
      'cargo',
      'build',
      '--release'
    ],
    import_command: [
      './target/release/forest',
      '--config', '%<c>s',
      '--target-peer-count', '50',
      '--encrypt-keystore', 'false',
      '--import-snapshot', '%<s>s',
      '--halt-after-import'
    ],
    validate_command: [
      './target/release/forest',
      '--config', '%<c>s',
      '--target-peer-count', '50',
      '--encrypt-keystore', 'false',
      '--import-snapshot', '%<s>s',
      '--halt-after-import',
      '--skip-load',
      '--height', '%<h>s'
    ]
  }
].freeze

BENCH_PATHS = { tmpdir: Dir.mktmpdir('forest-benchs-') }.freeze

def tmp_dir
  BENCH_PATHS[:tmpdir]
end

def syscall(*command)
  stdout, _stderr, status = Open3.capture3(*command)
  return stdout.chomp if status.success?

  raise "#{command}, #{status}"
end

def forest_version
  syscall('./target/release/forest', '--version')
end

def default_config
  toml_str = syscall('./target/release/forest-cli', 'config', 'dump')

  default = TomlRB.parse(toml_str)
  default['client']['data_dir'] = tmp_dir
  default
end

def snapshot_dir
  syscall('./target/release/forest-cli', 'snapshot', 'dir')
end

def db_dir
  data_dir = default_config.dig('client', 'data_dir')

  "#{data_dir}/mainnet/db"
end

def db_size
  size = syscall('du', '-h', db_dir)
  size.chomp.split[0]
end

def hr(seconds)
  seconds = seconds < MINUTE ? seconds.ceil(1) : seconds.ceil(0)
  time = Time.at(seconds)
  durfmt = "#{seconds > HOUR ? '%-Hh' : ''}#{seconds < MINUTE ? '' : '%-Mm'}%-S#{seconds < MINUTE ? '.%1L' : ''}s"
  time.strftime(durfmt)
end

def sample_proc(pid, metrics)
  output = syscall('ps', '-o', 'rss,vsz', pid.to_s)
  metrics[:rss].push(output[0].to_i)
  metrics[:vsz].push(output[1].to_i)
end

def proc_monitor(pid)
  metrics = { rss: [], vsz: [] }
  handle = Thread.new do
    loop do
      sample_proc(pid, metrics)
      sleep 0.5
    rescue StandardError => _e
      break
    end
  end
  [handle, metrics]
end

def exec_command_aux(command, metrics)
  Open3.popen2(*command) do |i, o, t|
    i.close

    handle, proc_metrics = proc_monitor(t.pid)
    o.each_line do |l|
      print l
    end

    handle.join
    metrics.merge!(proc_metrics)
  end
end

def exec_command(command, dry_run)
  puts "$ #{command.join(' ')}"
  return {} if dry_run

  metrics = {}
  t0 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
  exec_command_aux(command, metrics)
  t1 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
  metrics[:elapsed] = hr(t1 - t0)
  metrics
end

def config_path(bench)
  "#{tmp_dir}/#{bench[:name]}.toml"
end

def build_config_file(bench)
  default = default_config
  bench_config = bench[:config]
  # TODO: Find a better way to merge (conserve the default keys)
  default.merge!(bench_config)

  toml_str = TomlRB.dump(default)
  File.open(config_path(bench).to_s, 'w') { |file| file.write(toml_str) }
end

def build_substitution_hash(bench, options)
  snapshot = options.fetch(:snapshot, DEFAULT_SNAPSHOT)
  height = snapshot.match(SNAPSHOT_REGEX).named_captures['height'].to_i
  start = height - options.fetch(:height, HEIGHTS_TO_VALIDATE)

  # Escape spaces if any
  config_path = config_path(bench)
  snapshot_path = "#{snapshot_dir}/#{snapshot}"

  { c: config_path, s: snapshot_path, h: start }
end

def write_import_table(metrics)
  result = "Bench | Import Time | Import RSS | DB Size\n"
  result += "-|-|-|-\n"

  metrics.each do |key, value|
    elapsed = value[:import][:elapsed] || 'n/a'
    rss = value[:import][:rss]
    rss_max = rss ? "#{rss.max}B" : 'n/a'
    db_size = value[:import][:db_size] || 'n/a'
    result += "#{key} | #{elapsed} | #{rss_max} | #{db_size}\n"
  end

  result
end

def write_validate_table(metrics)
  result = "Bench | Validate Time | Validate RSS\n"
  result += "-|-|-\n"

  metrics.each do |key, value|
    elapsed = value[:validate][:elapsed] || 'n/a'
    rss = value[:validate][:rss]
    rss_max = rss ? "#{rss.max}B" : 'n/a'
    result += "#{key} | #{elapsed} | #{rss_max}\n"
  end

  result
end

def write_result(metrics)
  # Output file is a suite of markdown tables
  result = ''
  result += write_import_table(metrics)
  result += "---\n"
  result += write_validate_table(metrics)

  File.open('result.md', 'w') { |file| file.write(result) }
end

def splice_args(command, args)
  command.map { |s| s % args }
end

def run_benchmarks(benchs, options)
  dry_run = options[:dry_run]
  benchs_metrics = {}
  benchs.each do |bench|
    puts "Running bench: #{bench[:name]}"

    metrics = {}
    metrics[:name] = bench[:name]

    exec_command(bench[:build_command], dry_run)

    # Clean db
    dir = db_dir
    puts "Wiping #{dir}"
    FileUtils.rm_rf(dir, secure: true) unless dry_run

    # Build bench artefacts
    build_config_file(bench)
    args = build_substitution_hash(bench, options)

    import_command = splice_args(bench[:import_command], args)
    metrics[:import] = exec_command(import_command, dry_run)

    # Save db size just after import
    metrics[:import][:db_size] = db_size unless dry_run

    validate_command = splice_args(bench[:validate_command], args)
    metrics[:validate] = exec_command(validate_command, dry_run)

    benchs_metrics[bench[:name]] = metrics

    puts "\n"
  end
  write_result(benchs_metrics)
end

# TODO: read script arguments and do some filtering otherwise run them all

options = {}
OptionParser.new do |opts|
  opts.banner = 'Usage: bench.rb [options]'
  opts.on('--dry-run', 'Only print the commands that will be run') { |v| options[:dry_run] = v }
  opts.on('--snapshot [Object]', Object, 'Snapshot file to use for benchmarks') { |v| options[:snapshot] = v }
  opts.on('--height [Integer]', Integer, 'Number of heights to validate') { |v| options[:height] = v }
end.parse!

run_benchmarks(BENCHMARK_SUITE, options)

puts 'Done.'
