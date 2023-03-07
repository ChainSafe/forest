#!/usr/bin/env ruby

# frozen_string_literal: true

# Script to test various configurations that can impact performance of the node

require 'csv'
require 'deep_merge'
require 'fileutils'
require 'open3'
require 'optparse'
require 'pathname'
require 'set'
require 'tmpdir'
require 'tomlrb'
require 'toml-rb'

# Those are for capturing the snapshot height
SNAPSHOT_REGEXES = [
  /_height_(?<height>\d+)\.car/,
  /(?<height>\d+)_.*/
].freeze

HEIGHTS_TO_VALIDATE = 5

MINUTE = 60
HOUR = MINUTE * MINUTE

BENCHMARK_DIR = Dir.mktmpdir('benchmark-')

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

def proc_monitor(pid, benchmark)
  # TODO: synchronize access to metrics hashmap
  metrics = { rss: [], vsz: [] }
  if benchmark
    Thread.new do
      first_epoch = nil
      loop do
        start, first_epoch = benchmark.start_online_validation_command
        if start
          puts 'Start measure'
          break
        end
        sleep 0.05
      end
      sleep benchmark.online_validation_secs
      last_epoch = benchmark.epoch_command
      # TODO: sometimes last_epoch or first_epoch is nil, fix it
      metrics[:num_epochs] = last_epoch - first_epoch

      puts 'Stopping process...'
      benchmark.stop_command(pid)
    end
  end
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

def exec_command(command, dry_run, benchmark = nil)
  puts "$ #{command.join(' ')}"
  return {} if dry_run

  metrics = {}
  t0 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
  exec_command_aux(command, metrics, benchmark)
  t1 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
  metrics[:elapsed] = hr(t1 - t0)
  metrics
end

def format_import_table_row(key, value)
  elapsed = value[:import][:elapsed] || 'n/a'
  rss = value[:import][:rss]
  rss_max = rss ? (rss.max * 1024).to_bibyte : 'n/a'
  db_size = value[:import][:db_size] || 'n/a'
  "#{key} | #{elapsed} | #{rss_max} | #{db_size}\n"
end

def format_import_table(metrics)
  result = "Bench | Import Time | Import RSS | DB Size\n"
  result += "-|-|-|-\n"

  metrics.each do |key, value|
    result += format_import_table_row(key, value)
  end

  result
end

def format_validate_table(metrics)
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
    csv << ['Client', 'Snapshot Import Time [sec]', 'Validation Time [tipsets/min]']

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

