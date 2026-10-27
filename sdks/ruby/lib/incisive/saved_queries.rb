require 'json'

module Incisive
  # Saved queries under /{v}/projects/{id}/queries/saved.
  class SavedQueries
    def initialize(client)
      @client = client
    end

    # GET all definitions (master key).
    def all
      client.ensure_master_key!
      call(:get, "", client.master_key)
    end

    # GET one definition (master key), or with results=true its
    # /result (read key).
    def get(saved_query_name, results = false)
      path = "/#{seg(saved_query_name)}"
      if results
        client.ensure_read_key!
        call(:get, "#{path}/result", client.read_key)
      else
        client.ensure_master_key!
        call(:get, path, client.master_key)
      end
    end

    # PUT a full definition, e.g. {query: {...}, refresh_rate: 0, metadata: {...}}.
    # Top-level nil values (and nil values of nested hashes) are dropped.
    def create(saved_query_name, saved_query_body)
      client.ensure_master_key!
      call(:put, "/#{seg(saved_query_name)}", client.master_key,
           JSON.generate(drop_nils(saved_query_body)))
    end
    alias_method :update_full, :create

    # Read-modify-write: fetch the current definition, keep query_name,
    # refresh_rate and query, merge update_body, and PUT the result.
    def update(saved_query_name, update_body)
      current = get(saved_query_name)
      kept = current.select { |k, _| %w[query_name refresh_rate query].include?(k.to_s) }
      update_full(saved_query_name, kept.merge(stringify(update_body)))
    end

    def cache(saved_query_name, cache_rate)
      update(saved_query_name, refresh_rate: cache_rate)
    end

    def uncache(saved_query_name)
      update(saved_query_name, refresh_rate: 0)
    end

    def delete(saved_query_name)
      client.ensure_master_key!
      call(:delete, "/#{seg(saved_query_name)}", client.master_key)
    end

    private

    attr_reader :client

    def seg(value)
      Client.escape_segment(value)
    end

    def base_path
      client.ensure_project_id!
      "#{client.project_path}/queries/saved"
    end

    def call(method, path, key, body = nil)
      client.request_and_process(method, base_path + path, key, body: body, label: "saved queries")
    end

    def stringify(hash)
      hash.each_with_object({}) { |(k, v), h| h[k.to_s] = v }
    end

    def drop_nils(hash)
      hash.each_with_object({}) do |(k, v), out|
        next if v.nil?
        out[k] = v.is_a?(Hash) ? v.reject { |_ik, iv| iv.nil? } : v
      end
    end
  end
end
