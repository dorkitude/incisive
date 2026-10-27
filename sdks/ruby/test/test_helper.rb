$LOAD_PATH.unshift File.expand_path("../lib", __dir__)
require "minitest/autorun"
require "socket"
require "json"
require "uri"
require "cgi"
require "incisive"

Incisive.logger = Logger.new(IO::NULL)

# Minimal in-process HTTP/1.1 server on 127.0.0.1:<random>. Records each
# request and replies with queued [status, body, headers] responses
# (default 200 {"result": 1}).
class MockServer
  Request = Struct.new(:method, :target, :path, :query, :headers, :body, keyword_init: true) do
    # Query string decoded into a Hash of key => [values].
    def params
      CGI.parse(query.to_s)
    end

    # Single decoded value for a parameter.
    def param(name)
      params[name.to_s]&.first
    end

    def json_param(name)
      v = param(name)
      v && JSON.parse(v)
    end

    def json_body
      JSON.parse(body)
    end
  end

  attr_reader :requests, :port

  def initialize
    @server = TCPServer.new("127.0.0.1", 0)
    @port = @server.addr[1]
    @requests = Queue.new
    @responses = Queue.new
    @thread = Thread.new { loop { handle(@server.accept) } rescue nil }
  end

  def url
    "http://127.0.0.1:#{@port}"
  end

  def respond(status, body = "", headers = {})
    body = JSON.generate(body) unless body.is_a?(String)
    @responses << [status, body, headers]
    self
  end

  def last_request
    @requests.pop(timeout: 5) or raise "no request received"
  end

  def stop
    @thread.kill
    @server.close
  end

  private

  def handle(sock)
    line = sock.gets or return sock.close
    method, target, _ = line.split(" ", 3)
    headers = {}
    while (h = sock.gets) && h != "\r\n"
      k, v = h.split(":", 2)
      headers[k.strip.downcase] = v.strip
    end
    body = headers["content-length"] ? sock.read(headers["content-length"].to_i) : nil
    path, query = target.split("?", 2)
    @requests << Request.new(method: method, target: target, path: path, query: query,
                             headers: headers, body: body)
    status, rbody, rheaders = @responses.empty? ? [200, '{"result":1}', {}] : @responses.pop
    reason = { 200 => "OK", 201 => "Created", 204 => "No Content" }[status] || "Status"
    out = +"HTTP/1.1 #{status} #{reason}\r\nConnection: close\r\n"
    out << "Content-Type: application/json\r\n" unless status == 204
    rheaders.each { |k, v| out << "#{k}: #{v}\r\n" }
    out << "Content-Length: #{rbody.bytesize}\r\n" unless status == 204
    out << "\r\n"
    out << rbody unless status == 204
    sock.write(out)
  ensure
    sock&.close
  end
end

class MockTest < Minitest::Test
  PROJECT = "proj123"
  WRITE = "writekey"
  READ = "readkey"
  MASTER = "masterkey"

  def setup
    @server = MockServer.new
    @client = Incisive::Client.new(project_id: PROJECT, write_key: WRITE, read_key: READ,
                                   master_key: MASTER, api_url: @server.url)
  end

  def teardown
    @server.stop
  end

  def assert_headers(req, key, content_type: true)
    assert_equal key, req.headers["authorization"]
    assert_equal "application/json", req.headers["content-type"] if content_type
    assert_equal "incisive-ruby/#{Incisive::VERSION}", req.headers["user-agent"]
    allowed = %w[authorization content-type user-agent accept host content-length connection]
    assert_empty req.headers.keys - allowed, "unexpected headers"
  end
end
