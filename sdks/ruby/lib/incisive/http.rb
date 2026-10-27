require 'uri'
require 'net/http'

module Incisive
  module HTTP
    # Thin synchronous transport over Net::HTTP. One connection per request.
    class Sync
      def initialize(base_url, proxy_url = nil, read_timeout = nil, open_timeout = nil)
        uri = URI.parse(base_url)
        args = [uri.host, uri.port]
        if proxy_url
          p = URI.parse(proxy_url)
          args += [p.host, p.port, p.user, p.password]
        else
          args += [nil] # disable ENV http_proxy pickup for explicit control
        end
        @http = Net::HTTP.new(*args)
        @http.open_timeout = open_timeout if open_timeout
        @http.read_timeout = read_timeout if read_timeout
        if uri.scheme == "https"
          @http.use_ssl = true
          @http.verify_mode = OpenSSL::SSL::VERIFY_PEER
        end
        @prefix = uri.path.to_s.sub(%r{/\z}, "")
      end

      def get(path:, headers:)
        request(Net::HTTP::Get, path, headers)
      end

      def delete(path:, headers:)
        request(Net::HTTP::Delete, path, headers)
      end

      def post(path:, headers:, body:)
        request(Net::HTTP::Post, path, headers, body)
      end

      def put(path:, headers:, body:)
        request(Net::HTTP::Put, path, headers, body)
      end

      private

      def request(klass, path, headers, body = nil)
        req = klass.new(@prefix + path)
        # Net::HTTP adds its own defaults; drop the ones we do not want on the wire.
        req.delete("accept-encoding") unless headers.key?("Accept-Encoding")
        headers.each { |k, v| req[k] = v unless v.nil? }
        req.body = body unless body.nil?
        @http.request(req)
      end
    end
  end
end
