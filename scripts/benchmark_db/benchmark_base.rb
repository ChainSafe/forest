# frozen_string_literal: true

# Mixin module for base benchmark class exec (and exec helper) commands.
module ExecCommands
  # Helper function used in calculation of number of epochs.
  def get_last_epoch(benchmark)
    epoch = nil
    while epoch.nil?
      # Epoch can be nil (e.g. if the client is in the "fetchting messages" stage).
      epoch = benchmark.epoch_command
      sleep 0.5
    end
    epoch
  end

  # Measures validation time for daily metrics.
  def measure_online_validation(benchmark, pid, metrics)
    Thread.new do
      first_epoch = nil
      loop do
        start, first_epoch = benchmark.start_online_validation_command
        if start
          @logger.info 'Start measure'
          break
        end
        sleep 0.1
      end
      sleep benchmark.online_validation_secs
      last_epoch = get_last_epoch(benchmark)
      metrics[:num_epochs] = last_epoch - first_epoch

      @logger.info 'Stopping process...'
      benchmark.stop_command(pid)
    end
  end

  # Calls online validation function and runs monitor to measure memory usage.
  def proc_monitor(pid, benchmark)
    metrics = Concurrent::Hash.new
    metrics[:rss] = []
    metrics[:vsz] = []
    measure_online_validation(benchmark, pid, metrics) if benchmark
    handle = Thread.new do
      loop do
        sample_proc(pid, metrics)
        sleep 0.5
      # Loop will error during clean and build steps, so need to break until
      # client is running.
      rescue StandardError => _e
        break
      # Need to handle interrupt or process will continue on `ctrl-c`.
      rescue Interrupt
        stop_command(pid)
        break
      end
    end
    [handle, metrics]
  end

  # Helper function for measuring execution time; passes process ID to online
  # validation and process monitor.
  def exec_command_aux(command, metrics, benchmark)
    Open3.popen2(*command) do |i, o, t|
      pid = t.pid
      i.close

      handle, proc_metrics = proc_monitor(pid, benchmark)
      o.each_line do |l|
        print l
      end

      handle.join
      metrics.merge!(proc_metrics)
    end
  end

  # Measures execution time of command.
  def exec_command(command, benchmark = nil)
    @logger.info "$ #{command.join(' ')}"
    return {} if @dry_run

    metrics = Concurrent::Hash.new
    t0 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
    exec_command_aux(command, metrics, benchmark)
    t1 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
    metrics[:elapsed] = trunc_seconds(t1 - t0)
    metrics
  end
end

# Mixin module for base benchmark class build commands.
module BuildCommands
  # Helper function to build config file for `build_artefacts` function
  def build_config_file
    config = @config.deep_merge(default_config)
    config_path = "#{data_dir}/#{@name}.toml"

    toml_str = TomlRB.dump(config)
    File.write(config_path, toml_str)
  end
  private :build_config_file

  # Helper function for `build_artefacts` function; defines config path, snapshot
  # path, and start epoch for building and running client.
  def build_substitution_hash
    height = snapshot_height(@snapshot_path)
    start = height - @heights

    return { c: '<tbd>', s: '<tbd>', h: start } if @dry_run

    config_path = "#{data_dir}/#{@name}.toml"

    { c: config_path, s: @snapshot_path, h: start }
  end
  private :build_substitution_hash

  # Helper function for `build_artefacts`; clones repository, checks out branch,
  # runs clean command, and builds client.
  def build_client
    if Dir.exist?(repository_name)
      @logger.warn "Directory #{repository_name} is already present"
    else
      @logger.info 'Cloning repository'
      clone_command
      Dir.mkdir(repository_name) if @dry_run
      @created_repository = true
    end

    @logger.info 'Clean and build client'
    Dir.chdir(repository_name) do
      checkout_command
      clean_command
      build_command
    end
  end

  # Calls all helper functions to prepare each client to run benchmarks.
  def build_artefacts
    @logger.info 'Building artefacts...'
    build_client

    build_config_file unless @dry_run
    build_substitution_hash
  end
  private :build_artefacts
