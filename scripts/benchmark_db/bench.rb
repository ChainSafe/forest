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
require_relative 'forest_bench'
require_relative 'lotus_bench'
require_relative 'benchmark_base'

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