def get_url(chain: 'calibnet', url: nil)
  value = chain == 'mainnet' ? 'mainnet' : 'calibrationnet'
  output = syscall('aria2c', '--dry-run', "https://snapshots.#{value}.filops.net/minimal/latest.zst")
  url || output.match(%r{Redirecting to (https://.+?\d+_.+)})[1]
end

def download_and_move(url, filename, checksum_url, checksum_filename, output_dir)
  Dir.mktmpdir do |dir|
    # Download, decompress and verify checksums
    puts 'Downloading...'
    syscall('aria2c', checksum_url, chdir: dir)
    syscall('aria2c', '-x5', url, chdir: dir)
    puts 'Decompressing...'
    syscall('zstd', '-d', filename, chdir: dir)
    puts 'Verifying...'
    syscall('sha256sum', '--check', '--status', checksum_filename, chdir: dir)

    FileUtils.mv("#{dir}/#{decompressed_filename}", output_dir)
  end
end

def download_snapshot(output_dir: '.', chain: 'calibnet', url: nil)
  puts "output_dir: #{output_dir}"
  puts "chain: #{chain}"
  url = get_url(chain, url)
  puts "snapshot_url: #{url}"

  filename = url.match(/(\d+_.+)/)[1]
  checksum_url = url.sub(/\.car\.zst/, '.sha256sum')
  checksum_filename = checksum_url.match(/(\d+_.+)/)[1]
  decompressed_filename = filename.sub(/\.car\.zst/, '.car')

  download_and_move(url, filename, checksum_url, checksum_filename, output_dir)
  "#{output_dir}/#{decompressed_filename}"
end

# Base benchmark class
class Benchmark
  attr_reader :name, :metrics
  attr_accessor :snapshot_path, :heights, :chain

  def initialize(name:, config: {})
    @name = name
    @config = config
  end

  def build_config_file
    config = @config.deep_merge(default_config)
    config_path = "#{data_dir}/#{@name}.toml"

    toml_str = TomlRB.dump(config)
    File.write(config_path, toml_str)
  end
  private :build_config_file

  def build_substitution_hash(dry_run)
    height = snapshot_height(@snapshot_path)
    start = height - @heights

    return { c: '<tbd>', s: '<tbd>', h: start } if dry_run

    config_path = "#{data_dir}/#{@name}.toml"

    { c: config_path, s: @snapshot_path, h: start }
  end
  private :build_substitution_hash

  def build_artefacts(dry_run)
    puts '(I) Building artefacts...'
    if Dir.exist?(repository_name)
      puts "(W) Directory #{repository_name} is already present"
    else
      puts '(I) Cloning repository'
      clone_command(dry_run)
      Dir.mkdir(repository_name) if dry_run
    end
    Dir.chdir(repository_name) do
      checkout_command(dry_run)
    end
    puts '(I) Clean and build client'
    Dir.chdir(repository_name) do
      clean_command(dry_run)
      build_command(dry_run)
    end

    build_config_file unless dry_run
    build_substitution_hash(dry_run)
  end
  private :build_artefacts

  def run(dry_run, daily)
    puts "(I) Running bench: #{@name}"

    metrics = {}
    args = build_artefacts(dry_run)

    import_command = splice_args(@import_command, args)
    metrics[:import] = exec_command(import_command, dry_run)

    # Save db size just after import
    metrics[:import][:db_size] = db_size unless dry_run

    @sync_status_command = splice_args(@sync_status_command, args)

    unless daily
      validate_command = splice_args(@validate_command, args)
      metrics[:validate] = exec_command(validate_command, dry_run)
    end

    if daily
      validate_online_command = splice_args(@validate_online_command, args)
      new_metrics = exec_command(validate_online_command, dry_run, self)
      if new_metrics[:num_epochs] then
        new_metrics[:tpm] = (MINUTE * new_metrics[:num_epochs]) / online_validation_secs
      else
        new_metrics[:tpm] = 'n/a'
      end
      metrics[:validate_online] = new_metrics
    end

    puts '(I) Clean db'
    clean_db(dry_run)

    @metrics = metrics
  end

  def online_validation_secs
    #@chain == 'mainnet' ? 120.0 : 60.0
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
    path = "#{BENCHMARK_DIR}/.#{repository_name}"
    FileUtils.mkdir_p path
    path
  end
end

# Forest benchmark class
class ForestBenchmark < Benchmark
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

  def clean_db(dry_run)
    config_path = "#{data_dir}/#{@name}.toml"

    exec_command([target_cli, '-c', config_path, 'db', 'clean', '--force'], dry_run)
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

  def clone_command(dry_run)
    exec_command(['git', 'clone', 'https://github.com/ChainSafe/forest.git', repository_name], dry_run)
  end

  def checkout_command(_dry_run); end

  def clean_command(dry_run)
    exec_command(%w[cargo clean], dry_run)
  end

  def build_command(dry_run)
    exec_command(['cargo', 'build', '--release'], dry_run)
  end

  def epoch_command
    begin
      output = syscall(*@sync_status_command)
    rescue RuntimeError
      return nil
    end
    msg_sync = output.match(/Stage:\smessage sync/m)
    if msg_sync
      # TODO: merge matches since forest prints workers that could be at different stages
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
    @name = name
    @config = config
    @import_command = [
      target, '--config', '%<c>s', '--encrypt-keystore', 'false', '--import-snapshot', '%<s>s', '--halt-after-import'
    ]
    @validate_command = [
      target, '--config', '%<c>s', '--encrypt-keystore', 'false',
      '--import-snapshot', '%<s>s', '--halt-after-import', '--skip-load', '--height', '%<h>s'
    ]
    @validate_online_command = [
      target, '--config', '%<c>s', '--encrypt-keystore', 'false'
    ]
    @sync_status_command = [
      target_cli, '--config', '%<c>s', 'sync', 'status'
    ]
    @metrics = {}
  end
end

# Benchmark class for Forest with ParityDb backend
class ParityDbBenchmark < ForestBenchmark
  def build_command(dry_run)
    exec_command(['cargo', 'build', '--release', '--no-default-features', '--features', 'forest_fil_cns,paritydb'],
                 dry_run)
  end
end

# Benchmark class for Forest with ParityDb backend and Jemalloc allocator
class JemallocBenchmark < ForestBenchmark
  def build_command(dry_run)
    exec_command(
      ['cargo', 'build', '--release', '--no-default-features', '--features',
       'forest_fil_cns,paritydb,jemalloc'], dry_run
    )
  end
end

# Benchmark class for Forest with ParityDb backend and Mimalloc allocator
class MimallocBenchmark < ForestBenchmark
  def build_command(dry_run)
    exec_command(
      ['cargo', 'build', '--release', '--no-default-features', '--features',
       'forest_fil_cns,paritydb,mimalloc'], dry_run
    )
  end
end

# Lotus benchmark class
class LotusBenchmark < Benchmark
  def db_dir
    lotus_path = ENV['LOTUS_PATH']# || "#{Dir.home}/.lotus"
    "#{lotus_path}/datastore/chain"
  end

  def db_size
    size = syscall('du', '-h', db_dir)
    size.split[0]
  end

  def clean_db(dry_run)
    FileUtils.rm_rf(db_dir, secure: true) unless dry_run
  end

  def build_config_file
    # No support for passing custom config file right now
  end
  private :build_config_file

  def repository_name
    'lotus'
  end

  def target
    File.join('.', repository_name, 'lotus')
  end

  def clone_command(dry_run)
    exec_command(['git', 'clone', 'https://github.com/filecoin-project/lotus.git', repository_name], dry_run)
  end

  def checkout_command(dry_run)
    if @chain == 'mainnet' then
      exec_command(%w[git checkout releases], dry_run)
    else
      exec_command(%w[git checkout master], dry_run)
    end
  end

  def clean_command(dry_run)
    exec_command(%w[make clean], dry_run)
  end

  def build_command(dry_run)
    exec_command(['make', @chain == 'mainnet' ? 'all' : 'calibnet'], dry_run)
  end

  def epoch_command
    begin
      output = syscall(target, 'sync', 'status')
    rescue RuntimeError
      return nil
    end
    msg_sync = output.match(/Stage: message sync/m)
    if msg_sync
      # TODO: merge matches since lotus prints workers that could be at different stages
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
    ENV['LOTUS_PATH'] = File.join(BENCHMARK_DIR, ".#{repository_name}")
    @name = name
    @config = config
    @import_command = [
      target, 'daemon', '--import-snapshot', '%<s>s', '--halt-after-import'
    ]
    @validate_online_command = [
      target, 'daemon'
    ]
    @sync_status_command = [
      target, 'sync', 'status'
    ]
    @metrics = {}
  end
end

def run_benchmarks(benchmarks, options)
  bench_metrics = {}
  snapshot_abs_path = File.expand_path(options[:snapshot_path])
  puts "(I) Using snapshot: #{snapshot_abs_path}"
  puts "(I) BENCHMARK_DIR: #{BENCHMARK_DIR}"
  puts ''
  Dir.chdir(BENCHMARK_DIR) do
    benchmarks.each do |bench|
      bench.snapshot_path = snapshot_abs_path
      bench.heights = options[:heights]
      bench.chain = options[:chain]
      bench.run(options[:dry_run], options[:daily])

      bench_metrics[bench.name] = bench.metrics

      puts "\n"
    end
  end
  if options[:daily]
    write_csv(bench_metrics)
  else
    write_markdown(bench_metrics)
  end

  # puts bench_metrics
end

BENCHMARKS = [
  ForestBenchmark.new(name: 'baseline'),
  ParityDbBenchmark.new(name: 'paritydb'),
  JemallocBenchmark.new(name: 'paritydb-jemalloc'),
  MimallocBenchmark.new(name: 'paritydb-mimalloc')
].freeze

options = {
  heights: HEIGHTS_TO_VALIDATE,
  pattern: 'baseline',
  chain: 'calibnet', # TODO: replace with 'mainnet' before merging
  daily: true
}
OptionParser.new do |opts|
  opts.banner = 'Usage: bench.rb [options] snapshot'
  opts.on('--dry-run', 'Only print the commands that will be run') { |v| options[:dry_run] = v }
  opts.on('--heights [Integer]', Integer, 'Number of heights to validate') { |v| options[:heights] = v }
  opts.on('--pattern [String]', 'Run benchmarks that match the pattern') { |v| options[:pattern] = v }
  opts.on('--chain [String]', 'Choose network chain [default: mainnet]') { |v| options[:chain] = v }
end.parse!

snapshot_path = ARGV.pop
raise "The file '#{snapshot_path}' does not exist" if snapshot_path && !File.file?(snapshot_path)

if snapshot_path.nil?
  puts '(I) No snapshot provided, downloading one'
  snapshot_path = download_snapshot(chain: options[:chain])
  puts snapshot_path
end

options[:snapshot_path] = snapshot_path

if options[:daily]
  selection = Set[
    #ForestBenchmark.new(name: 'forest'),
    LotusBenchmark.new(name: 'lotus')
  ]
  run_benchmarks(selection, options)
else
  selection = Set[]
  BENCHMARKS.each do |bench|
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
