require 'json'
require 'cgi'

module Incisive
  class Client
    module PublishingMethods
      # Deprecated alias of #publish (options ignored).
      def add_event(event_collection, properties, _options = {})
        publish(event_collection, properties)
      end

      # POST /{v}/projects/{id}/events/{collection} with the event as the JSON body.
      def publish(event_collection, properties)
        ensure_project_id!
        ensure_write_key!
        check_event_data!(event_collection, properties)
        request_and_process(:post, api_event_collection_resource_path(event_collection),
                            write_key, body: JSON.generate(properties), label: "publish")
      end

      # POST /{v}/projects/{id}/events with {"collection" => [event, ...], ...}.
      def publish_batch(events)
        ensure_project_id!
        ensure_write_key!
        request_and_process(:post, "#{project_path}/events", write_key,
                            body: JSON.generate(events), label: "publish")
      end

      # Background variants: validate synchronously, then perform the request
      # on a Thread. `thread.value` returns the result or re-raises the error.
      def publish_async(event_collection, properties)
        ensure_project_id!
        ensure_write_key!
        check_event_data!(event_collection, properties)
        background { publish(event_collection, properties) }
      end

      def publish_batch_async(events)
        ensure_project_id!
        ensure_write_key!
        background { publish_batch(events) }
      end

      # URL that records an event when fetched (e.g. an email image beacon).
      def beacon_url(event_collection, properties)
        "#{api_url}#{api_event_collection_resource_path(event_collection)}" \
          "?api_key=#{write_key}&data=#{encode_beacon_data(properties)}"
      end

      # Like #beacon_url, then redirects to `redirect_url`.
      def redirect_url(event_collection, properties, redirect_url)
        "#{beacon_url(event_collection, properties)}&redirect=#{CGI.escape(redirect_url)}"
      end

      private

      def background(&block)
        thread = Thread.new(&block)
        thread.report_on_exception = false
        thread
      end

      def encode_beacon_data(properties)
        [JSON.generate(properties)].pack("m0").tr("+/", "-_")
      end

      def check_event_data!(event_collection, properties)
        raise ArgumentError, "Event collection can not be nil" unless event_collection
        raise ArgumentError, "Event properties can not be nil" unless properties
      end
    end
  end
end
