require 'json'
require 'cgi'
require 'incisive/version'
require 'incisive/errors'
require 'incisive/http'
require 'incisive/client/publishing_methods'
require 'incisive/client/querying_methods'
require 'incisive/client/maintenance_methods'

module Incisive
  class Client
    include PublishingMethods
    include QueryingMethods
    include MaintenanceMethods

    DEFAULT_API_URL = "http://localhost:8080".freeze
    DEFAULT_API_VERSION = "1.0".freeze

    attr_accessor :project_id, :write_key, :read_key, :master_key, :api_url,
                  :api_version, :proxy_url, :proxy_type, :read_timeout,
                  :open_timeout, :log_queries

    alias_method :base_url, :api_url
    alias_method :base_url=, :api_url=

    # Build a client from INCISIVE_* environment variables.
    def self.from_env(env = ENV)
      new(
        project_id: env['INCISIVE_PROJECT_ID'],
        write_key: env['INCISIVE_WRITE_KEY'],
        read_key: env['INCISIVE_READ_KEY'],
        master_key: env['INCISIVE_MASTER_KEY'],
        api_url: env['INCISIVE_BASE_URL'] || env['INCISIVE_API_URL'],
        api_version: env['INCISIVE_API_VERSION'],
        proxy_url: env['INCISIVE_PROXY_URL'],
        proxy_type: env['INCISIVE_PROXY_TYPE'],
        read_timeout: env['INCISIVE_READ_TIMEOUT'],
        open_timeout: env['INCISIVE_OPEN_TIMEOUT']
      )
    end

    # Accepts an options hash, or the legacy positional form
    # `Client.new(project_id, write_key, read_key, extra_options = {})`.
    def initialize(*args)
      options = args[0]
      unless options.is_a?(Hash)
        options = { project_id: args[0], write_key: args[1], read_key: args[2] }
                  .merge(args[3] || {})
      end
      options = options.transform_keys(&:to_sym)

      @project_id = options[:project_id]
      @write_key = options[:write_key]
      @read_key = options[:read_key]
      @master_key = options[:master_key]
      @api_url = options[:api_url] || options[:base_url] || DEFAULT_API_URL
      @api_version = (options[:api_version] || DEFAULT_API_VERSION).to_s
      @proxy_url = options[:proxy_url]
      @proxy_type = options[:proxy_type]
      @read_timeout = options[:read_timeout].to_f unless options[:read_timeout].nil?
      @open_timeout = options[:open_timeout].to_f unless options[:open_timeout].nil?
      @log_queries = options[:log_queries]
    end

    def saved_queries
      @saved_queries ||= SavedQueries.new(self)
    end

    def cached_datasets
      @cached_datasets ||= CachedDatasets.new(self)
    end

    def access_keys
      @access_keys ||= AccessKeys.new(self)
    end

    # Request headers. `sync_or_async` is accepted for call compatibility.
    def api_headers(authorization, _sync_or_async = "sync")
      {
        "Content-Type" => "application/json",
        "User-Agent" => "incisive-ruby/#{Incisive::VERSION}",
        "Authorization" => authorization
      }
    end

    # Maps a status code + body to a return value or an exception.
    #   2xx JSON  -> parsed JSON (invalid JSON -> {} with a warning)
    #   204       -> true
    #   400/401/404 -> BadRequestError / AuthenticationError / NotFoundError
    #   other     -> HttpError
    def process_response(status_code, response_body)
      status = status_code.to_i
      body = response_body.to_s
      case status
      when 204
        true
      when 200..299
        begin
          JSON.parse(body)
        rescue JSON::ParserError
          Incisive.logger.warn("Invalid JSON for response code #{status}: #{body}")
          {}
        end
      when 400 then raise BadRequestError.new(body, nil, status: status, body: body)
      when 401 then raise AuthenticationError.new(body, nil, status: status, body: body)
      when 404 then raise NotFoundError.new(body, nil, status: status, body: body)
      else raise HttpError.new(body, nil, status: status, body: body)
      end
    end

    def ensure_project_id!
      raise ConfigurationError, "Project ID must be set" unless project_id
    end

    def ensure_write_key!
      raise ConfigurationError, "Write Key must be set for this operation" unless write_key
    end

    def ensure_master_key!
      raise ConfigurationError, "Master Key must be set for this operation" unless master_key
    end

    def ensure_read_key!
      raise ConfigurationError, "Read Key must be set for this operation" unless read_key
    end

    # Base path for this project, e.g. "/1.0/projects/abc".
    def project_path
      "/#{api_version}/projects/#{project_id}"
    end

    # Escape one URL path segment: everything except RFC 3986 unreserved
    # characters, sub-delims and ':' / '@' is percent-encoded (so '/', '?', '#',
    # '%' and spaces are always escaped).
    def self.escape_segment(value)
      value.to_s.b.gsub(%r{[^A-Za-z0-9\-._~!$&'()*+,;=:@]}) { |c| format("%%%02X", c.ord) }
    end

    # Runs one HTTP request and returns the Net::HTTPResponse; transport
    # errors are wrapped in HttpError.
    def http_request(method, path, key, body: nil, headers: nil, label: nil)
      hdrs = api_headers(key, "sync")
      hdrs = hdrs.merge(headers) if headers
      http = HTTP::Sync.new(api_url, proxy_url, read_timeout, open_timeout)
      args = { path: path, headers: hdrs }
      args[:body] = body if %i[post put].include?(method)
      http.public_send(method, **args)
    rescue Incisive::Error
      raise
    rescue StandardError, Timeout::Error => e
      raise HttpError.new("HTTP #{label || method} failure: #{e.message}", e)
    end

    def request_and_process(method, path, key, body: nil, headers: nil, label: nil)
      response = http_request(method, path, key, body: body, headers: headers, label: label)
      process_response(response.code, response.body.to_s.chomp)
    end

    private

    def api_event_collection_resource_path(event_collection)
      "#{project_path}/events/#{Client.escape_segment(event_collection)}"
    end

    # Serialize query parameters the way the analysis endpoints expect:
    # nils dropped; filters/steps/analyses always JSON; any other Hash or
    # Array JSON; max_age only if numeric; everything else stringified; each
    # value form-escaped (space -> '+').
    def preprocess_params(params)
      params = params.reject { |_k, v| v.nil? }
      pairs = params.map do |key, value|
        str =
          case key.to_sym
          when :filters, :steps, :analyses
            JSON.generate(value)
          when :max_age
            next nil unless value.is_a?(Numeric)
            value.to_s
          else
            value.is_a?(Hash) || value.is_a?(Array) ? JSON.generate(value) : value.to_s
          end
        "#{key}=#{CGI.escape(str)}"
      end
      pairs.compact.join('&')
    end
  end
end
