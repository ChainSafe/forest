# frozen_string_literal: true

require 'docker'

# Tools to facilitate interacting with Docker
module DockerUtils
  # returns the specified container logs as String
  def self.get_container_logs(container_name)
    container = Docker::Container.get container_name
    container.streaming_logs(stdout: true, stderr: true)
  end
end
