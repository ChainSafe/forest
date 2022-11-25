#!/usr/bin/env ruby

# frozen_string_literal: true

# Script to test various configurations that can impact performance of the node

require 'fileutils'
require 'open3'
require 'optparse'
require 'pathname'
require 'tmpdir'
require 'toml-rb'

# This is just for capturing the snapshot height
SNAPSHOT_REGEX = /(?<height>\d+)_.*/.freeze

HEIGHTS_TO_VALIDATE = 400

MINUTE = 60
HOUR = MINUTE * MINUTE

TEMP_DIR = Dir.mktmpdir('forest-benchs-')

BENCHMARK_SUITE = [
  {
    name: 'baseline',
    config: {
      'rocks_db' => {
        'enable_statistics' => false
      }
    },
    build_command: [
      'cargo', 'build', '--release'
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
  },
  {
    name: 'paritydb',
    config: {},
    build_command: [
      'cargo', 'build', '--release',
      '--no-default-features', '--features', 'forest_fil_cns,paritydb'
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

# Provides human readable formatting to Numeric class
class Numeric
  def to_bibyte
    syscall('numfmt', '--to=iec-i', '--suffix=B', '--format=%.2f', to_s)
  end
end

# Performs a simple deep merge for nested Hashes
class ::Hash
  def deep_merge(second)
    merger = proc { |_key, v1, v2| v1.is_a?(Hash) && v2.is_a?(Hash) ? v1.merge(v2, &merger) : v2 }
    merge(second, &merger)
  end
end

def syscall(*command)
  stdout, _stderr, status = Open3.capture3(*command)

  status.success? ? stdout.chomp : (raise "#{command}, #{status}")
end

def forest_version
  syscall('./target/release/forest', '--version')
end

def default_config
  toml_str = syscall('./target/release/forest-cli', 'config', 'dump')

  default = TomlRB.parse(toml_str)
  default['client']['data_dir'] = TEMP_DIR
  default
end

def snapshot_dir
  syscall('./target/release/forest-cli', 'snapshot', 'dir')
end

def db_dir
  config = default_config
  data_dir = config.dig('client', 'data_dir')
  db = config.key?('rocks_db') ? 'rocksdb' : 'paritydb'

  "#{data_dir}/mainnet/#{db}"
end

def db_size
  size = syscall('du', '-h', db_dir)
  size.split[0]
end

def hr(seconds)
  seconds = seconds < MINUTE ? seconds.ceil(1) : seconds.ceil(0)
  time = Time.at(seconds)
  durfmt = "#{seconds > HOUR ? '%-Hh ' : ''}#{seconds < MINUTE ? '' : '%-Mm '}%-S#{seconds < MINUTE ? '.%1L' : ''}s"
  time.strftime(durfmt)
end

def sample_proc(pid, metrics)
  output = syscall('ps', '-o', 'rss,vsz', pid.to_s)
  output = output.split("\n").last.split
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
  "#{TEMP_DIR}/#{bench[:name]}.toml"
end

def build_config_file(bench)
  config = default_config.deep_merge(bench[:config])

  toml_str = TomlRB.dump(config)
  pp config
  File.open(config_path(bench).to_s, 'w') { |file| file.write(toml_str) }
end

def build_substitution_hash(bench, options)
  snapshot = options[:snapshot]
  height = snapshot.match(SNAPSHOT_REGEX).named_captures['height'].to_i
  start = height - options.fetch(:height, HEIGHTS_TO_VALIDATE)

  return { c: '<tbd>', s: '<tbd>', h: start } if options[:dry_run]

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
    rss_max = rss ? (rss.max * 1024).to_bibyte : 'n/a'
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
    rss_max = rss ? (rss.max * 1024).to_bibyte : 'n/a'
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

  File.open("result_#{Time.now.to_i}.md", 'w') { |file| file.write(result) }
end

def splice_args(command, args)
  command.map { |s| s % args }
end

def prepare_bench(bench, options)
  exec_command(%w[cargo clean], options[:dry_run])
  exec_command(bench[:build_command], options[:dry_run])

  # Build bench artefacts
  build_config_file(bench) unless options[:dry_run]
  build_substitution_hash(bench, options)
end

def run_bench(bench, args, options, metrics)
  import_command = splice_args(bench[:import_command], args)
  metrics[:import] = exec_command(import_command, options[:dry_run])

  # Save db size just after import
  metrics[:import][:db_size] = db_size unless options[:dry_run]

  validate_command = splice_args(bench[:validate_command], args)
  metrics[:validate] = exec_command(validate_command, options[:dry_run])
  clean_up(options)
  metrics
end

def clean_up(options)
  # Clean db
  puts 'Wiping db'
  FileUtils.rm_rf(db_dir, secure: true) unless options[:dry_run]
end

def run_benchmarks(benchs, options)
  benchs_metrics = {}
  benchs.each do |bench|
    puts "Running bench: #{bench[:name]}"
    metrics = {}
    args = prepare_bench(bench, options)
    run_bench(bench, args, options, metrics)
    benchs_metrics[bench[:name]] = metrics

    puts "\n"
  end
  write_result(benchs_metrics)
end

# TODO: read script arguments and do some filtering otherwise run them all

options = {}
OptionParser.new do |opts|
  opts.banner = 'Usage: bench.rb [options] snapshot'
  opts.on('--dry-run', 'Only print the commands that will be run') { |v| options[:dry_run] = v }
  opts.on('--height [Integer]', Integer, 'Number of heights to validate') { |v| options[:height] = v }
end.parse!

snapshot = ARGV.pop
raise OptionParser::ParseError, 'need to specify a snapshot for running benchmarks' unless snapshot

options[:snapshot] = snapshot

run_benchmarks(BENCHMARK_SUITE, options)

puts 'Done.'
