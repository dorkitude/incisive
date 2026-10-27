require 'json'

module Incisive
  # Access key management under /{v}/projects/{id}/keys. Master key.
  class AccessKeys
    def initialize(client)
      @client = client
    end

    def all
      call(:get, "")
    end

    def get(key)
      call(:get, "/#{seg(key)}")
    end

    # key_body: {"name" => ..., "is_active" => true, "permitted" => [...], "options" => {...}}
    def create(key_body)
      call(:post, "", key_body)
    end

    def update(key, key_body)
      call(:post, "/#{seg(key)}", key_body)
    end

    def revoke(key)
      call(:post, "/#{seg(key)}/revoke", "")
    end

    def unrevoke(key)
      call(:post, "/#{seg(key)}/unrevoke", "")
    end

    def delete(key)
      call(:delete, "/#{seg(key)}")
    end

    def access_keys_base_url
      client.ensure_project_id!
      "#{client.project_path}/keys"
    end

    private

    attr_reader :client

    def seg(value)
      Client.escape_segment(value)
    end

    def call(method, path, body = nil)
      client.ensure_master_key!
      base = access_keys_base_url
      client.request_and_process(method, base + path, client.master_key,
                                 body: body.nil? ? nil : JSON.generate(body),
                                 label: "access keys")
    end
  end
end
