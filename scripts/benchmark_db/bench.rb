#!/usr/bin/env ruby

# frozen_string_literal: true

# Script to test various configurations that can impact performance of the node

require 'concurrent-ruby'
require 'csv'
require 'deep_merge'
require 'fileutils'
require 'open3'
require 'optparse'
require 'set'
require 'tmpdir'
require 'tomlrb'
require 'toml-rb'

# Those are for capturing the snapshot height
SNAPSHOT_REGEXES = [
  /_height_(?<height>\d+)\.car/,
  %r{/(?<height>\d+)_*}
].freeze

HEIGHTS_TO_VALIDATE = 40

MINUTE = 60
HOUR = MINUTE * MINUTE

options = {
  heights: HEIGHTS_TO_VALIDATE,
  pattern: 'baseline',
  chain: 'mainnet'
}
OptionParser.new do |opts|
  opts.banner = 'Usage: bench.rb [options] snapshot'
  opts.on('--dry-run', 'Only print the commands that will be run') { |v| options[:dry_run] = v }
  opts.on('--heights [Integer]', Integer, 'Number of heights to validate') { |v| options[:heights] = v }
  opts.on('--pattern [String]', 'Run benchmarks that match the pattern') { |v| options[:pattern] = v }
  opts.on('--chain [String]', 'Choose network chain [default: mainnet]') { |v| options[:chain] = v }
  opts.on('--tempdir [String]', 'Specify a custom directory for running benchmarks') { |v| options[:tempdir] = v }
  opts.on('--daily', 'Run snapshot import and validation time metrics') { |v| options[:daily] = v }
end.parse!

WORKING_DIR = if options[:tempdir].nil?
                Dir.mktmpdir('benchmark-')
              else
                FileUtils.mkdir_p options[:tempdir]
                options[:tempdir]
              end

# Provides human readable formatting to Numeric class
class Numeric
  def to_bibyte
    syscall('numfmt', '--to=iec-i', '--suffix=B', '--format=%.2f', to_s)
  end
end

def syscall(*command, chdir: '.')
  stdout, _stderr, status = Open3.capture3(*command, chdir: chdir)

  status.success? ? stdout.chomp : (raise "#{command}, #{status}")
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

def get_last_epoch(benchmark)
  epoch = nil
  while epoch.nil?
    # epoch can be nil (e.g. if the client is in the "fetchting messages" stage)
    epoch = benchmark.epoch_command
    sleep 0.5
  end
  epoch
end

def measure_online_validation(benchmark, pid, metrics)
  Thread.new do
    first_epoch = nil
    loop do
      start, first_epoch = benchmark.start_online_validation_command
      if start
        puts 'Start measure'
        break
      end
      sleep 0.1
    end
    sleep benchmark.online_validation_secs
    last_epoch = get_last_epoch(benchmark)
    metrics[:num_epochs] = last_epoch - first_epoch

    puts 'Stopping process...'
    benchmark.stop_command(pid)
  end
end

def proc_monitor(pid, benchmark)
  metrics = Concurrent::Hash.new
  metrics[:rss] = []
  metrics[:vsz] = []
  measure_online_validation(benchmark, pid, metrics) if benchmark
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

def exec_command_aux(command, metrics, benchmark)
  Open3.popen2(*command) do |i, o, t|
    i.close

    handle, proc_metrics = proc_monitor(t.pid, benchmark)
    o.each_line do |l|
      print l
    end

    handle.join
    metrics.merge!(proc_metrics)
  end
end

def format_import_table_row(key, value)
  elapsed = value[:import][:elapsed] || 'n/a'
  rss = value[:import][:rss]
  rss_max = rss ? (rss.max * 1024).to_bibyte : 'n/a'
  vsz = value[:import][:vsz]
  vsz_max = vsz ? (vsz.max * 1024).to_bibyte : 'n/a'
  db_size = value[:import][:db_size] || 'n/a'
  "#{key} | #{elapsed} | #{rss_max} | #{vsz_max} | #{db_size}\n"
end

def format_import_table(metrics)
  result = "Bench | Import Time | Import RSS | Import VSZ | DB Size\n"
  result += "-|-|-|-|-\n"

  metrics.each do |key, value|
    result += format_import_table_row(key, value)
  end

  result
end

