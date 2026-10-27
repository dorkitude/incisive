require "test_helper"

class MaintenanceTest < MockTest
  def test_delete_collection
    @server.respond(204)
    assert_equal true, @client.delete(:foodstuffs)
    req = @server.last_request
    assert_equal "DELETE", req.method
    assert_equal "/3.0/projects/#{PROJECT}/events/foodstuffs", req.path
    assert_nil req.query
    assert_headers req, MASTER
  end

  def test_delete_with_filters
    @server.respond(204)
    filters = [{ property_name: "delete", operator: "eq", property_value: "me" }]
    @client.delete("food stuffs", filters: filters, timeframe: "this_7_days", timezone: nil)
    req = @server.last_request
    assert_equal "/3.0/projects/#{PROJECT}/events/food%20stuffs", req.path
    assert_equal [{ "property_name" => "delete", "operator" => "eq", "property_value" => "me" }],
                 req.json_param(:filters)
    assert_equal "this_7_days", req.param(:timeframe)
    refute req.params.key?("timezone")
  end

  def test_requires_master_key
    c = Incisive::Client.new(project_id: PROJECT, read_key: READ, api_url: @server.url)
    e = assert_raises(Incisive::ConfigurationError) { c.delete("x") }
    assert_equal "Incisive Exception: Master Key must be set for this operation", e.message
  end

  def test_event_collections_and_single
    resp = [{ "name" => "c", "properties" => { "a" => "num" }, "url" => "/3.0/projects/p/events/c" }]
    @server.respond(200, resp)
    assert_equal resp, @client.event_collections
    req = @server.last_request
    assert_equal "GET", req.method
    assert_equal "/3.0/projects/#{PROJECT}/events", req.path
    assert_headers req, MASTER

    @server.respond(200, resp.first)
    assert_equal resp.first, @client.event_collection("my coll")
    assert_equal "/3.0/projects/#{PROJECT}/events/my%20coll", @server.last_request.path
  end

  def test_project_info
    @server.respond(200, { "id" => PROJECT })
    assert_equal({ "id" => PROJECT }, @client.project_info)
    req = @server.last_request
    assert_equal "/3.0/projects/#{PROJECT}", req.path
    assert_headers req, MASTER
  end
end
