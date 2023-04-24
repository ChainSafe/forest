# frozen_string_literal: true

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
    new_metrics[:tpm] =
      new_metrics[:num_epochs] ? (MINUTE * new_metrics[:num_epochs]) / online_validation_secs : 'n/a'
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
