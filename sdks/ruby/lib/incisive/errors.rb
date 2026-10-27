require 'json'

module Incisive
  # Base error. `message` is the raw detail (for HTTP errors, the raw response
  # body), and `to_s` adds a fixed prefix.
  class Error < RuntimeError
    attr_accessor :original_error

    def initialize(message, original_error = nil)
      self.original_error = original_error
      super(message)
    end

    def to_s
      "Incisive Exception: #{super}"
    end
  end

  class ConfigurationError < Error; end

  # Raised for non-success HTTP statuses and for transport failures
  # (status is nil for the latter; original_error holds the cause).
  class HttpError < Error
    attr_reader :status, :body, :error_code, :api_message

    def initialize(message, original_error = nil, status: nil, body: nil)
      super(message, original_error)
      @status = status
      @body = body
      parsed = begin
        body && !body.empty? ? JSON.parse(body) : nil
      rescue JSON::ParserError
        nil
      end
      if parsed.is_a?(Hash)
        @error_code = parsed["error_code"]
        @api_message = parsed["message"]
      end
    end
  end

  class BadRequestError < HttpError; end
  class AuthenticationError < HttpError; end
  class NotFoundError < HttpError; end
end
