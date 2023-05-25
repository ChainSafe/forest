#!/usr/bin/env ruby

# frozen_string_literal: true

# Script to test various configurations that can impact performance of the node

require 'concurrent-ruby'
require 'csv'
require 'deep_merge'
require 'fileutils'
require 'logger'
require 'open3'
require 'optparse'
require 'set'
require 'tmpdir'
require 'tomlrb'
require 'toml-rb'
require_relative 'forest_bench'
require_relative 'lotus_bench'
require_relative 'benchmark_base'

# Define `regex` for capturing the snapshot height.
SNAPSHOT_REGEXES = [
  /_height_(?<height>\d+)\.car/,
  %r{/(?<height>\d+)_*}
].freeze

HEIGHTS_TO_VALIDATE = 40

MINUTE = 60
HOUR = MINUTE * MINUTE

@logger = Logger.new($stdout)

# Define default options and parse command line options.
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

# Create random temporary directory (or user-specified dir) for benchmarks,
# and set appropriate permissions to allow script to run properly.
WORKING_DIR = if options[:tempdir].nil?
                Dir.mktmpdir('benchmark-')
              else
                FileUtils.mkdir_p options[:tempdir]
                options[:tempdir]
              end
FileUtils.chmod 0o744, WORKING_DIR

# Provides human readable formatting to Numeric class.
class Numeric
  def to_bibyte
    syscall('numfmt', '--to=iec-i', '--suffix=B', '--format=%.2f', to_s)
  end
end

# Helper function to capture standard output of commands.
def syscall(*command, chdir: '.')
  stdout, _stderr, status = Open3.capture3(*command, chdir: chdir)

  status.success? ? stdout.chomp : (raise "#{command}, #{status}")
end

# Convert raw seconds to a nicer format for readability, displaying one decimal
# point if less than `60` seconds and rounding to whole numbers if greater than
# `60` seconds.
def trunc_seconds(seconds)
  seconds < MINUTE ? seconds.ceil(1) : seconds.ceil(0)
end

# Updates physical and virutal memory size metrics.
def sample_proc(pid, metrics)
  output = syscall('ps', '-o', 'rss,vsz', pid.to_s)
  output = output.split("\n").last.split
  metrics[:rss].push(output[0].to_i)
  metrics[:vsz].push(output[1].to_i)
end

# Populate import table with metrics.
def format_import_table_row(key, value)
  elapsed = value[:import][:elapsed] || 'n/a'
  rss = value[:import][:rss]
  rss_max = rss ? (rss.max * 1024).to_bibyte : 'n/a'
  vsz = value[:import][:vsz]
  vsz_max = vsz ? (vsz.max * 1024).to_bibyte : 'n/a'
  db_size = value[:import][:db_size] || 'n/a'
  "#{key} | #{elapsed} | #{rss_max} | #{vsz_max} | #{db_size}\n"
end

# Format import table and call function to populate metrics.
def format_import_table(metrics)
  result = "Bench | Import Time [sec] | Import RSS | Import VSZ | DB Size\n"
  result += "-|-|-|-|-\n"

  metrics.each do |key, value|
    result += format_import_table_row(key, value)
  end

  result
end

# Format validation table and populate table with metric.
def format_validate_table(metrics)
  result = "Bench | Validate Time [sec] | Validate RSS | Validate VSZ\n"
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

# Output database benchmark metrics to markdown file.
def write_markdown(metrics)
  # Output file is a suite of markdown tables.
  result = ''
  result += format_import_table(metrics)
  result += "---\n"
  result += format_validate_table(metrics)

  filename = "result_#{Time.now.to_i}.md"
  File.write(filename, result)
  @logger.info "Wrote #{filename}"
end

# Output daily benchmark metrics to comma-separated value file.
def write_csv(metrics)
  filename = "result_#{Time.now.to_i}.csv"
  CSV.open(filename, 'w') do |csv|
    csv << ['Client', 'Snapshot Import Time [sec]', 'Validation Time [tipsets/sec]']

    metrics.each do |key, value|
      elapsed = value[:import][:elapsed] || 'n/a'
      tpm = value[:validate_online][:tpm] || 'n/a'

      csv << [key, elapsed, tpm]
    end
  end
  @logger.info "Wrote #{filename}"
end

def splice_args(command, args)
  command.map { |s| s % args }
end

# Capture snapshot height from snapshot path.
def snapshot_height(snapshot)
  SNAPSHOT_REGEXES.each do |regex|
    match = snapshot.match(regex)
    return match.named_captures['height'].to_i if match
  end
  raise 'unsupported snapshot name'
end

