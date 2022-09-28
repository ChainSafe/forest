# frozen_string_literal: true

require 'slack-ruby-client'

# Wrapper Slack client class to handle sending messages and uploading logs.
class SlackClient
  @last_thread = nil
  @channel = nil
  @client = nil

  def initialize(channel, token)
    raise "Invalid channel name: #{channel}, must start with \#" unless channel.start_with? '#'
    raise 'Missing token' if token.nil?

    Slack.configure do |config|
      config.token = token
    end

    @channel = channel
    @client = Slack::Web::Client.new
  end

  # Posts a new message to configured channel.
  def post_message(text)
    msg = @client.chat_postMessage(channel: @channel, text: text)
    @last_thread = msg[:ts]
  end

  # Attaches a comment/reply to the latest posted thread.
  def attach_comment(comment)
    raise 'Need to create a thread before attaching a comment.' if @last_thread.nil?

    @client.chat_postMessage(channel: @channel, thread_ts: @last_thread, text: comment)
  end

  # Attaches files to the last posted thread.
  def attach_files(*files)
    files.each do |file|
      attach_file file
    end
  end

  # Attaches a file to the latest posted thread.
  def attach_file(file)
    raise "No such file #{file}" unless File.exist? file
    raise 'Need to create a thread before attaching a file.' if @last_thread.nil?

    @client.files_upload(
      channels: @channel,
      file: Faraday::UploadIO.new(file, 'text/plain'),
      filename: File.basename(file),
      initial_comment: 'Attached a file.',
      thread_ts: @last_thread
    )
  end
end
