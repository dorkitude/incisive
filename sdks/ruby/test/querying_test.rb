require "test_helper"

class QueryingTest < MockTest
  ANALYSES = %i[count_unique minimum maximum sum average median percentile
                select_unique multi_analysis standard_deviation].freeze

  def test_count_get_unwraps_result
    @server.respond(200, { "result" => 42 })
    assert_equal 42, @client.count("purchases", timeframe: "this_7_days")
    req = @server.last_request
    assert_equal "GET", req.method
    assert_equal "/1.0/projects/#{PROJECT}/queries/count", req.path
    assert_headers req, READ
    assert_nil req.body
    assert_equal({ "timeframe" => ["this_7_days"], "event_collection" => ["purchases"] }, req.params)
  end

  def test_every_analysis_route
    ANALYSES.each do |a|
      @server.respond(200, { "result" => a.to_s })
      assert_equal a.to_s, @client.public_send(a, "c", target_property: "price")
      req = @server.last_request
      assert_equal "/1.0/projects/#{PROJECT}/queries/#{a}", req.path
      assert_equal "price", req.param(:target_property)
      assert_equal "c", req.param(:event_collection)
    end
  end

  def test_query_string_encoding
    filters = [{ property_name: "the+animal", operator: "eq", property_value: "dogs & cats" }]
    @server.respond(200, { "result" => [] })
    @client.extraction("c",
                       filters: filters,
                       timeframe: { start: "2026-01-01T00:00:00Z", end: "2026-02-01T00:00:00Z" },
                       group_by: ["a", "b"],
                       property_names: ["x", "y.z"],
                       interval: "daily",
                       timezone: "US/Pacific",
                       latest: 10,
                       percentile: 99.5,
                       max_age: 300,
                       email: "a@b.com",
                       ignored: nil)
    req = @server.last_request
    assert_equal [{ "property_name" => "the+animal", "operator" => "eq", "property_value" => "dogs & cats" }],
                 req.json_param(:filters)
    assert_equal({ "start" => "2026-01-01T00:00:00Z", "end" => "2026-02-01T00:00:00Z" }, req.json_param(:timeframe))
    assert_equal %w[a b], req.json_param(:group_by)
    assert_equal %w[x y.z], req.json_param(:property_names)
    assert_equal "daily", req.param(:interval)
    assert_equal "US/Pacific", req.param(:timezone)
    assert_equal "10", req.param(:latest)
    assert_equal "99.5", req.param(:percentile)
    assert_equal "300", req.param(:max_age)
    assert_equal "a@b.com", req.param(:email)
    refute req.params.key?("ignored")
    # form-style escaping: spaces as '+', JSON fully escaped
    assert_includes req.query, "timezone=US%2FPacific"
    assert_includes req.query, "filters=%5B%7B%22property_name%22%3A%22the%2Banimal%22"
  end

  def test_single_group_by_string_and_non_numeric_max_age
    @server.respond(200, { "result" => 1 })
    @client.count("c", group_by: "one foo", max_age: "soon")
    req = @server.last_request
    assert_includes req.query, "group_by=one+foo"
    refute req.params.key?("max_age")
  end

  def test_funnel_and_multi_analysis_encode_json
    steps = [{ event_collection: "a", actor_property: "u" }, { event_collection: "b", actor_property: "u" }]
    @server.respond(200, { "result" => [10, 5] })
    assert_equal [10, 5], @client.funnel(steps: steps)
    req = @server.last_request
    assert_equal "/1.0/projects/#{PROJECT}/queries/funnel", req.path
    refute req.params.key?("event_collection")
    assert_equal JSON.parse(JSON.generate(steps)), req.json_param(:steps)

    analyses = { "total" => { analysis_type: "sum", target_property: "p" } }
    @server.respond(200, { "result" => { "total" => 3 } })
    assert_equal({ "total" => 3 }, @client.multi_analysis("c", analyses: analyses))
    assert_equal({ "total" => { "analysis_type" => "sum", "target_property" => "p" } },
                 @server.last_request.json_param(:analyses))
  end

  def test_post_method_sends_json_body
    @server.respond(200, { "result" => 7 })
    params = { timeframe: "this_1_days", filters: [{ property_name: "a", operator: "exists", property_value: true }] }
    assert_equal 7, @client.count("c", params, method: :post)
    req = @server.last_request
    assert_equal "POST", req.method
    assert_nil req.query
    assert_headers req, READ
    assert_equal({ "timeframe" => "this_1_days", "event_collection" => "c",
                   "filters" => [{ "property_name" => "a", "operator" => "exists", "property_value" => true }] },
                 req.json_body)
  end

  def test_all_keys_and_extra_headers
    resp = { "result" => 3, "query" => { "analysis_type" => "count" } }
    @server.respond(200, resp)
    assert_equal resp, @client.count("c", {}, response: :all_keys, headers: { "X-Extra" => "1" })
    assert_equal "1", @server.last_request.headers["x-extra"]
  end

  def test_query_generic_and_url
    @server.respond(200, { "result" => 2 })
    assert_equal 2, @client.query(:count, "c", { timeframe: "this_2_days" })
    url = @client.query_url(:count, "c", { timeframe: "this_2_days" })
    assert_equal "#{@server.url}/1.0/projects/#{PROJECT}/queries/count?timeframe=this_2_days&event_collection=c&api_key=#{READ}", url
    assert_equal "#{@server.url}/1.0/projects/#{PROJECT}/queries/count?event_collection=c",
                 @client.query_url(:count, "c", {}, exclude_api_key: true)
  end

  def test_requires_read_key
    c = Incisive::Client.new(project_id: PROJECT, api_url: @server.url)
    e = assert_raises(Incisive::ConfigurationError) { c.count("c") }
    assert_equal "Incisive Exception: Read Key must be set for this operation", e.message
  end

  def test_error_statuses
    { 400 => Incisive::BadRequestError, 401 => Incisive::AuthenticationError,
      404 => Incisive::NotFoundError, 403 => Incisive::HttpError, 500 => Incisive::HttpError }.each do |status, klass|
      body = { "message" => "boom #{status}", "error_code" => "Code#{status}" }
      @server.respond(status, body)
      err = assert_raises(klass) { @client.count("c") }
      assert_instance_of klass, err
      assert_equal status, err.status
      assert_equal "Code#{status}", err.error_code
      assert_equal "boom #{status}", err.api_message
      assert_equal JSON.generate(body), err.body
      assert_equal "Incisive Exception: #{JSON.generate(body)}", err.message
      assert_kind_of Incisive::HttpError, err
      assert_kind_of Incisive::Error, err
    end
  end

  def test_non_json_error_body
    @server.respond(502, "Bad Gateway")
    err = assert_raises(Incisive::HttpError) { @client.count("c") }
    assert_equal 502, err.status
    assert_nil err.error_code
  end

  def test_invalid_json_success_returns_empty_hash
    @server.respond(200, "not json")
    assert_equal({}, @client.count("c", {}, response: :all_keys))
  end

  def test_api_version_and_base_path_prefix
    c = Incisive::Client.new(project_id: PROJECT, read_key: READ, api_url: "#{@server.url}/prefix/", api_version: "3.1")
    @server.respond(200, { "result" => 1 })
    c.count("c")
    assert_equal "/prefix/3.1/projects/#{PROJECT}/queries/count", @server.last_request.path
  end
end
