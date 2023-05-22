# frozen_string_literal: true

require_relative 'benchmark_base'

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
    @init_command = [target_cli, '--config', '%<c>s', 'fetch-params', '--keys']
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
