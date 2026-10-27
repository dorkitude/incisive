using System;
using System.Collections.Generic;
using System.Linq;
using System.Net.Http;
using System.Threading.Tasks;
using Newtonsoft.Json.Linq;
using static Incisive.IncisiveUtil;

namespace Incisive
{
    /// <summary>
    /// Entry point: records events, runs analyses, manages saved queries and cached datasets.
    /// Every operation has an async form (<c>...Async</c>) and a blocking form.
    /// </summary>
    public class IncisiveClient
    {
        private readonly IProjectSettings _prjSettings;
        private readonly IDictionary<string, object> _globalProperties = new Dictionary<string, object>();

        /// <summary>Single-collection operations (replaceable, e.g. for mocking).</summary>
        public IEventCollection EventCollection { get; set; }
        /// <summary>Project-level event operations (replaceable).</summary>
        public IEvent Event { get; set; }
        /// <summary>Event cache; when set, AddEvent(s) queue instead of sending.</summary>
        public IEventCache EventCache { get; private set; }
        /// <summary>Analysis operations (replaceable).</summary>
        public IQueries Queries { get; set; }
        /// <summary>Cached dataset operations (replaceable).</summary>
        public IDataset Datasets { get; set; }

        /// <summary>The settings this client was built with.</summary>
        public IProjectSettings Settings => _prjSettings;

        public IncisiveClient(IProjectSettings prjSettings)
            : this(prjSettings, null, null) { }

        public IncisiveClient(IProjectSettings prjSettings, IEventCache eventCache)
            : this(prjSettings, eventCache, null) { }

        /// <param name="prjSettings">Project settings.</param>
        /// <param name="httpHandler">Custom HTTP handler (proxies, logging, tests).</param>
        public IncisiveClient(IProjectSettings prjSettings, HttpMessageHandler httpHandler)
            : this(prjSettings, null, httpHandler) { }

        public IncisiveClient(IProjectSettings prjSettings, IEventCache eventCache, HttpMessageHandler httpHandler)
        {
            if (null == prjSettings)
                throw new IncisiveException("An IProjectSettings instance is required.");
            if (string.IsNullOrWhiteSpace(prjSettings.ProjectId))
                throw new IncisiveException("A Project ID is required.");
            if (string.IsNullOrWhiteSpace(prjSettings.MasterKey) &&
                string.IsNullOrWhiteSpace(prjSettings.WriteKey) &&
                string.IsNullOrWhiteSpace(prjSettings.ReadKey))
                throw new IncisiveException("An API key is required.");
            if (string.IsNullOrWhiteSpace(prjSettings.ServerUrl))
                throw new IncisiveException("A URL for the server address is required.");

            _prjSettings = prjSettings;
            EventCache = eventCache;

            var transport = new Transport(prjSettings, httpHandler);
            EventCollection = new EventCollection(prjSettings, transport);
            Event = new Event(prjSettings, transport);
            Queries = new Queries(prjSettings, transport);
            Datasets = new Datasets(prjSettings, transport);
        }

        // ---- global properties ---------------------------------------------------------

        /// <summary>
        /// Add a property to every event this client sends. The value may be an
        /// <see cref="IDynamicPropertyValue"/>, evaluated for each event.
        /// </summary>
        public void AddGlobalProperty(string property, object value)
        {
            ValidatePropertyName(property);
            if (value == null)
                throw new IncisiveException("Global properties must have a non-null value.");
            if (value is IDynamicPropertyValue dyn)
                ExecDynamicPropertyValue(property, dyn);
            _globalProperties.Add(property, value);
        }

        private static object ExecDynamicPropertyValue(string propName, IDynamicPropertyValue dynProp)
        {
            object result;
            try
            {
                result = dynProp.Value();
            }
            catch (Exception e)
            {
                throw new IncisiveException($"Dynamic property \"{propName}\" execution failure", e);
            }
            if (result == null)
                throw new IncisiveException($"Dynamic property \"{propName}\" execution returned null");
            return result;
        }

