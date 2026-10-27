require "test_helper"

class AccessKeysTest < MockTest
  BASE = "/1.0/projects/#{PROJECT}/keys"

  def test_all_get_create_update
    @server.respond(200, [{ "key" => "k1" }])
    assert_equal [{ "key" => "k1" }], @client.access_keys.all
    req = @server.last_request
    assert_equal ["GET", BASE], [req.method, req.path]
    assert_headers req, MASTER

    @server.respond(200, { "key" => "k1" })
    @client.access_keys.get("k1")
    req = @server.last_request
    assert_equal ["GET", "#{BASE}/k1"], [req.method, req.path]

    body = { name: "reader", is_active: true, permitted: ["queries"], options: { queries: { filters: [] } } }
    @server.respond(201, { "key" => "new" })
    assert_equal({ "key" => "new" }, @client.access_keys.create(body))
    req = @server.last_request
    assert_equal ["POST", BASE], [req.method, req.path]
    assert_equal JSON.parse(JSON.generate(body)), req.json_body

    @server.respond(200, { "key" => "k1" })
    @client.access_keys.update("k1", { name: "x" })
    req = @server.last_request
    assert_equal ["POST", "#{BASE}/k1"], [req.method, req.path]
    assert_equal({ "name" => "x" }, req.json_body)
  end

  def test_revoke_unrevoke_delete
    @server.respond(200, { "is_active" => false })
    @client.access_keys.revoke("k1")
    req = @server.last_request
    assert_equal ["POST", "#{BASE}/k1/revoke", '""'], [req.method, req.path, req.body]

    @server.respond(200, { "is_active" => true })
    @client.access_keys.unrevoke("k1")
    assert_equal "#{BASE}/k1/unrevoke", @server.last_request.path

    @server.respond(204)
    assert_equal true, @client.access_keys.delete("k1")
    req = @server.last_request
    assert_equal ["DELETE", "#{BASE}/k1"], [req.method, req.path]
    assert_headers req, MASTER
  end
end

class SavedQueriesTest < MockTest
  BASE = "/1.0/projects/#{PROJECT}/queries/saved"

  def test_all_and_get
    @server.respond(200, [{ "query_name" => "q" }])
    @client.saved_queries.all
    req = @server.last_request
    assert_equal ["GET", BASE], [req.method, req.path]
    assert_headers req, MASTER

    @server.respond(200, { "query_name" => "q" })
    @client.saved_queries.get("q")
    req = @server.last_request
    assert_equal "#{BASE}/q", req.path
    assert_headers req, MASTER

    @server.respond(200, { "result" => 9 })
    assert_equal({ "result" => 9 }, @client.saved_queries.get("q", true))
    req = @server.last_request
    assert_equal "#{BASE}/q/result", req.path
    assert_headers req, READ
  end

  def test_create_drops_nils_and_puts
    @server.respond(201, { "query_name" => "q" })
    @client.saved_queries.create("q", { refresh_rate: nil, metadata: { display_name: "Q", visualization: nil },
                                        query: { analysis_type: "count", event_collection: "c" } })
    req = @server.last_request
    assert_equal ["PUT", "#{BASE}/q"], [req.method, req.path]
    assert_headers req, MASTER
    assert_equal({ "metadata" => { "display_name" => "Q" },
                   "query" => { "analysis_type" => "count", "event_collection" => "c" } }, req.json_body)
  end

  def test_update_cache_uncache
    current = { "query_name" => "q", "refresh_rate" => 0, "query" => { "analysis_type" => "count" },
                "created_at" => "x", "run_information" => nil }
    @server.respond(200, current).respond(200, {})
    @client.saved_queries.cache("q", 3600)
    get = @server.last_request
    put = @server.last_request
    assert_equal ["GET", "#{BASE}/q"], [get.method, get.path]
    assert_equal ["PUT", "#{BASE}/q"], [put.method, put.path]
    assert_equal({ "query_name" => "q", "refresh_rate" => 3600, "query" => { "analysis_type" => "count" } },
                 put.json_body)

    @server.respond(200, current).respond(200, {})
    @client.saved_queries.uncache("q")
    @server.last_request
    assert_equal 0, @server.last_request.json_body["refresh_rate"]
  end

  def test_delete
    @server.respond(204)
    assert_equal true, @client.saved_queries.delete("q")
    req = @server.last_request
    assert_equal ["DELETE", "#{BASE}/q"], [req.method, req.path]
  end
end

class CachedDatasetsTest < MockTest
  BASE = "/1.0/projects/#{PROJECT}/datasets"

  def test_list
    @server.respond(200, { "datasets" => [], "count" => 0 })
    @client.cached_datasets.list
    req = @server.last_request
    assert_equal ["GET", BASE, nil], [req.method, req.path, req.query]
    assert_headers req, MASTER

    @server.respond(200, { "datasets" => [] })
    @client.cached_datasets.list(limit: 5, after_name: "some-dataset")
    assert_equal({ "limit" => ["5"], "after_name" => ["some-dataset"] }, @server.last_request.params)
  end

  def test_definition_results_create_delete
    @server.respond(200, { "dataset_name" => "d" })
    @client.cached_datasets.get_definition("d")
    assert_equal "#{BASE}/d", @server.last_request.path

    tf = { start: "2026-01-01T00:00:00.000Z", end: "2026-01-02T00:00:00.000Z" }
    @server.respond(200, { "result" => [] })
    assert_equal({ "result" => [] }, @client.cached_datasets.get_results("d", tf, "user.id"))
    req = @server.last_request
    assert_equal "#{BASE}/d/results", req.path
    assert_headers req, READ
    assert_equal JSON.parse(JSON.generate(tf)), req.json_param(:timeframe)
    assert_equal "user.id", req.param(:index_by)

    @server.respond(200, { "result" => [] })
    @client.cached_datasets.get_results("d", "this_2_days", %w[a b], "accesskey")
    req = @server.last_request
    assert_headers req, "accesskey"
    assert_equal({ "timeframe" => ["this_2_days"], "index_by" => %w[a b] }, req.params)

    query = { project_id: PROJECT, analysis_type: "count", event_collection: "c",
              timeframe: "this_500_days", interval: "daily", group_by: nil }
    @server.respond(201, { "dataset_name" => "d" })
    @client.cached_datasets.create("d", "user.id", query, "My Dataset")
    req = @server.last_request
    assert_equal ["PUT", "#{BASE}/d"], [req.method, req.path]
    assert_headers req, MASTER
    assert_equal({ "query" => { "project_id" => PROJECT, "analysis_type" => "count", "event_collection" => "c",
                                "timeframe" => "this_500_days", "interval" => "daily" },
                   "index_by" => "user.id", "display_name" => "My Dataset" }, req.json_body)

    @server.respond(204)
    assert_equal true, @client.cached_datasets.delete("d")
    req = @server.last_request
    assert_equal ["DELETE", "#{BASE}/d"], [req.method, req.path]
  end
end
