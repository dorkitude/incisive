require 'json'

module Incisive
  class Client
    module QueryingMethods
      # Each analysis takes (event_collection, params, options):
      #   params  - target_property, group_by, timeframe, interval, filters,
      #             timezone, percentile, latest, property_names, max_age, ...
      #   options - :method => :post   send params as a JSON body instead of GET
      #             :response => :all_keys  return the full JSON, not just "result"
      #             :headers => {...}  extra request headers
      def count(event_collection, params = {}, options = {})
        query(:count, event_collection, params, options)
      end

      %i[count_unique minimum maximum sum average median percentile
         select_unique multi_analysis standard_deviation].each do |analysis|
        define_method(analysis) do |event_collection, params, options = {}|
          query(analysis, event_collection, params, options)
        end
      end

      def extraction(event_collection, params = {}, options = {})
        query(:extraction, event_collection, params, options)
      end

      # Funnels carry their collections in `steps`, so there is no collection argument.
      def funnel(params, options = {})
        query(:funnel, nil, params, options)
      end

      # Absolute GET URL for a query; appends api_key=<read key> unless
      # options[:exclude_api_key].
      def query_url(analysis_type, event_collection, params = {}, options = {})
        url = build_query_url(analysis_type, event_collection, params)
        url += "&api_key=#{read_key}" unless options[:exclude_api_key]
        url
      end

      def query(analysis_type, event_collection, params = {}, options = {})
        ensure_project_id!
        ensure_read_key!
        path = api_query_resource_path(analysis_type)
        query_params = params.dup
        query_params[:event_collection] = event_collection.to_s if event_collection

        response =
          if options[:method] == :post
            log_query("#{api_url}#{path}", 'POST', query_params) if log_queries
            http_request(:post, path, read_key, body: JSON.generate(query_params),
                         headers: options[:headers], label: analysis_type)
          else
            path = "#{path}?#{preprocess_params(query_params)}"
            log_query("#{api_url}#{path}") if log_queries
            http_request(:get, path, read_key, headers: options[:headers], label: analysis_type)
          end

        result = process_response(response.code, response.body.to_s.chomp)
        return result if options[:response] == :all_keys
        result.is_a?(Hash) ? result["result"] : result
      end

      private

      def build_query_url(analysis_type, event_collection, params)
        ensure_project_id!
        ensure_read_key!
        query_params = params.dup
        query_params[:event_collection] = event_collection.to_s if event_collection
        "#{api_url}#{api_query_resource_path(analysis_type)}?#{preprocess_params(query_params)}"
      end

      def api_query_resource_path(analysis_type)
        "#{project_path}/queries/#{analysis_type}"
      end

      def log_query(url, method = 'GET', params = {})
        Incisive.logger.info { "[INCISIVE] Send #{method} query to #{url} with options #{params}" }
      end
    end
  end
end
