require "test_helper"


class PublishingTest < MockTest
  def test_publish_posts_json_with_write_key
    @server.respond(201, { "created" => true })
    event = { "name" => "bob", "incisive" => { "timestamp" => "2026-01-01T00:00:00.000Z" } }
    assert_equal({ "created" => true }, @client.publish(:signups, event))
    req = @server.last_request
    assert_equal "POST", req.method
    assert_equal "/1.0/projects/#{PROJECT}/events/signups", req.path
    assert_headers req, WRITE
    assert_equal event, req.json_body
  end

  def test_collection_name_escaping
    @server.respond(201, { "created" => true })
    @client.publish("a b/c?d#e%", { x: 1 })
    assert_equal "/1.0/projects/#{PROJECT}/events/a%20b%2Fc%3Fd%23e%25", @server.last_request.path
  end

  def test_collection_name_keeps_sub_delims
    @server.respond(201, {})
    @client.publish("x:y@z!$&'()*+,;=-._~", {})
    assert_equal "/1.0/projects/#{PROJECT}/events/x:y@z!$&'()*+,;=-._~", @server.last_request.path
  end

  def test_add_event_alias
    @server.respond(201, { "created" => true })
    @client.add_event("c", { a: 1 })
    assert_equal "/1.0/projects/#{PROJECT}/events/c", @server.last_request.path
  end

  def test_publish_batch
    body = { "signups" => [{ "n" => 1 }, { "n" => 2 }], "purchases" => [{ "p" => 3 }] }
    resp = { "signups" => [{ "success" => true }, { "success" => true }], "purchases" => [{ "success" => true }] }
    @server.respond(200, resp)
    assert_equal resp, @client.publish_batch(body)
    req = @server.last_request
    assert_equal "POST", req.method
    assert_equal "/1.0/projects/#{PROJECT}/events", req.path
    assert_headers req, WRITE
    assert_equal body, req.json_body
  end

  def test_publish_async_returns_thread
    @server.respond(201, { "created" => true })
    t = @client.publish_async("c", { a: 1 })
    assert_kind_of Thread, t
    assert_equal({ "created" => true }, t.value)
    assert_equal({ "a" => 1 }, @server.last_request.json_body)
  end

  def test_publish_batch_async_error_propagates
    @server.respond(400, { "message" => "bad", "error_code" => "InvalidBatchError" })
    t = @client.publish_batch_async({ "c" => [{}] })
    err = assert_raises(Incisive::BadRequestError) { t.value }
    assert_equal "InvalidBatchError", err.error_code
  end

  def test_requires_write_key_and_project
    c = Incisive::Client.new(project_id: PROJECT, api_url: @server.url)
    e = assert_raises(Incisive::ConfigurationError) { c.publish("c", {}) }
    assert_equal "Incisive Exception: Write Key must be set for this operation", e.message
    c = Incisive::Client.new(write_key: WRITE)
    e = assert_raises(Incisive::ConfigurationError) { c.publish("c", {}) }
    assert_equal "Incisive Exception: Project ID must be set", e.message
  end

  def test_nil_args
    assert_raises(ArgumentError) { @client.publish(nil, {}) }
    assert_raises(ArgumentError) { @client.publish("c", nil) }
  end

  def test_beacon_and_redirect_url
    props = { "a" => "b?" }
    data = [JSON.generate(props)].pack("m0").tr("+/", "-_")
    expected = "#{@server.url}/1.0/projects/#{PROJECT}/events/sign%20ups?api_key=#{WRITE}&data=#{data}"
    assert_equal expected, @client.beacon_url("sign ups", props)
    assert_equal "#{expected}&redirect=http%3A%2F%2Fx.com%2F%3Fa%3D1",
                 @client.redirect_url("sign ups", props, "http://x.com/?a=1")
  end

  def test_transport_error_wrapped
    @server.stop
    c = Incisive::Client.new(project_id: PROJECT, write_key: WRITE, api_url: @server.url)
    e = assert_raises(Incisive::HttpError) { c.publish("c", {}) }
    assert_match(/HTTP publish failure/, e.message)
    refute_nil e.original_error
    assert_nil e.status
    @server = MockServer.new # for teardown
  end
end
