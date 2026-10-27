using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace Incisive
{
    /// <summary>
    /// Analysis queries. Every analysis is a GET to <c>projects/{id}/queries/{analysis}</c> with
    /// parameters in the query string (JSON values are compact-serialized then percent-encoded).
    /// Authenticated with the master key when configured, otherwise the read key.
    /// </summary>
    public interface IQueries
    {
        Task<IEnumerable<KeyValuePair<string, string>>> AvailableQueries();

        Task<JObject> Metric(string queryName, Dictionary<string, string> parms);

        Task<string> Metric(QueryType queryType, string collection, string targetProperty, IQueryTimeframe timeframe = null, IEnumerable<QueryFilter> filters = null, string timezone = "");
        Task<IEnumerable<QueryGroupValue<string>>> Metric(QueryType queryType, string collection, string targetProperty, string groupBy, IQueryTimeframe timeframe = null, IEnumerable<QueryFilter> filters = null, string timezone = "");
        Task<IEnumerable<QueryIntervalValue<string>>> Metric(QueryType queryType, string collection, string targetProperty, IQueryTimeframe timeframe, QueryInterval interval, IEnumerable<QueryFilter> filters = null, string timezone = "");
        Task<IEnumerable<QueryIntervalValue<IEnumerable<QueryGroupValue<string>>>>> Metric(QueryType queryType, string collection, string targetProperty, string groupBy, IQueryTimeframe timeframe, QueryInterval interval, IEnumerable<QueryFilter> filters = null, string timezone = "");

        Task<IDictionary<string, string>> MultiAnalysis(string collection, IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null, IEnumerable<QueryFilter> filters = null, string timezone = "");
        Task<IEnumerable<QueryGroupValue<IDictionary<string, string>>>> MultiAnalysis(string collection, IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null, IEnumerable<QueryFilter> filters = null, string groupby = "", string timezone = "");
        Task<IEnumerable<QueryIntervalValue<IDictionary<string, string>>>> MultiAnalysis(string collection, IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null, QueryInterval interval = null, IEnumerable<QueryFilter> filters = null, string timezone = "");
        Task<IEnumerable<QueryIntervalValue<IEnumerable<QueryGroupValue<IDictionary<string, string>>>>>> MultiAnalysis(string collection, IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null, QueryInterval interval = null, IEnumerable<QueryFilter> filters = null, string groupby = "", string timezone = "");

        Task<IEnumerable<dynamic>> Extract(string collection, IQueryTimeframe timeframe = null, IEnumerable<QueryFilter> filters = null, int latest = 0, string email = "");

        Task<FunnelResult> Funnel(IEnumerable<FunnelStep> steps, IQueryTimeframe timeframe = null, string timeZone = "");

        // Saved / cached queries (projects/{id}/queries/saved)
        Task<JArray> ListSavedQueries();
        Task<JObject> GetSavedQuery(string queryName);
        Task<JObject> GetSavedQueryResult(string queryName);
        Task<JObject> PutSavedQuery(string queryName, JObject body);
        Task DeleteSavedQuery(string queryName);
    }

    internal sealed class Queries : IQueries
    {
        private readonly Transport _transport;
        private readonly IProjectSettings _settings;
        private readonly string _key;

        internal Queries(IProjectSettings settings, Transport transport)
        {
            _settings = settings;
            _transport = transport;
            _key = string.IsNullOrWhiteSpace(settings.MasterKey) ? settings.ReadKey : settings.MasterKey;
        }

        private static string FiltersJson(IEnumerable<QueryFilter> filters) =>
            filters == null ? "" : JArray.FromObject(filters).ToString(Formatting.None);

        private async Task<JObject> Request(string operation, IEnumerable<KeyValuePair<string, string>> parms)
        {
            if (string.IsNullOrWhiteSpace(_key))
                throw new IncisiveException("An API ReadKey or MasterKey is required.");

            var qs = IncisiveUtil.BuildQueryString(parms);
            var url = _transport.ProjectUrl(IncisiveConstants.QueriesResource) +
                      (string.IsNullOrWhiteSpace(operation) ? "" : "/" + operation) +
                      (string.IsNullOrEmpty(qs) ? "" : "?" + qs);
            var resp = await _transport.GetAsync(url, _key).ConfigureAwait(false);
            return IncisiveUtil.RequireObject(IncisiveUtil.CheckResponse(resp, "Query"), "Query");
        }

        private static List<KeyValuePair<string, string>> MetricParms(QueryType queryType, string collection,
            string targetProperty, string groupBy, IQueryTimeframe timeframe, QueryInterval interval,
            IEnumerable<QueryFilter> filters, string timezone)
        {
            var p = new List<KeyValuePair<string, string>>
            {
                Kv(IncisiveConstants.QueryParmEventCollection, collection),
                Kv(IncisiveConstants.QueryParmTargetProperty, targetProperty),
            };
            if (groupBy != null) p.Add(Kv(IncisiveConstants.QueryParmGroupBy, groupBy));
            p.Add(Kv(IncisiveConstants.QueryParmTimeframe, timeframe.ToSafeString()));
            if (interval != null) p.Add(Kv(IncisiveConstants.QueryParmInterval, interval.ToString()));
            p.Add(Kv(IncisiveConstants.QueryParmTimezone, timezone));
            p.Add(Kv(IncisiveConstants.QueryParmFilters, FiltersJson(filters)));
            if (queryType.PercentileValue.HasValue)
                p.Add(Kv(IncisiveConstants.QueryParmPercentile, Fmt.Number(queryType.PercentileValue.Value)));
            return p;
        }

        private static KeyValuePair<string, string> Kv(string k, string v) => new KeyValuePair<string, string>(k, v);

        private static void ValidateMetric(QueryType queryType, string collection, string targetProperty)
        {
            if (queryType is null)
                throw new ArgumentNullException(nameof(queryType));
            if (string.IsNullOrWhiteSpace(collection))
                throw new ArgumentNullException(nameof(collection));
            if (string.IsNullOrWhiteSpace(targetProperty) && !queryType.IsCount)
                throw new ArgumentNullException(nameof(targetProperty));
        }

        public async Task<IEnumerable<KeyValuePair<string, string>>> AvailableQueries()
        {
            var reply = await Request("", null).ConfigureAwait(false);
            return reply.Properties()
                .Select(p => new KeyValuePair<string, string>(p.Name, IncisiveUtil.TokenToString(p.Value)))
                .ToList();
        }

        public Task<JObject> Metric(string queryName, Dictionary<string, string> parms)
        {
            if (string.IsNullOrEmpty(queryName))
                throw new ArgumentNullException(nameof(queryName));
            if (null == parms)
                throw new ArgumentNullException(nameof(parms));
            return Request(queryName, parms);
        }

        public async Task<string> Metric(QueryType queryType, string collection, string targetProperty,
            IQueryTimeframe timeframe = null, IEnumerable<QueryFilter> filters = null, string timezone = "")
        {
            ValidateMetric(queryType, collection, targetProperty);
            var reply = await Request(queryType.ToString(),
                MetricParms(queryType, collection, targetProperty, null, timeframe, null, filters, timezone)).ConfigureAwait(false);
            return IncisiveUtil.TokenToString(reply["result"]);
        }

        public async Task<IEnumerable<QueryGroupValue<string>>> Metric(QueryType queryType, string collection,
            string targetProperty, string groupBy, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, string timezone = "")
        {
            ValidateMetric(queryType, collection, targetProperty);
            if (string.IsNullOrWhiteSpace(groupBy))
                throw new ArgumentNullException(nameof(groupBy), "groupBy field name must be specified for a group-by query");
            var reply = await Request(queryType.ToString(),
                MetricParms(queryType, collection, targetProperty, groupBy, timeframe, null, filters, timezone)).ConfigureAwait(false);
            return ParseGroups(reply["result"], groupBy);
        }

        public async Task<IEnumerable<QueryIntervalValue<string>>> Metric(QueryType queryType, string collection,
            string targetProperty, IQueryTimeframe timeframe, QueryInterval interval,
            IEnumerable<QueryFilter> filters = null, string timezone = "")
        {
            ValidateMetric(queryType, collection, targetProperty);
            if (null == timeframe)
                throw new ArgumentException("Timeframe must be specified for a series query.", nameof(timeframe));
            if (null == interval)
                throw new ArgumentNullException(nameof(interval), "interval must be specified for a series query");
            var reply = await Request(queryType.ToString(),
                MetricParms(queryType, collection, targetProperty, null, timeframe, interval, filters, timezone)).ConfigureAwait(false);
            return ParseIntervals(reply["result"], v => IncisiveUtil.TokenToString(v));
        }

        public async Task<IEnumerable<QueryIntervalValue<IEnumerable<QueryGroupValue<string>>>>> Metric(
            QueryType queryType, string collection, string targetProperty, string groupBy,
            IQueryTimeframe timeframe, QueryInterval interval, IEnumerable<QueryFilter> filters = null,
            string timezone = "")
        {
            ValidateMetric(queryType, collection, targetProperty);
            if (null == timeframe)
                throw new ArgumentException("Timeframe must be specified for a series query.", nameof(timeframe));
            if (null == interval)
                throw new ArgumentNullException(nameof(interval), "interval must be specified for a series query");
            if (string.IsNullOrWhiteSpace(groupBy))
                throw new ArgumentNullException(nameof(groupBy), "groupBy field name must be specified for a group-by query");
            var reply = await Request(queryType.ToString(),
                MetricParms(queryType, collection, targetProperty, groupBy, timeframe, interval, filters, timezone)).ConfigureAwait(false);
            return ParseIntervals(reply["result"], v => ParseGroups(v, groupBy));
        }

        private static IEnumerable<QueryGroupValue<string>> ParseGroups(JToken result, string groupBy)
        {
            return (result as JArray ?? new JArray())
                .OfType<JObject>()
                .Select(r => new QueryGroupValue<string>(IncisiveUtil.TokenToString(r["result"]), IncisiveUtil.TokenToString(r[groupBy])))
                .ToList();
        }

        private static IEnumerable<QueryIntervalValue<T>> ParseIntervals<T>(JToken result, Func<JToken, T> valueSelector)
        {
            return (result as JArray ?? new JArray())
                .OfType<JObject>()
                .Select(i =>
                {
                    var tf = i["timeframe"] as JObject;
                    return new QueryIntervalValue<T>(valueSelector(i["value"]),
                        IncisiveUtil.ParseDate(tf?["start"]), IncisiveUtil.ParseDate(tf?["end"]));
                })
                .ToList();
        }

        public async Task<IEnumerable<dynamic>> Extract(string collection, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, int latest = 0, string email = "")
        {
            if (string.IsNullOrWhiteSpace(collection))
                throw new ArgumentNullException(nameof(collection));
            var parms = new List<KeyValuePair<string, string>>
            {
                Kv(IncisiveConstants.QueryParmEventCollection, collection),
                Kv(IncisiveConstants.QueryParmTimeframe, timeframe.ToSafeString()),
                Kv(IncisiveConstants.QueryParmFilters, FiltersJson(filters)),
                Kv(IncisiveConstants.QueryParmEmail, email),
                Kv(IncisiveConstants.QueryParmLatest, latest > 0 ? latest.ToString(System.Globalization.CultureInfo.InvariantCulture) : ""),
            };
            var reply = await Request(IncisiveConstants.QueryExtraction, parms).ConfigureAwait(false);
            // Email extractions answer with a status message instead of an event list.
            return reply["result"] is JArray arr ? arr.Select(i => (dynamic)i).ToList() : new List<dynamic>();
        }

        public async Task<FunnelResult> Funnel(IEnumerable<FunnelStep> steps, IQueryTimeframe timeframe = null, string timeZone = "")
        {
            if (steps == null)
                throw new ArgumentNullException(nameof(steps));
            var parms = new List<KeyValuePair<string, string>>();
            if (null != timeframe)
                parms.Add(Kv(IncisiveConstants.QueryParmTimeframe, timeframe.ToString()));
            parms.Add(Kv(IncisiveConstants.QueryParmTimezone, timeZone));
            parms.Add(Kv(IncisiveConstants.QueryParmSteps, JArray.FromObject(steps).ToString(Formatting.None)));
            var reply = await Request(IncisiveConstants.QueryFunnel, parms).ConfigureAwait(false);
            return reply.ToObject<FunnelResult>();
        }

        private static string AnalysesJson(IEnumerable<MultiAnalysisParam> analysisParams)
        {
            if (analysisParams == null)
                throw new ArgumentNullException(nameof(analysisParams));
            var o = new JObject();
            foreach (var a in analysisParams)
                o.Add(a.Label, a.ToJObject());
            return o.ToString(Formatting.None);
        }

        private static List<KeyValuePair<string, string>> MultiParms(string collection,
            IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe, QueryInterval interval,
            IEnumerable<QueryFilter> filters, string groupBy, string timezone)
        {
            if (string.IsNullOrWhiteSpace(collection))
                throw new ArgumentNullException(nameof(collection));
            var p = new List<KeyValuePair<string, string>>
            {
                Kv(IncisiveConstants.QueryParmEventCollection, collection),
                Kv(IncisiveConstants.QueryParmTimeframe, timeframe.ToSafeString()),
            };
            if (interval != null) p.Add(Kv(IncisiveConstants.QueryParmInterval, interval.ToString()));
            p.Add(Kv(IncisiveConstants.QueryParmTimezone, timezone));
            if (groupBy != null) p.Add(Kv(IncisiveConstants.QueryParmGroupBy, groupBy));
            p.Add(Kv(IncisiveConstants.QueryParmFilters, FiltersJson(filters)));
            p.Add(Kv(IncisiveConstants.QueryParmAnalyses, AnalysesJson(analysisParams)));
            return p;
        }

        private static IDictionary<string, string> ToDict(JToken o, string skip = null)
        {
            var d = new Dictionary<string, string>();
            if (o is JObject obj)
                foreach (var p in obj.Properties())
                    if (p.Name != skip)
                        d[p.Name] = IncisiveUtil.TokenToString(p.Value);
            return d;
        }

        private static IEnumerable<QueryGroupValue<IDictionary<string, string>>> ParseMultiGroups(JToken arr, string groupBy)
        {
            return (arr as JArray ?? new JArray())
                .OfType<JObject>()
                .Select(o => new QueryGroupValue<IDictionary<string, string>>(ToDict(o, groupBy),
                    IncisiveUtil.TokenToString(o[groupBy]) ?? ""))
                .ToList();
        }

        public async Task<IDictionary<string, string>> MultiAnalysis(string collection,
            IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, string timezone = "")
        {
            var reply = await Request(IncisiveConstants.QueryMultiAnalysis,
                MultiParms(collection, analysisParams, timeframe, null, filters, null, timezone)).ConfigureAwait(false);
            return ToDict(reply["result"]);
        }

        public async Task<IEnumerable<QueryGroupValue<IDictionary<string, string>>>> MultiAnalysis(string collection,
            IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null,
            IEnumerable<QueryFilter> filters = null, string groupby = "", string timezone = "")
        {
            var reply = await Request(IncisiveConstants.QueryMultiAnalysis,
                MultiParms(collection, analysisParams, timeframe, null, filters, groupby ?? "", timezone)).ConfigureAwait(false);
            return ParseMultiGroups(reply["result"], groupby);
        }

        public async Task<IEnumerable<QueryIntervalValue<IDictionary<string, string>>>> MultiAnalysis(string collection,
            IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null,
            QueryInterval interval = null, IEnumerable<QueryFilter> filters = null, string timezone = "")
        {
            var reply = await Request(IncisiveConstants.QueryMultiAnalysis,
                MultiParms(collection, analysisParams, timeframe, interval, filters, null, timezone)).ConfigureAwait(false);
            return ParseIntervals(reply["result"], v => ToDict(v));
        }

        public async Task<IEnumerable<QueryIntervalValue<IEnumerable<QueryGroupValue<IDictionary<string, string>>>>>> MultiAnalysis(
            string collection, IEnumerable<MultiAnalysisParam> analysisParams, IQueryTimeframe timeframe = null,
            QueryInterval interval = null, IEnumerable<QueryFilter> filters = null, string groupby = "", string timezone = "")
        {
            var reply = await Request(IncisiveConstants.QueryMultiAnalysis,
                MultiParms(collection, analysisParams, timeframe, interval, filters, groupby ?? "", timezone)).ConfigureAwait(false);
            return ParseIntervals(reply["result"], v => ParseMultiGroups(v, groupby));
        }

        // ---- saved / cached queries --------------------------------------------------

        private string SavedUrl(string name = null) =>
            _transport.ProjectUrl(IncisiveConstants.QueriesResource + "/" + IncisiveConstants.SavedQueriesResource +
                                  (name == null ? "" : "/" + IncisiveUtil.EscapePath(name)));

        private string RequireMaster(string op)
        {
            if (string.IsNullOrWhiteSpace(_settings.MasterKey))
                throw new IncisiveException("An API MasterKey is required to " + op + ".");
            return _settings.MasterKey;
        }

        private static void RequireName(string name)
        {
            if (string.IsNullOrWhiteSpace(name))
                throw new IncisiveException("A saved query name is required.");
        }

        public async Task<JArray> ListSavedQueries()
        {
            var resp = await _transport.GetAsync(SavedUrl(), RequireMaster("list saved queries")).ConfigureAwait(false);
            return IncisiveUtil.RequireArray(IncisiveUtil.CheckResponse(resp, "ListSavedQueries"), "ListSavedQueries");
        }

        public async Task<JObject> GetSavedQuery(string queryName)
        {
            RequireName(queryName);
            var resp = await _transport.GetAsync(SavedUrl(queryName), RequireMaster("get a saved query")).ConfigureAwait(false);
            return IncisiveUtil.RequireObject(IncisiveUtil.CheckResponse(resp, "GetSavedQuery"), "GetSavedQuery");
        }

        public async Task<JObject> GetSavedQueryResult(string queryName)
        {
            RequireName(queryName);
            if (string.IsNullOrWhiteSpace(_key))
                throw new IncisiveException("An API ReadKey or MasterKey is required.");
            var resp = await _transport.GetAsync(SavedUrl(queryName) + "/result", _key).ConfigureAwait(false);
            return IncisiveUtil.RequireObject(IncisiveUtil.CheckResponse(resp, "GetSavedQueryResult"), "GetSavedQueryResult");
        }

        public async Task<JObject> PutSavedQuery(string queryName, JObject body)
        {
            RequireName(queryName);
            if (body == null)
                throw new ArgumentNullException(nameof(body));
            var resp = await _transport.PutAsync(SavedUrl(queryName), RequireMaster("save a query"), body.ToString(Formatting.None))
                .ConfigureAwait(false);
            return IncisiveUtil.CheckResponse(resp, "PutSavedQuery") as JObject ?? new JObject();
        }

        public async Task DeleteSavedQuery(string queryName)
        {
            RequireName(queryName);
            var resp = await _transport.DeleteAsync(SavedUrl(queryName), RequireMaster("delete a saved query")).ConfigureAwait(false);
            IncisiveUtil.CheckResponse(resp, "DeleteSavedQuery");
        }
    }
}
