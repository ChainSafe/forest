# frozen_string_literal: true

require_relative 'benchmark_base'

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
    Dir.chdir(@working_dir) do
      current_dir = Dir.pwd
      ENV['LOTUS_PATH'] = File.join(current_dir, ".#{repository_name}")
    end
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
