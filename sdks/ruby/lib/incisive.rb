require 'logger'
require 'forwardable'

require 'incisive/version'
require 'incisive/errors'
require 'incisive/client'
require 'incisive/access_keys'
require 'incisive/saved_queries'
require 'incisive/cached_datasets'
require 'incisive/scoped_key'

# Module-level API: every call is forwarded to a lazily built default client
# configured from INCISIVE_* environment variables. Attributes can be set
# directly too, e.g. `Incisive.project_id = "..."`.
module Incisive
  class << self
    extend Forwardable

    def_delegators :default_client,
                   :project_id, :project_id=,
                   :write_key, :write_key=,
                   :read_key, :read_key=,
                   :master_key, :master_key=,
                   :api_url, :api_url=,
                   :base_url, :base_url=,
                   :api_version, :api_version=,
                   :log_queries, :log_queries=,
                   :proxy_url, :proxy_url=,
                   :proxy_type, :proxy_type=,
                   :read_timeout, :read_timeout=,
                   :open_timeout, :open_timeout=

    def_delegators :default_client,
                   :publish, :publish_async, :publish_batch, :publish_batch_async,
                   :add_event, :beacon_url, :redirect_url

    def_delegators :default_client,
                   :count, :count_unique, :minimum, :maximum, :sum, :average,
                   :select_unique, :funnel, :extraction, :multi_analysis,
                   :median, :percentile, :standard_deviation

    def_delegators :default_client,
                   :delete, :event_collections, :event_collection, :project_info,
                   :query_url, :query, :saved_queries, :cached_datasets, :access_keys

    attr_writer :logger

    def logger
      @logger ||= Logger.new($stdout).tap { |l| l.level = Logger::INFO }
    end

    # Replace the default client (or pass nil to rebuild it from the environment).
    attr_writer :default_client

    def default_client
      @default_client ||= Client.from_env
    end
  end
end
