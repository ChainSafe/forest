#!/usr/bin/env ruby

# frozen_string_literal: true

# Script to test various configurations that can impact performance of the node

require 'deep_merge'
require 'fileutils'
require 'open3'
require 'optparse'
require 'pathname'
require 'set'
require 'tmpdir'
require 'toml-rb'

# Those are for capturing the snapshot height
SNAPSHOT_REGEXES = [
  /_height_(?<height>\d+)\.car/,
  /(?<height>\d+)_.*/
].freeze

HEIGHTS_TO_VALIDATE = 400

MINUTE = 60
HOUR = MINUTE * MINUTE

TEMP_DIR = Dir.mktmpdir('forest-benchs-')

# Provides human readable formatting to Numeric class
class Numeric
  def to_bibyte
    syscall('numfmt', '--to=iec-i', '--suffix=B', '--format=%.2f', to_s)
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

  # Comment chain.policy section
  patched_toml = toml_str.sub(/(\[chain.policy\].+?(?=\n\[))/m) do |s|
    commented = s.split("\n").map { |l| l.prepend('# ') }.join("\n")
    "#{commented}\n"
  end
  toml_str = patched_toml

  default = TomlRB.parse(toml_str)
  default['client']['data_dir'] = TEMP_DIR
  default
end

def snapshot_dir
  syscall('./target/release/forest-cli', 'snapshot', 'dir')
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

# rubocop:disable Metrics/MethodLength, Metrics/AbcSize
def write_import_table(metrics)
  result = "Bench | Import Time | Import RSS | Import VSZ | DB Size\n"
  result += "-|-|-|-|-\n"

  metrics.each do |key, value|
    elapsed = value[:import][:elapsed] || 'n/a'
    rss = value[:import][:rss]
    rss_max = rss ? (rss.max * 1024).to_bibyte : 'n/a'
    vsz = value[:import][:vsz]
    vsz_max = vsz ? (vsz.max * 1024).to_bibyte : 'n/a'
    db_size = value[:import][:db_size] || 'n/a'
    result += "#{key} | #{elapsed} | #{rss_max} | #{vsz_max} | #{db_size}\n"
  end

  result
end
# rubocop:enable Metrics/MethodLength, Metrics/AbcSize

# rubocop:disable Metrics/MethodLength
def write_validate_table(metrics)
  result = "Bench | Validate Time | Validate RSS | Validate VSZ\n"
  result += "-|-|-|-\n"

  metrics.each do |key, value|
    elapsed = value[:validate][:elapsed] || 'n/a'
    rss = value[:validate][:rss]
    rss_max = rss ? (rss.max * 1024).to_bibyte : 'n/a'
    vsz = value[:validate][:vsz]
    vsz_max = vsz ? (vsz.max * 1024).to_bibyte : 'n/a'
    result += "#{key} | #{elapsed} | #{rss_max} | {#{vsz_max}}\n"
  end

  result
end
# rubocop:enable Metrics/MethodLength

def write_result(metrics)
  # Output file is a suite of markdown tables
  result = ''
  result += write_import_table(metrics)
  result += "---\n"
  result += write_validate_table(metrics)

  filename = "result_#{Time.now.to_i}.md"
  File.open(filename, 'w') { |file| file.write(result) }
  puts "Wrote #{filename}"
end

def splice_args(command, args)
  command.map { |s| s % args }
end

def snapshot_height(snapshot)
  SNAPSHOT_REGEXES.each do |regex|
    match = snapshot.match(regex)
    return match.named_captures['height'].to_i if match
  end
  raise 'unsupported snapshot name'
end

# Benchmarks Forest import of a snapshot and validation of the chain
class Benchmark
  attr_reader :name, :metrics
  attr_accessor :snapshot_path, :heights

  def db_size
    config_path = "#{TEMP_DIR}/#{@name}.toml"

    line = syscall('./target/release/forest-cli', '-c', config_path, 'db', 'stats').split("\n")[1]
    match = line.match(/Database size: (.+)/)
    match[1]
  end

  def clean_db(dry_run)
    puts 'Wiping db'

    config_path = "#{TEMP_DIR}/#{@name}.toml"

    syscall('./target/release/forest-cli', '-c', config_path, 'db', 'clean', '--force') unless dry_run
  end

  def build_config_file
    config = @config.deep_merge(default_config)
    config_path = "#{TEMP_DIR}/#{@name}.toml"

    toml_str = TomlRB.dump(config)
    File.open(config_path, 'w') { |file| file.write(toml_str) }
  end
  private :build_config_file

  def build_substitution_hash(dry_run)
    snapshot = @snapshot_path
    height = snapshot_height(snapshot)
    start = height - @heights

    return { c: '<tbd>', s: '<tbd>', h: start } if dry_run

    config_path = "#{TEMP_DIR}/#{@name}.toml"

    snapshot_path = File.file?(snapshot) ? snapshot : "#{snapshot_dir}/#{snapshot}"

    { c: config_path, s: snapshot_path, h: start }
  end
  private :build_substitution_hash

  def build_artefacts(dry_run)
    # exec_command(%w[cargo clean], dry_run)
    exec_command(build_command, dry_run)

    build_config_file unless dry_run
    build_substitution_hash(dry_run)
  end
  private :build_artefacts

  def run(dry_run)
    puts "Running bench: #{@name}"

    metrics = {}
    args = build_artefacts(dry_run)

    import_command = splice_args(@import_command, args)
    metrics[:import] = exec_command(import_command, dry_run)

    # Save db size just after import
    metrics[:import][:db_size] = db_size unless dry_run

    validate_command = splice_args(@validate_command, args)
    metrics[:validate] = exec_command(validate_command, dry_run)

    clean_db(dry_run)

    @metrics = metrics
  end

  def target
    './target/release/forest'
  end

  def build_command
    ['cargo', 'build', '--release']
  end

  def initialize(name:, config: {})
    @name = name
    @config = config
    @import_command = [
      target, '--config', '%<c>s', '--encrypt-keystore', 'false', '--import-snapshot', '%<s>s', '--halt-after-import'
    ]
    @validate_command = [
      target, '--config', '%<c>s', '--encrypt-keystore', 'false',
      '--import-snapshot', '%<s>s', '--halt-after-import', '--skip-load', '--height', '%<h>s'
    ]
    @metrics = {}
  end
end

# Benchmark class for ParityDb
class ParityDbBenchmark < Benchmark
  def build_command
    ['cargo', 'build', '--release', '--no-default-features', '--features', 'forest_fil_cns,paritydb']
  end
end

# Benchmark class for ParityDb with Jemalloc
class JemallocBenchmark < Benchmark
  def build_command
    ['cargo', 'build', '--release', '--no-default-features', '--features', 'forest_fil_cns,paritydb,jemalloc']
  end
end

# Benchmark class for ParityDb with MiMalloc
class MiMallocBenchmark < Benchmark
  def build_command
    ['cargo', 'build', '--release', '--no-default-features', '--features', 'forest_fil_cns,paritydb,mimalloc']
  end
end

def run_benchmarks(benchmarks, options)
  bench_metrics = {}
  benchmarks.each do |bench|
    bench.snapshot_path = options[:snapshot_path]
    bench.heights = options[:heights]
    bench.run(options[:dry_run])

    bench_metrics[bench.name] = bench.metrics

    puts "\n"
  end
  write_result(bench_metrics)
end

BENCHMARKS = [
  Benchmark.new(name: 'baseline'),
  Benchmark.new(name: 'baseline-with-stats', config: { 'rocks_db' => { 'enable_statistics' => true } }),
  ParityDbBenchmark.new(name: 'paritydb'),
  JemallocBenchmark.new(name: 'paritydb-jemalloc'),
  MiMallocBenchmark.new(name: 'paritydb-mimalloc')
].freeze

options = {
  heights: HEIGHTS_TO_VALIDATE,
  pattern: 'baseline'
}
OptionParser.new do |opts|
  opts.banner = 'Usage: bench.rb [options] snapshot'
  opts.on('--dry-run', 'Only print the commands that will be run') { |v| options[:dry_run] = v }
  opts.on('--heights [Integer]', Integer, 'Number of heights to validate') { |v| options[:heights] = v }
  opts.on('--pattern [String]', 'Run benchmarks that match the pattern') { |v| options[:pattern] = v }
end.parse!

snapshot_path = ARGV.pop
raise OptionParser::ParseError, 'need to specify a snapshot for running benchmarks' unless snapshot_path

options[:snapshot_path] = snapshot_path

selection = Set[]
BENCHMARKS.each do |bench|
  options[:pattern].split(',').each do |pat|
    selection.add(bench) if File.fnmatch(pat.strip, bench.name)
  end
end
if !selection.empty?
  run_benchmarks(selection, options)
else
  puts 'Nothing to run'
end
