using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace Incisive
{
    /// <summary>Project-level event operations.</summary>
    public interface IEvent
    {
        /// <summary>GET projects/{id}/events: schemas of every collection.</summary>
        Task<JArray> GetSchemas();

        /// <summary>
        /// POST projects/{id}/events with <c>{"collection": [events...]}</c>.
        /// Returns the events the server rejected (with errors); empty when all succeeded.
        /// </summary>
        Task<IEnumerable<CachedEvent>> AddEvents(JObject events);
    }

    /// <summary>Single-collection operations.</summary>
    public interface IEventCollection
    {
        Task<JObject> GetSchema(string collection);
        Task DeleteCollection(string collection);
        Task AddEvent(string collection, JObject anEvent);
    }

    internal sealed class Event : IEvent
    {
        private readonly Transport _transport;
        private readonly IProjectSettings _settings;

        internal Event(IProjectSettings settings, Transport transport)
        {
            _settings = settings;
            _transport = transport;
        }

        public async Task<JArray> GetSchemas()
        {
            if (string.IsNullOrWhiteSpace(_settings.ReadKey))
                throw new IncisiveException("An API ReadKey is required to get schemas.");
            var resp = await _transport.GetAsync(_transport.ProjectUrl(IncisiveConstants.EventsResource), _settings.ReadKey)
                .ConfigureAwait(false);
            return IncisiveUtil.RequireArray(IncisiveUtil.CheckResponse(resp, "GetSchemas"), "GetSchemas");
        }

        public async Task<IEnumerable<CachedEvent>> AddEvents(JObject events)
        {
            if (string.IsNullOrWhiteSpace(_settings.WriteKey))
                throw new IncisiveException("An API WriteKey is required to add events.");

            var resp = await _transport.PostAsync(_transport.ProjectUrl(IncisiveConstants.EventsResource),
                    _settings.WriteKey, events.ToString(Formatting.None))
                .ConfigureAwait(false);
            var token = IncisiveUtil.CheckResponse(resp, "AddEvents");
            if (!(token is JObject result))
                throw new IncisiveException("AddEvents failed with empty response from server.");

            var failed = new List<CachedEvent>();
            foreach (var sent in events.Properties())
            {
                if (!(result[sent.Name] is JArray statuses) || !(sent.Value is JArray sentEvents))
                    continue;
                var n = System.Math.Min(statuses.Count, sentEvents.Count);
                for (int i = 0; i < n; i++)
                {
                    var status = statuses[i] as JObject;
                    var ok = status?["success"];
                    if (ok != null && ok.Type == JTokenType.Boolean && (bool)ok)
                        continue;
                    failed.Add(new CachedEvent(sent.Name, sentEvents[i] as JObject, IncisiveUtil.GetBulkApiError(status)));
                }
            }
            return failed;
        }
    }

    internal sealed class EventCollection : IEventCollection
    {
        private readonly Transport _transport;
        private readonly IProjectSettings _settings;

        internal EventCollection(IProjectSettings settings, Transport transport)
        {
            _settings = settings;
            _transport = transport;
        }

        private string CollectionUrl(string collection) =>
            _transport.ProjectUrl(IncisiveConstants.EventsResource + "/" + IncisiveUtil.EscapePath(collection));

        public async Task<JObject> GetSchema(string collection)
        {
            if (string.IsNullOrWhiteSpace(_settings.ReadKey))
                throw new IncisiveException("An API ReadKey is required to get collection schema.");
            var resp = await _transport.GetAsync(CollectionUrl(collection), _settings.ReadKey).ConfigureAwait(false);
            return IncisiveUtil.RequireObject(IncisiveUtil.CheckResponse(resp, "GetSchema"), "GetSchema");
        }

        public async Task DeleteCollection(string collection)
        {
            if (string.IsNullOrWhiteSpace(_settings.MasterKey))
                throw new IncisiveException("An API MasterKey is required to delete a collection.");
            var resp = await _transport.DeleteAsync(CollectionUrl(collection), _settings.MasterKey).ConfigureAwait(false);
            IncisiveUtil.CheckResponse(resp, "DeleteCollection");
        }

        public async Task AddEvent(string collection, JObject anEvent)
        {
            if (string.IsNullOrWhiteSpace(_settings.WriteKey))
                throw new IncisiveException("An API WriteKey is required to add events.");
            var resp = await _transport.PostAsync(CollectionUrl(collection), _settings.WriteKey, anEvent.ToString(Formatting.None))
                .ConfigureAwait(false);
            IncisiveUtil.CheckResponse(resp, "AddEvent");
        }
    }
}
