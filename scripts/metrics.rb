#!/usr/bin/env ruby

require 'csv'
require 'open3'
require 'optparse'
require 'set'
require 'tmpdir'
require 'toml-rb'

MINUTE = 60
HOUR = MINUTE * MINUTE

TEMP_DIR = Dir.mktmpdir('forest-metrics-')

def syscall(*command)
    stdout, _stderr, status = Open3.capture3(*command)
  
    #status.success? ? stdout.chomp : (raise "#{command}, #{status}")
    return stdout, _stderr, status
end

def splice_args(command, args)
    command.map { |s| s % args }
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

def exec_command(command)
    puts "$ #{command.join(' ')}"

    metrics = {}
    t0 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
    exec_command_aux(command, metrics)
    t1 = Process.clock_gettime(Process::CLOCK_MONOTONIC)
    metrics[:elapsed] = hr(t1 - t0)
    metrics
end

def build_forest_command
    ['cargo', 'build', '--release']
  end

def build_forest_artefacts(snapshot_path)
    # TODO: remove comments when finished with script
    #exec_command(%w[cargo clean])
    #exec_command(build_forest_command)
    build_substitution_hash(snapshot_path)
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

def forest_target
    './target/release/forest'
end

def lotus_target
    '.target/lotus'
end

def build_substitution_hash(snapshot_path)
    snapshot = snapshot_path

    snapshot_path = File.file?(snapshot) ? snapshot : "#{snapshot_dir}/#{snapshot}"

    { s: snapshot_path }
end
private :build_substitution_hash





puts "Running script"
# Pop snapshot path off end of command line arguments
snapshot_path = ARGV.pop
# Raise error if there is no snapshot path provided
raise OptionParser::ParseError, 'need to specify a snapshot for running metrics' unless snapshot_path
metrics = {}
args = build_forest_artefacts(snapshot_path)
forest_import_command = ['time', 
    forest_target, '--chain', 'calibnet', '--encrypt-keystore', 'false', '--import-snapshot', '%<s>s', '--halt-after-import'
  ]
import_command = splice_args(forest_import_command, args)
metrics[:import] = exec_command(import_command)
#output = syscall('time', 
#    forest_target, '--chain', 'calibnet', '--encrypt-keystore', 'false', '--import-snapshot', import_command[7], '--halt-after-import')
#puts output[1].split("\n").last(2).first.split(" ")
#import_time = output[1].split("\n").last(2).first.split(" ")[2].delete("elapsed")
import_time = metrics[:import][:elapsed].delete('s')
puts import_time

test_import_command = [forest_target, '--chain', 'calibnet', '--encrypt-keystore', 'false', '--import-snapshot', '%<s>s'
]
import_command = splice_args(test_import_command, args)



daemon = Thread.new { 
  exec_command(import_command)
}
sleep(30)

sync_stage = ""
while sync_stage != "message"
  sync_status = syscall('forest-cli', 'sync', 'status')
  sync_stage = sync_status[0].split("\n").last(3).first.split(' ').drop(1)[0]
end
start_height = sync_status[0].split("\n").last(2).first.split(' ').drop(1)
puts sync_status
puts start_height
sleep(10)
sync_status = syscall('forest-cli', 'sync', 'status')
end_height = sync_status[0].split("\n").last(2).first.split(' ').drop(1)
puts sync_status
puts end_height
height_diff = end_height[0].to_i - start_height[0].to_i
validation_time = height_diff / 10.0
puts validation_time
exec_command(%w[pkill -9 forest])
daemon.exit

# Export to CSV
CSV.open('results.csv', 'w') do |csv|
  csv << ["Client", "Snapshot Import Time [sec]", "Validation Time [tipsets/sec]"]
  csv << ["Forest", import_time, validation_time]
end




#sync_status = exec_command(%w[forest-cli sync status])
#puts sync_stage
# daemon = Thread.new { metrics[:import] = exec_command(import_command) }
# sleep(30)
# killer = Thread.new { exec_command(%w[pkill -9 forest]) }
# daemon.exit