        private JObject PrepareUserObject(object eventInfo, IEnumerable<AddOn> addOns)
        {
            var jEvent = eventInfo as JObject != null ? (JObject)((JObject)eventInfo).DeepClone() : JObject.FromObject(eventInfo);

            foreach (var p in _globalProperties)
            {
                var val = p.Value is IDynamicPropertyValue dyn ? ExecDynamicPropertyValue(p.Key, dyn) : p.Value;
                jEvent.Add(p.Key, JToken.FromObject(val));
            }

            var ns = IncisiveConstants.ReservedNamespace;
            var nsProp = jEvent.Property(ns);
            if (nsProp == null)
                jEvent.Add(ns, new JObject());
            else if (!(nsProp.Value is JObject))
                throw new IncisiveException($"Value of property \"{ns}\" must be an object, is {nsProp.Value.Type}");

            var meta = (JObject)jEvent[ns];
            if (addOns != null && addOns.Any())
                meta.Add("addons", JArray.FromObject(addOns));
            if (meta.Property("timestamp") == null)
                meta.Add("timestamp", DateTime.UtcNow);
            return jEvent;
        }

        // ---- events ------------------------------------------------------------------

        /// <summary>POST events/{collection}. Queued instead when an event cache is configured.</summary>
        public async Task AddEventAsync(string collection, object eventInfo, IEnumerable<AddOn> addOns = null)
        {
            ValidateEventCollectionName(collection);
            if (null == eventInfo)
                throw new IncisiveException("Event data is required.");
            if (string.IsNullOrWhiteSpace(_prjSettings.WriteKey))
                throw new IncisiveException("Write API key is required for AddEvent");

            var jEvent = PrepareUserObject(eventInfo, addOns);
            if (null != EventCache)
                await EventCache.AddAsync(new CachedEvent(collection, jEvent)).ConfigureAwait(false);
            else
                await EventCollection.AddEvent(collection, jEvent).ConfigureAwait(false);
        }

        public void AddEvent(string collection, object eventInfo, IEnumerable<AddOn> addOns = null) =>
            RunSync(() => AddEventAsync(collection, eventInfo, addOns));

        /// <summary>
        /// POST events with <c>{"collection": [...]}</c>. Throws <see cref="IncisiveBulkException"/>
        /// listing rejected events. Queued instead when an event cache is configured.
        /// </summary>
        public async Task AddEventsAsync(string collection, IEnumerable<object> eventsInfo, IEnumerable<AddOn> addOns = null)
        {
            if (null == eventsInfo)
                throw new IncisiveException("AddEvents eventsInfo may not be null");
            ValidateEventCollectionName(collection);
            if (string.IsNullOrWhiteSpace(_prjSettings.WriteKey))
                throw new IncisiveException("Write API key is required for AddEvents");

            var local = new List<JObject>();
            foreach (var e in eventsInfo)
            {
                var jEvent = PrepareUserObject(e, addOns);
                if (null != EventCache)
                    await EventCache.AddAsync(new CachedEvent(collection, jEvent)).ConfigureAwait(false);
                else
                    local.Add(jEvent);
            }

            if (local.Any())
            {
                var errs = (await AddEventsBulkAsync(collection, local).ConfigureAwait(false)).ToList();
                if (errs.Any())
                    throw new IncisiveBulkException("One or more events was rejected during the bulk add operation", errs);
            }
        }

        public void AddEvents(string collection, IEnumerable<object> eventsInfo, IEnumerable<AddOn> addOns = null) =>
            RunSync(() => AddEventsAsync(collection, eventsInfo, addOns));

        private async Task<IEnumerable<CachedEvent>> AddEventsBulkAsync(string collection, IEnumerable<JObject> events)
        {
            var list = events.ToList();
            if (!list.Any())
                return new List<CachedEvent>();
            var body = new JObject { { collection, new JArray(list) } };
            return await Event.AddEvents(body).ConfigureAwait(false);
        }

        /// <summary>Send every cached event, grouped by collection in batches of 1000.</summary>
        public async Task SendCachedEventsAsync()
        {
            if (null == EventCache)
                throw new IncisiveException("Event caching is not enabled");

            var batches = new Dictionary<string, List<CachedEvent>>();
            var failed = new List<CachedEvent>();
            CachedEvent e;
            while (null != (e = await EventCache.TryTakeAsync().ConfigureAwait(false)))
            {
                if (!batches.TryGetValue(e.Collection, out var batch))
                    batches[e.Collection] = batch = new List<CachedEvent>();
                batch.Add(e);
                if (batch.Count == IncisiveConstants.BulkBatchSize)
                {
                    failed.AddRange(await AddEventsBulkAsync(e.Collection, batch.Select(n => n.Event)).ConfigureAwait(false));
                    batch.Clear();
                }
            }

            foreach (var c in batches.Where(b => b.Value.Any()))
                failed.AddRange(await AddEventsBulkAsync(c.Key, c.Value.Select(n => n.Event)).ConfigureAwait(false));

            if (failed.Any())
                throw new IncisiveBulkException("One or more cached events could not be submitted", failed);
        }