def format_validate_table(metrics)
  result = "Bench | Validate Time | Validate RSS | Validate VSZ\n"
  result += "-|-|-|-\n"

  metrics.each do |key, value|
    elapsed = value[:validate][:elapsed] || 'n/a'
    rss = value[:validate][:rss]
    rss_max = rss ? (rss.max * 1024).to_bibyte : 'n/a'
    vsz = value[:validate][:vsz]
    vsz_max = vsz ? (vsz.max * 1024).to_bibyte : 'n/a'
    result += "#{key} | #{elapsed} | #{rss_max} | #{vsz_max}\n"
  end

  result
end

def write_markdown(metrics)
  # Output file is a suite of markdown tables
  result = ''
  result += format_import_table(metrics)
  result += "---\n"
  result += format_validate_table(metrics)

  filename = "result_#{Time.now.to_i}.md"
  File.write(filename, result)
  puts "(I) Wrote #{filename}"
end

def write_csv(metrics)
  filename = "result_#{Time.now.to_i}.csv"
  CSV.open(filename, 'w') do |csv|
    csv << ['Client', 'Snapshot Import Time [min:sec]', 'Validation Time [tipsets/min]']

    metrics.each do |key, value|
      elapsed = value[:import][:elapsed] || 'n/a'
      tpm = value[:validate_online][:tpm] || 'n/a'

      csv << [key, elapsed, tpm]
    end
  end
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

