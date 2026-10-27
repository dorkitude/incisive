require 'json'
require 'uri'

module Incisive
  # Cached datasets under /{v}/projects/{id}/datasets.
  class CachedDatasets
    def initialize(client)
      @client = client
    end

    # GET the list (master key), optionally paged with limit / after_name.
    def list(limit: nil, after_name: nil)
      client.ensure_master_key!
      get("", { limit: limit, after_name: after_name }, client.master_key)
    end

    def get_definition(dataset_name)
      client.ensure_master_key!
      get("/#{seg(dataset_name)}", {}, client.master_key)
    end

    # GET /datasets/{name}/results?timeframe=..&index_by=.. using the read key
    # (or an explicit api_key, e.g. an access key). A Hash timeframe is sent as
    # JSON; an Array index_by repeats the parameter.
    def get_results(dataset_name, timeframe, index_by, api_key = nil)
      client.ensure_read_key! unless api_key
      params = {
        timeframe: timeframe.is_a?(Hash) ? JSON.generate(timeframe) : timeframe,
        index_by: index_by
      }
      get("/#{seg(dataset_name)}/results", params, api_key || client.read_key)
    end

    # PUT /datasets/{name} with {query, index_by, display_name}.
    def create(name, index_by, query, display_name)
      client.ensure_master_key!
      body = {
        'query' => query.reject { |_k, v| v.nil? },
        'index_by' => index_by,
        'display_name' => display_name
      }
      client.request_and_process(:put, "#{base_path}/#{seg(name)}", client.master_key,
                                 body: JSON.generate(body), label: "datasets")
    end

    def delete(dataset_name)
      client.ensure_master_key!
      client.request_and_process(:delete, "#{base_path}/#{seg(dataset_name)}",
                                 client.master_key, label: "datasets")
    end

    private

    attr_reader :client

    def seg(value)
      Client.escape_segment(value)
    end

    def base_path
      client.ensure_project_id!
      "#{client.project_path}/datasets"
    end

    def get(path, params, key)
      qs = URI.encode_www_form(params.reject { |_k, v| v.nil? })
      full = base_path + path
      full += "?#{qs}" unless qs.empty?
      client.request_and_process(:get, full, key, label: "datasets")
    end
  end
end