        public void SendCachedEvents() => RunSync(SendCachedEventsAsync);

        // ---- collections / schema ----------------------------------------------------

        /// <summary>DELETE events/{collection} (master key).</summary>
        public Task DeleteCollectionAsync(string collection)
        {
            ValidateEventCollectionName(collection);
            if (string.IsNullOrWhiteSpace(_prjSettings.MasterKey))
                throw new IncisiveException("Master API key is required for DeleteCollection");
            return EventCollection.DeleteCollection(collection);
        }

        public void DeleteCollection(string collection) => RunSync(() => DeleteCollectionAsync(collection));

        /// <summary>GET events: every collection's schema (read key).</summary>
        public Task<JArray> GetSchemasAsync()
        {
            if (string.IsNullOrWhiteSpace(_prjSettings.ReadKey))
                throw new IncisiveException("Read API key is required for GetSchemas");
            return Event.GetSchemas();
        }

        public JArray GetSchemas() => RunSync(GetSchemasAsync);

        /// <summary>GET events/{collection}: one collection's schema (read key).</summary>
        public async Task<dynamic> GetSchemaAsync(string collection)
        {
            ValidateEventCollectionName(collection);
            if (string.IsNullOrWhiteSpace(_prjSettings.ReadKey))
                throw new IncisiveException("Read API key is required for GetSchema");
            return await EventCollection.GetSchema(collection).ConfigureAwait(false);
        }

        public dynamic GetSchema(string collection) => RunSync<dynamic>(() => GetSchemaAsync(collection));

        // ---- queries -----------------------------------------------------------------

        /// <summary>GET queries: available query resources (name to URL).</summary>
        public Task<IEnumerable<KeyValuePair<string, string>>> GetQueriesAsync() => Queries.AvailableQueries();
        public IEnumerable<KeyValuePair<string, string>> GetQueries() => RunSync(GetQueriesAsync);

        /// <summary>Run any analysis by name with raw query-string parameters; returns the full response.</summary>
        public Task<JObject> QueryAsync(string queryName, Dictionary<string, string> parms) => Queries.Metric(queryName, parms);
        public JObject Query(string queryName, Dictionary<string, string> parms) => RunSync(() => QueryAsync(queryName, parms));