end

# Mixin module for base benchmark class run and validation commands.
module RunCommands
  # Create and call proper validation command, then write results to metrics.
  def run_validation_step(daily, args, metrics)
    unless daily
      validate_command = splice_args(@validate_command, args)
      metrics[:validate] = exec_command(validate_command)
      return
    end

    validate_online_command = splice_args(@validate_online_command, args)
    new_metrics = exec_command(validate_online_command, self)
    new_metrics[:tpm] =
      new_metrics[:num_epochs] ? new_metrics[:num_epochs] / online_validation_secs : 'n/a'
    new_metrics[:tpm] = new_metrics[:tpm].ceil(3)
    metrics[:validate_online] = new_metrics
  end

  # Import snapshot, write metrics, and call validation function, returning metrics.
  def import_and_validation(daily, args, metrics)
    import_command = splice_args(@import_command, args)
    metrics[:import] = exec_command(import_command)

    # Save db size just after import.
    metrics[:import][:db_size] = db_size unless @dry_run

    run_validation_step(daily, args, metrics)
    metrics
  rescue StandardError, Interrupt
    @logger.error('Fiasco during benchmark run. Deleting downloaded files, cleaning DB and stopping process...')
    FileUtils.rm_f(@snapshot_path) if @snapshot_downloaded
    clean_db
    FileUtils.rm_rf(repository_name) if @created_repository
    exit(1)
  end

  def forest_init(args)
    @init_command = splice_args(@init_command, args)
    exec_command(@init_command)
  end

  # This is the primary function called in `bench.rb` to run the metrics for
  # each benchmark.
  def run(daily, snapshot_downloaded)
    begin
      @snapshot_downloaded = snapshot_downloaded
      @logger.info "Running bench: #{@name}"

      metrics = Concurrent::Hash.new
      args = build_artefacts
      @sync_status_command = splice_args(@sync_status_command, args)

      forest_init(args) if @name == 'forest'

      @metrics = import_and_validation(daily, args, metrics)
    rescue StandardError, Interrupt
      @logger.error('Fiasco during benchmark run. Deleting downloaded files and stopping process...')
      FileUtils.rm_f(@snapshot_path) if @snapshot_downloaded
      FileUtils.rm_rf(repository_name) if @created_repository
      exit(1)
    end

    @logger.info 'Cleaning database'
    clean_db
    @logger.info 'Deleting downloaded repository'
    delete_repository
  end

  # Number of seconds to run online validation is defined here.
  def online_validation_secs
    @chain == 'mainnet' ? 120.0 : 10.0
  end

  # Check to see if current epoch is at least `10` epochs past snapshot height.
  def start_online_validation_command
    snapshot_height = snapshot_height(@snapshot_path)
    current = epoch_command
    if current
      @logger.info "@#{current}"
      # Check if we can start the measure.
      [current >= snapshot_height + 10, current]
    else
      [false, nil]
    end
  end

  # Raise an error if repository name is not defined in each class instance.
  def repository_name
    raise 'repository_name method should be implemented'
  end

  # Create repository data directory.
  def data_dir
    current_dir = Dir.pwd
    path = "#{current_dir}/.#{repository_name}"
    FileUtils.mkdir_p path
    path
  end

  # Helper function for deleting repository after successful run.
  def delete_repository
    FileUtils.rm_rf(repository_name) if @created_repository
  end
end

# Base benchmark class (not usable on its own).
class BenchmarkBase
  include ExecCommands
  include BuildCommands
  include RunCommands
  attr_reader :name, :metrics
  attr_accessor :dry_run, :snapshot_path, :heights, :chain

  def initialize(name:, config: {})
    @name = name
    @config = config
    @logger = Logger.new($stdout)
    @created_repository = false
    @working_dir = WORKING_DIR
  end
end
