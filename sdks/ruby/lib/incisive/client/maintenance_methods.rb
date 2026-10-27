module Incisive
  class Client
    module MaintenanceMethods
      # DELETE events in a collection. With params (filters, timeframe,
      # timezone) only matching events are removed; without, the whole
      # collection is dropped. Master key.
      def delete(event_collection, params = {})
        ensure_project_id!
        ensure_master_key!
        path = api_event_collection_resource_path(event_collection)
        qs = params.nil? || params.empty? ? "" : preprocess_params(params)
        path += "?#{qs}" unless qs.empty?
        request_and_process(:delete, path, master_key, label: "delete")
      end

      # GET /{v}/projects/{id}/events - all collections with their schemas.
      def event_collections
        ensure_project_id!
        ensure_master_key!
        request_and_process(:get, "#{project_path}/events", master_key, label: "events")
      end

      # GET /{v}/projects/{id}/events/{collection} - one collection's schema.
      def event_collection(event_collection)
        ensure_project_id!
        ensure_master_key!
        request_and_process(:get, api_event_collection_resource_path(event_collection),
                            master_key, label: "events")
      end

      # GET /{v}/projects/{id}
      def project_info
        ensure_project_id!
        ensure_master_key!
        request_and_process(:get, project_path, master_key, label: "project info")
      end
    end
  end
end