        /// <summary>Single-value analysis; the <c>result</c> as a string (select_unique joined with ",").</summary>
        public Task<string> QueryAsync(QueryType queryType, string collection, string targetProperty,
            IQueryTimeframe timeframe = null, IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            Queries.Metric(queryType, collection, targetProperty, timeframe, filters, timezone);

        public string Query(QueryType queryType, string collection, string targetProperty,
            IQueryTimeframe timeframe = null, IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            RunSync(() => QueryAsync(queryType, collection, targetProperty, timeframe, filters, timezone));

        public Task<IEnumerable<QueryGroupValue<string>>> QueryGroupAsync(QueryType queryType, string collection,
            string targetProperty, string groupBy, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            Queries.Metric(queryType, collection, targetProperty, groupBy, timeframe, filters, timezone);

        public IEnumerable<QueryGroupValue<string>> QueryGroup(QueryType queryType, string collection,
            string targetProperty, string groupBy, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            RunSync(() => QueryGroupAsync(queryType, collection, targetProperty, groupBy, timeframe, filters, timezone));

        public Task<IEnumerable<QueryIntervalValue<string>>> QueryIntervalAsync(QueryType queryType,
            string collection, string targetProperty, IQueryTimeframe timeframe, QueryInterval interval = null,
            IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            Queries.Metric(queryType, collection, targetProperty, timeframe, interval, filters, timezone);

        public IEnumerable<QueryIntervalValue<string>> QueryInterval(QueryType queryType, string collection,
            string targetProperty, IQueryTimeframe timeframe, QueryInterval interval = null,
            IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            RunSync(() => QueryIntervalAsync(queryType, collection, targetProperty, timeframe, interval, filters, timezone));

        public Task<IEnumerable<QueryIntervalValue<IEnumerable<QueryGroupValue<string>>>>> QueryIntervalGroupAsync(
            QueryType queryType, string collection, string targetProperty, string groupBy, IQueryTimeframe timeframe,
            QueryInterval interval, IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            Queries.Metric(queryType, collection, targetProperty, groupBy, timeframe, interval, filters, timezone);

        public IEnumerable<QueryIntervalValue<IEnumerable<QueryGroupValue<string>>>> QueryIntervalGroup(
            QueryType queryType, string collection, string targetProperty, string groupBy, IQueryTimeframe timeframe,
            QueryInterval interval, IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            RunSync(() => QueryIntervalGroupAsync(queryType, collection, targetProperty, groupBy, timeframe, interval, filters, timezone));

        /// <summary>Extraction: raw events (or, with <paramref name="email"/>, an emailed export).</summary>
        public Task<IEnumerable<dynamic>> QueryExtractResourceAsync(string collection, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, int latest = 0, string email = "") =>
            Queries.Extract(collection, timeframe, filters, latest, email);

        public IEnumerable<dynamic> QueryExtractResource(string collection, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, int latest = 0, string email = "") =>
            RunSync(() => QueryExtractResourceAsync(collection, timeframe, filters, latest, email));

        public Task<FunnelResult> QueryFunnelAsync(IEnumerable<FunnelStep> steps, IQueryTimeframe timeframe = null, string timezone = "") =>
            Queries.Funnel(steps, timeframe, timezone);

        public FunnelResult QueryFunnel(IEnumerable<FunnelStep> steps, IQueryTimeframe timeframe = null, string timezone = "") =>
            RunSync(() => QueryFunnelAsync(steps, timeframe, timezone));

        public Task<IDictionary<string, string>> QueryMultiAnalysisAsync(string collection,
            IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            Queries.MultiAnalysis(collection, analysisParams, timeframe, filters, timezone);

        public IDictionary<string, string> QueryMultiAnalysis(string collection,
            IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            RunSync(() => QueryMultiAnalysisAsync(collection, analysisParams, timeframe, filters, timezone));

        public Task<IEnumerable<QueryGroupValue<IDictionary<string, string>>>> QueryMultiAnalysisGroupAsync(
            string collection, IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, string groupBy = "", string timezone = "") =>
            Queries.MultiAnalysis(collection, analysisParams, timeframe, filters, groupBy, timezone);

        public IEnumerable<QueryGroupValue<IDictionary<string, string>>> QueryMultiAnalysisGroup(string collection,
            IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, string groupBy = "", string timezone = "") =>
            RunSync(() => QueryMultiAnalysisGroupAsync(collection, analysisParams, timeframe, filters, groupBy, timezone));

        public Task<IEnumerable<QueryIntervalValue<IDictionary<string, string>>>> QueryMultiAnalysisIntervalAsync(
            string collection, IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null,
            QueryInterval interval = null, IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            Queries.MultiAnalysis(collection, analysisParams, timeframe, interval, filters, timezone);

        public IEnumerable<QueryIntervalValue<IDictionary<string, string>>> QueryMultiAnalysisInterval(
            string collection, IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null,
            QueryInterval interval = null, IEnumerable<QueryFilter> filters = null, string timezone = "") =>
            RunSync(() => QueryMultiAnalysisIntervalAsync(collection, analysisParams, timeframe, interval, filters, timezone));

        public Task<IEnumerable<QueryIntervalValue<IEnumerable<QueryGroupValue<IDictionary<string, string>>>>>>
            QueryMultiAnalysisIntervalGroupAsync(string collection, IEnumerable<MultiAnalysisParam> analysisParams,
                IQueryTimeframe timeframe = null, QueryInterval interval = null, IEnumerable<QueryFilter> filters = null,
                string groupBy = "", string timezone = "") =>
            Queries.MultiAnalysis(collection, analysisParams, timeframe, interval, filters, groupBy, timezone);

        public IEnumerable<QueryIntervalValue<IEnumerable<QueryGroupValue<IDictionary<string, string>>>>>
            QueryMultiAnalysisIntervalGroup(string collection, IEnumerable<MultiAnalysisParam> analysisParams,
                IQueryTimeframe timeframe = null, QueryInterval interval = null, IEnumerable<QueryFilter> filters = null,
                string groupBy = "", string timezone = "") =>
            RunSync(() => QueryMultiAnalysisIntervalGroupAsync(collection, analysisParams, timeframe, interval, filters, groupBy, timezone));

        // ---- saved / cached queries --------------------------------------------------

        /// <summary>GET queries/saved (master key): every saved query definition.</summary>
        public Task<JArray> ListSavedQueriesAsync() => Queries.ListSavedQueries();
        public JArray ListSavedQueries() => RunSync(ListSavedQueriesAsync);

        /// <summary>GET queries/saved/{name} (master key).</summary>
        public Task<JObject> GetSavedQueryAsync(string queryName) => Queries.GetSavedQuery(queryName);
        public JObject GetSavedQuery(string queryName) => RunSync(() => GetSavedQueryAsync(queryName));

        /// <summary>GET queries/saved/{name}/result (read or master key).</summary>
        public Task<JObject> GetSavedQueryResultAsync(string queryName) => Queries.GetSavedQueryResult(queryName);
        public JObject GetSavedQueryResult(string queryName) => RunSync(() => GetSavedQueryResultAsync(queryName));

        /// <summary>
        /// PUT queries/saved/{name} (master key). <paramref name="savedQuery"/> is the full body, e.g.
        /// <c>new { refresh_rate = 0, query = new { analysis_type = "count", event_collection = "x", timeframe = "this_7_days" } }</c>.
        /// </summary>
        public Task<JObject> CreateSavedQueryAsync(string queryName, object savedQuery) =>
            Queries.PutSavedQuery(queryName, ToJObject(savedQuery));
        public JObject CreateSavedQuery(string queryName, object savedQuery) => RunSync(() => CreateSavedQueryAsync(queryName, savedQuery));

        /// <summary>PUT queries/saved/{name} (master key) with an updated body.</summary>
        public Task<JObject> UpdateSavedQueryAsync(string queryName, object savedQuery) =>
            Queries.PutSavedQuery(queryName, ToJObject(savedQuery));
        public JObject UpdateSavedQuery(string queryName, object savedQuery) => RunSync(() => UpdateSavedQueryAsync(queryName, savedQuery));

        /// <summary>
        /// Create a cached query: a saved query with a non-zero <c>refresh_rate</c> (seconds).
        /// <paramref name="query"/> is the analysis definition (analysis_type, event_collection, ...).
        /// </summary>
        public Task<JObject> CreateCachedQueryAsync(string queryName, object query, int refreshRate)
        {
            var body = new JObject { { "refresh_rate", refreshRate }, { "query", ToJObject(query) } };
            return Queries.PutSavedQuery(queryName, body);
        }
        public JObject CreateCachedQuery(string queryName, object query, int refreshRate) =>
            RunSync(() => CreateCachedQueryAsync(queryName, query, refreshRate));

        /// <summary>DELETE queries/saved/{name} (master key).</summary>
        public Task DeleteSavedQueryAsync(string queryName) => Queries.DeleteSavedQuery(queryName);
        public void DeleteSavedQuery(string queryName) => RunSync(() => DeleteSavedQueryAsync(queryName));

        private static JObject ToJObject(object o)
        {
            if (o == null)
                throw new ArgumentNullException(nameof(o));
            return o as JObject ?? JObject.FromObject(o);
        }

        // ---- cached datasets ---------------------------------------------------------

        public Task<JObject> QueryDatasetAsync(string datasetName, string indexBy, string timeframe) =>
            Datasets.GetResultsAsync(datasetName, indexBy, timeframe);
        public JObject QueryDataset(string datasetName, string indexBy, string timeframe) =>
            RunSync(() => QueryDatasetAsync(datasetName, indexBy, timeframe));

        public Task<DatasetDefinition> GetDatasetDefinitionAsync(string datasetName) => Datasets.GetDefinitionAsync(datasetName);
        public DatasetDefinition GetDatasetDefinition(string datasetName) => RunSync(() => GetDatasetDefinitionAsync(datasetName));

        public Task<DatasetDefinitionCollection> ListDatasetDefinitionsAsync(int limit = 10, string afterName = null) =>
            Datasets.ListDefinitionsAsync(limit, afterName);
        public DatasetDefinitionCollection ListDatasetDefinitions(int limit = 10, string afterName = null) =>
            RunSync(() => ListDatasetDefinitionsAsync(limit, afterName));

        public Task<IEnumerable<DatasetDefinition>> ListAllDatasetDefinitionsAsync() => Datasets.ListAllDefinitionsAsync();
        public IEnumerable<DatasetDefinition> ListAllDatasetDefinitions() => RunSync(ListAllDatasetDefinitionsAsync);

        public Task<DatasetDefinition> CreateDatasetAsync(DatasetDefinition dataset) => Datasets.CreateDatasetAsync(dataset);
        public DatasetDefinition CreateDataset(DatasetDefinition dataset) => RunSync(() => CreateDatasetAsync(dataset));

        public Task DeleteDatasetAsync(string datasetName) => Datasets.DeleteDatasetAsync(datasetName);
        public void DeleteDataset(string datasetName) => RunSync(() => DeleteDatasetAsync(datasetName));
    }
}