# Determine correct snapshot download URL path based on value of chain.
def get_url(chain, url)
  value = chain == 'mainnet' ? 'mainnet' : 'calibrationnet'
  output = syscall('aria2c', '--dry-run', "https://snapshots.#{value}.filops.net/minimal/latest.zst")
  url || output.match(%r{Redirecting to (https://.+?\d+_.+)})[1]
end

# Helper function to create assignments for `download_and_move` function.
def download_and_move_assignments(url)
  filename = url.match(/(\d+_.+)/)[1]
  checksum_url = url.sub(/\.car\.zst/, '.sha256sum')
  checksum_filename = checksum_url.match(/(\d+_.+)/)[1]
  decompressed_filename = filename.sub(/\.car\.zst/, '.car')
  [filename, checksum_url, checksum_filename, decompressed_filename]
end

# Create snapshot directory; download checksum and compressed snapshot;
# decompress and verify snapshot; clean up helper files and return path to snapshot.
def download_and_move(url, output_dir)
  filename, checksum_url, checksum_filename, decompressed_filename = download_and_move_assignments(url)

  # Must move to `WORKING_DIR` and create temporary folder for temp snapshot
  # files, as filesystem `tmp` partition may not be large enough for this task.
  Dir.chdir(WORKING_DIR)
  snapshot_dir = Dir.pwd
  FileUtils.mkdir_p('snapshot_dl_files')
  Dir.chdir('snapshot_dl_files') do
    # Download, decompress and verify checksums.
    @logger.info 'Downloading checksum...'
    syscall('aria2c', checksum_url)
    @logger.info 'Downloading snapshot...'
    syscall('aria2c', '-x5', url)
    @logger.info "Decompressing #{filename}..."
    syscall('zstd', '-d', filename)
    @logger.info 'Verifying...'
    syscall('sha256sum', '--check', '--status', checksum_filename)

    FileUtils.mv(decompressed_filename.to_s, snapshot_dir)
  end
  FileUtils.rm_rf('snapshot_dl_files')
  "#{output_dir}/#{decompressed_filename}"
end

# Get proper snapshot download URL based on chain value and download to
# `WORKING_DIR`; define `snapshot_path` instance variable based on the
# download location for use in benchmarks, error handling, and cleanup after run.
def download_snapshot(output_dir: WORKING_DIR, chain: 'calibnet', url: nil)
  @logger.info "output_dir: #{output_dir}"
  @logger.info "chain: #{chain}"
  url = get_url(chain, url)
  @logger.info "snapshot_url: #{url}"

  current_dir = Dir.pwd
  @snapshot_path = download_and_move(url, output_dir)
  Dir.chdir(current_dir)
end

# Helper function for `run_benchmarks` to loop through benchmarks selection,
# run metrics, and assign metrics for each benchmark.
def benchmarks_loop(benchmarks, options, bench_metrics)
  benchmarks.each do |bench|
    bench.dry_run, bench.snapshot_path, bench.heights, bench.chain = bench_loop_assignments(options)
    bench.run(options[:daily], @snapshot_downloaded)

    bench_metrics[bench.name] = bench.metrics

    puts "\n"
  rescue StandardError, Interrupt
    @logger.error('Fiasco during benchmark run. Exiting...')
    # Delete snapshot if downloaded, but not if user-provided.
    FileUtils.rm_f(@snapshot_path) if @snapshot_downloaded
    exit(1)
  end
end

# Helper function for to create assignments for `benchmarks_loop` function.
def bench_loop_assignments(options)
  [options[:dry_run], options[:snapshot_path], options[:heights], options[:chain]]
end

# Run benchmarks and write to `CSV` if daily or markdown file if `DB` benchmarks.
def run_benchmarks(benchmarks, options)
  bench_metrics = Concurrent::Hash.new
  options[:snapshot_path] = File.expand_path(options[:snapshot_path])
  @logger.info "Using snapshot: #{options[:snapshot_path]}"
  @logger.info "WORKING_DIR: #{WORKING_DIR}"
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

# Benchmarks for database metrics.
FOREST_BENCHMARKS = [
  ForestBenchmark.new(name: 'baseline'),
  SysAllocBenchmark.new(name: 'sysalloc'),
  MimallocBenchmark.new(name: 'mimalloc')
].freeze

# Get snapshot path from command line arguments.
@snapshot_path = ARGV.pop
raise "The file '#{@snapshot_path}' does not exist" if @snapshot_path && !File.file?(@snapshot_path)

# Set variable to false for use later during error handling and cleanup after run.
@snapshot_downloaded = false

# Download snapshot if a snapshot path is not specified by the user.
begin
  if @snapshot_path.nil?
    @logger.info 'No snapshot provided, downloading one'
    download_snapshot(chain: options[:chain])
    @logger.info "Snapshot successfully downloaded to: #{@snapshot_path}"
    @snapshot_downloaded = true
    # If a fresh snapshot is downloaded, allow network to move ahead, otherwise
    # `message sync` phase may not be long enough for validation metric.
    if options[:daily]
      @logger.info 'Fresh snapshot; sleeping while network advances for 5 minutes...'
      sleep 300
    end
  end
rescue StandardError, Interrupt
  @logger.error('Fiasco during snapshot download. Deleting snapshot and exiting...')
  # Delete downloaded snapshot if it exists.
  FileUtils.rm_f(@snapshot_path) unless @snapshot_path.nil?
  FileUtils.rm_rf("#{WORKING_DIR}/snapshot_dl_files")
  exit(1)
end

# Define snapshot path in options to pass to the benchmark run.
options[:snapshot_path] = @snapshot_path

# Run metrics based on daily flag setting.
if options[:daily]
  # Benchmarks for daily metrics.
  selection = Set[
    ForestBenchmark.new(name: 'forest'),
    LotusBenchmark.new(name: 'lotus')
  ]
  run_benchmarks(selection, options)
else
  # Benchmarks for database metrics.
  selection = Set[]
  FOREST_BENCHMARKS.each do |bench|
    options[:pattern].split(',').each do |pat|
      selection.add(bench) if File.fnmatch(pat.strip, bench.name)
    end
  end
  if selection.empty?
    @logger.info 'Nothing to run'
  else
    run_benchmarks(selection, options)
  end
end

# After benchmarks are complete, delete snapshot if downloaded, but not if user-provided.
FileUtils.rm_f(@snapshot_path) if @snapshot_downloaded