def get_url(chain, url)
  value = chain == 'mainnet' ? 'mainnet' : 'calibrationnet'
  output = syscall('aria2c', '--dry-run', "https://snapshots.#{value}.filops.net/minimal/latest.zst")
  url || output.match(%r{Redirecting to (https://.+?\d+_.+)})[1]
end

def download_and_move(url, output_dir)
  filename = url.match(/(\d+_.+)/)[1]
  checksum_url = url.sub(/\.car\.zst/, '.sha256sum')
  checksum_filename = checksum_url.match(/(\d+_.+)/)[1]
  decompressed_filename = filename.sub(/\.car\.zst/, '.car')

  Dir.mktmpdir do |dir|
    # Download, decompress and verify checksums
    puts '(I) Downloading checksum...'
    syscall('aria2c', checksum_url, chdir: dir)
    puts '(I) Downloading snapshot...'
    syscall('aria2c', '-x5', url, chdir: dir)
    puts "(I) Decompressing #{filename}..."
    syscall('zstd', '-d', filename, chdir: dir)
    puts '(I) Verifying...'
    syscall('sha256sum', '--check', '--status', checksum_filename, chdir: dir)

    FileUtils.mv("#{dir}/#{decompressed_filename}", output_dir)
  end
  "#{output_dir}/#{decompressed_filename}"
end

def download_snapshot(output_dir: WORKING_DIR, chain: 'calibnet', url: nil)
  puts "output_dir: #{output_dir}"
  puts "chain: #{chain}"
  url = get_url(chain, url)
  puts "snapshot_url: #{url}"

  download_and_move(url, output_dir)
end

# Base benchmark class (not usable on its own)
class BenchmarkBase
  attr_reader :name, :metrics
  attr_accessor :dry_run, :snapshot_path, :heights, :chain

  def initialize(name:, config: {})
    @name = name
    @config = config
  end

  def exec_command(command, benchmark = nil)
    puts "$ #{command.join(' ')}"
    return {} if @dry_run

    metrics = Concurrent::Hash.new
    t0 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
    exec_command_aux(command, metrics, benchmark)
    t1 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
    metrics[:elapsed] = hr(t1 - t0)
    metrics
  end

  def build_config_file
    config = @config.deep_merge(default_config)
    config_path = "#{data_dir}/#{@name}.toml"

    toml_str = TomlRB.dump(config)
    File.write(config_path, toml_str)
  end
  private :build_config_file

  def build_substitution_hash
    height = snapshot_height(@snapshot_path)
    start = height - @heights

    return { c: '<tbd>', s: '<tbd>', h: start } if @dry_run

    config_path = "#{data_dir}/#{@name}.toml"

    { c: config_path, s: @snapshot_path, h: start }
  end
  private :build_substitution_hash

  def build_client
    if Dir.exist?(repository_name)
      puts "(W) Directory #{repository_name} is already present"
    else
      puts '(I) Cloning repository'
      clone_command
      Dir.mkdir(repository_name) if @dry_run
    end

    puts '(I) Clean and build client'
    Dir.chdir(repository_name) do
      checkout_command
      clean_command
      build_command
    end
  end

  def build_artefacts
    puts '(I) Building artefacts...'
    build_client

    build_config_file unless @dry_run
    build_substitution_hash
  end
  private :build_artefacts

  def run_validation_step(daily, args, metrics)
    unless daily
      validate_command = splice_args(@validate_command, args)
      metrics[:validate] = exec_command(validate_command)
      return
    end

    validate_online_command = splice_args(@validate_online_command, args)
    new_metrics = exec_command(validate_online_command, self)
    new_metrics[:tpm] = new_metrics[:num_epochs] ? (MINUTE * new_metrics[:num_epochs]) / online_validation_secs : 'n/a'
    metrics[:validate_online] = new_metrics
  end

  def run(daily)
    puts "(I) Running bench: #{@name}"

    metrics = Concurrent::Hash.new
    args = build_artefacts
    @sync_status_command = splice_args(@sync_status_command, args)

    exec_command(@init_command) if @name == 'forest'

    import_command = splice_args(@import_command, args)
    metrics[:import] = exec_command(import_command)

    # Save db size just after import
    metrics[:import][:db_size] = db_size unless @dry_run

    run_validation_step(daily, args, metrics)

    puts '(I) Clean db'
    clean_db

    @metrics = metrics
  end

  def online_validation_secs
    @chain == 'mainnet' ? 120.0 : 10.0
  end

  def start_online_validation_command
    snapshot_height = snapshot_height(@snapshot_path)
    current = epoch_command
    if current
      puts "@#{current}"
      # Check if we can start the measure
      [current >= snapshot_height + 10, current]
    else
      [false, nil]
    end
  end

  def repository_name
    raise 'repository_name method should be implemented'
  end

  def data_dir
    path = "#{WORKING_DIR}/.#{repository_name}"
    FileUtils.mkdir_p path
    path
  end
end

# Forest benchmark class
class ForestBenchmark < BenchmarkBase
  def default_config
    toml_str = syscall(target_cli, '--chain', @chain, 'config', 'dump')

    default = Tomlrb.parse(toml_str)
    default['client']['data_dir'] = data_dir
    default
  end

  def db_size
    config_path = "#{data_dir}/#{@name}.toml"

    line = syscall(target_cli, '-c', config_path, 'db', 'stats').split("\n")[1]
    match = line.match(/Database size: (.+)/)
    match[1]
  end

  def clean_db
    config_path = "#{data_dir}/#{@name}.toml"

    exec_command([target_cli, '-c', config_path, 'db', 'clean', '--force'])
  end

  def target
    File.join('.', repository_name, 'target', 'release', 'forest')
  end

  def target_cli
    File.join('.', repository_name, 'target', 'release', 'forest-cli')
  end

  def repository_name
    'forest'
  end

  def clone_command
    exec_command(['git', 'clone', 'https://github.com/ChainSafe/forest.git', repository_name])
  end

  def checkout_command
    exec_command(%w[git checkout main])
  end

  def clean_command
    exec_command(%w[cargo clean])
  end

  def build_command
    exec_command(['cargo', 'build', '--release'])
  end

  def epoch_command
    begin
      output = syscall(*@sync_status_command)
    rescue RuntimeError
      return nil
    end
    msg_sync = output.match(/Stage:\smessage sync/m)
    if msg_sync
      match = output.match(/Height:\s(\d+)/m)
      return match.captures[0].to_i if match
    end
    nil
  end

  def stop_command(pid)
    syscall('kill', '-SIGINT', pid.to_s)
  end

  def initialize(name:, config: {})
    super(name: name, config: config)
    @init_command = [target_cli, 'fetch-params', '--keys']
    @import_command = [
      target, '--config', '%<c>s', '--encrypt-keystore', 'false', '--import-snapshot', '%<s>s', '--halt-after-import'
    ]
    @validate_command = [
      target, '--config', '%<c>s', '--encrypt-keystore', 'false',
      '--import-snapshot', '%<s>s', '--halt-after-import', '--skip-load=true', '--height', '%<h>s'
    ]
    @validate_online_command = [
      target, '--config', '%<c>s', '--encrypt-keystore', 'false'
    ]
    @sync_status_command = [
      target_cli, '--config', '%<c>s', 'sync', 'status'
    ]
    @metrics = Concurrent::Hash.new
  end
end

# Benchmark class for Forest with the system allocator
class SysAllocBenchmark < ForestBenchmark
  def build_command
    exec_command(
      ['cargo', 'build', '--release', '--features', 'rustalloc']
    )
  end
end

# Benchmark class for Forest with Mimalloc allocator
class MimallocBenchmark < ForestBenchmark
  def build_command
    exec_command(
      ['cargo', 'build', '--release', '--features', 'mimalloc']
    )
  end
end

# Lotus benchmark class
class LotusBenchmark < BenchmarkBase
  def default_config
    toml_str = syscall(target, 'config', 'default')

    Tomlrb.parse(toml_str)
  end

  def db_dir
    lotus_path = ENV.fetch('LOTUS_PATH', nil)
    "#{lotus_path}/datastore/chain"
  end

  def db_size
    size = syscall('du', '-h', db_dir)
    size.split[0]
  end

  def clean_db
    FileUtils.rm_rf(db_dir, secure: true) unless @dry_run
  end

  def repository_name
    'lotus'
  end

  def target
    File.join('.', repository_name, 'lotus')
  end

  def clone_command
    exec_command(['git', 'clone', 'https://github.com/filecoin-project/lotus.git', repository_name])
  end

  def checkout_command
    if @chain == 'mainnet'
      exec_command(%w[git checkout releases])
    else
      exec_command(%w[git checkout master])
    end
  end

  def clean_command
    exec_command(%w[make clean])
  end

  def build_command
    exec_command(['make', @chain == 'mainnet' ? 'all' : 'calibnet'])
  end

  def epoch_command
    begin
      output = syscall(target, 'sync', 'status')
    rescue RuntimeError
      return nil
    end
    msg_sync = output.match(/Stage: message sync/m)
    if msg_sync
      match = output.match(/Height: (\d+)/m)
      return match.captures[0].to_i if match
    end
    nil
  end

  def stop_command(_pid)
    syscall(target, 'daemon', 'stop')
  end

  def initialize(name:, config: {})
    super(name: name, config: config)
    ENV['LOTUS_PATH'] = File.join(WORKING_DIR, ".#{repository_name}")
    @import_command = [
      target, 'daemon', '--config', '%<c>s', '--import-snapshot', '%<s>s', '--halt-after-import'
    ]
    @validate_online_command = [
      target, 'daemon', '--config', '%<c>s'
    ]
    @sync_status_command = [
      target, 'sync', 'status', '--config', '%<c>s'
    ]
    @metrics = Concurrent::Hash.new
  end
end

def benchmarks_loop(benchmarks, options, bench_metrics)
  benchmarks.each do |bench|
    bench.dry_run = options[:dry_run]
    bench.snapshot_path = options[:snapshot_path]
    bench.heights = options[:heights]
    bench.chain = options[:chain]
    bench.run(options[:daily])

    bench_metrics[bench.name] = bench.metrics

    puts "\n"
  end
end

def run_benchmarks(benchmarks, options)
  bench_metrics = Concurrent::Hash.new
  options[:snapshot_path] = File.expand_path(options[:snapshot_path])
  puts "(I) Using snapshot: #{options[:snapshot_path]}"
  puts "(I) WORKING_DIR: #{WORKING_DIR}"
  puts ''
  Dir.chdir(WORKING_DIR) do
    benchmarks_loop(benchmarks, options, bench_metrics)
  end
  if options[:daily]
    write_csv(bench_metrics)
  else
    write_markdown(bench_metrics)
  end
end

FOREST_BENCHMARKS = [
  ForestBenchmark.new(name: 'baseline'),
  SysAllocBenchmark.new(name: 'sysalloc'),
  MimallocBenchmark.new(name: 'mimalloc')
].freeze

snapshot_path = ARGV.pop
raise "The file '#{snapshot_path}' does not exist" if snapshot_path && !File.file?(snapshot_path)

if snapshot_path.nil?
  puts '(I) No snapshot provided, downloading one'
  snapshot_path = download_snapshot(chain: options[:chain])
  puts snapshot_path
  # if a fresh snapshot is downloaded, allow network to move ahead, otherwise
  # `message sync` phase may not be long enough for validation metric
  puts 'Fresh snapshot; sleeping while network advances for 5 minutes...'
  sleep 300 if options[:daily]
end

options[:snapshot_path] = snapshot_path

if options[:daily]
  selection = Set[
    ForestBenchmark.new(name: 'forest'),
    LotusBenchmark.new(name: 'lotus')
  ]
  run_benchmarks(selection, options)
else
  selection = Set[]
  FOREST_BENCHMARKS.each do |bench|
    options[:pattern].split(',').each do |pat|
      selection.add(bench) if File.fnmatch(pat.strip, bench.name)
    end
  end
  if selection.empty?
    puts '(I) Nothing to run'
  else
    run_benchmarks(selection, options)
  end
end
