require "test_helper"

class ModuleTest < Minitest::Test
  ENV_KEYS = %w[INCISIVE_PROJECT_ID INCISIVE_WRITE_KEY INCISIVE_READ_KEY INCISIVE_MASTER_KEY
                INCISIVE_BASE_URL INCISIVE_READ_TIMEOUT].freeze

  def setup
    @saved = ENV_KEYS.to_h { |k| [k, ENV[k]] }
    @server = MockServer.new
    Incisive.default_client = nil
  end

  def teardown
    @saved.each { |k, v| ENV[k] = v }
    Incisive.default_client = nil
    @server.stop
  end

  def test_defaults
    c = Incisive::Client.new({})
    assert_equal "http://localhost:8080", c.api_url
    assert_equal "3.0", c.api_version
  end

  def test_from_env_and_module_delegation
    ENV["INCISIVE_PROJECT_ID"] = "envproj"
    ENV["INCISIVE_WRITE_KEY"] = "w"
    ENV["INCISIVE_READ_KEY"] = "r"
    ENV["INCISIVE_MASTER_KEY"] = "m"
    ENV["INCISIVE_BASE_URL"] = @server.url
    ENV["INCISIVE_READ_TIMEOUT"] = "12"
    assert_equal "envproj", Incisive.project_id
    assert_equal @server.url, Incisive.base_url
    assert_equal 12.0, Incisive.read_timeout

    @server.respond(201, { "created" => true })
    Incisive.publish("c", { a: 1 })
    req = @server.last_request
    assert_equal "/3.0/projects/envproj/events/c", req.path
    assert_equal "w", req.headers["authorization"]

    @server.respond(200, { "result" => 5 })
    assert_equal 5, Incisive.count("c")
    assert_equal "r", @server.last_request.headers["authorization"]

    @server.respond(200, [])
    Incisive.cached_datasets.list
    assert_equal "m", @server.last_request.headers["authorization"]

    Incisive.project_id = "other"
    assert_equal "other", Incisive.project_id
  end

  def test_legacy_positional_constructor
    c = Incisive::Client.new("p", "w", "r", master_key: "m")
    assert_equal %w[p w r m], [c.project_id, c.write_key, c.read_key, c.master_key]
  end
end
