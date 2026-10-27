using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;
using Newtonsoft.Json.Serialization;

namespace Incisive
{
    /// <summary>A cached dataset definition (serialized snake_case).</summary>
    public class DatasetDefinition
    {
        public string DatasetName { get; set; }
        public string DisplayName { get; set; }
        public QueryDefinition Query { get; set; }
        public DateTime? LastScheduledDate { get; set; }
        public DateTime? LatestSubtimeframeAvailable { get; set; }
        public long MillisecondsBehind { get; set; }
        public IEnumerable<string> IndexBy { get; set; }

        internal void Validate()
        {
            if (string.IsNullOrWhiteSpace(DatasetName))
                throw new IncisiveException("DatasetDefinition must have a name.");
            if (string.IsNullOrWhiteSpace(DisplayName))
                throw new IncisiveException("DatasetDefinition must have a display name.");
            if (null == IndexBy || string.IsNullOrWhiteSpace(IndexBy.FirstOrDefault()))
                throw new IncisiveException("DatasetDefinition must specify a property by which to index.");
            if (null == Query)
                throw new IncisiveException("DatasetDefinition must contain a query to be cached.");
            if (string.IsNullOrWhiteSpace(Query.AnalysisType))
                throw new IncisiveException("QueryDefinition must have an analysis type");
            if (string.IsNullOrWhiteSpace(Query.EventCollection))
                throw new IncisiveException("QueryDefinition must specify an event collection");
            if (string.IsNullOrWhiteSpace(Query.Timeframe))
                throw new IncisiveException("QueryDefinition must specify a timeframe");
            if (string.IsNullOrWhiteSpace(Query.Interval))
                throw new IncisiveException("QueryDefinition must specify an interval");
        }
    }

    /// <summary>One page of dataset definitions.</summary>
    public class DatasetDefinitionCollection
    {
        public IEnumerable<DatasetDefinition> Datasets { get; set; }
        public string NextPageUrl { get; set; }
        public int Count { get; set; }
    }

    /// <summary>Cached dataset operations (<c>projects/{id}/datasets</c>).</summary>
    public interface IDataset
    {
        Task<JObject> GetResultsAsync(string datasetName, string indexBy, string timeframe);
        Task<DatasetDefinition> GetDefinitionAsync(string datasetName);
        Task<DatasetDefinitionCollection> ListDefinitionsAsync(int limit = 10, string afterName = null);
        Task<IEnumerable<DatasetDefinition>> ListAllDefinitionsAsync();
        Task DeleteDatasetAsync(string datasetName);
        Task<DatasetDefinition> CreateDatasetAsync(DatasetDefinition dataset);
    }

    internal sealed class Datasets : IDataset
    {
        private const int MaxDatasetDefinitionListLimit = 100;

        internal static readonly JsonSerializerSettings SerializerSettings = new JsonSerializerSettings
        {
            ContractResolver = new DefaultContractResolver { NamingStrategy = new SnakeCaseNamingStrategy() },
            DefaultValueHandling = DefaultValueHandling.IgnoreAndPopulate,
            Formatting = Formatting.None,
            DateParseHandling = DateParseHandling.None,
        };

        private readonly Transport _transport;
        private readonly IProjectSettings _settings;

        internal Datasets(IProjectSettings settings, Transport transport)
        {
            _settings = settings;
            _transport = transport;
        }

        private string DatasetUrl(string name = null) =>
            _transport.ProjectUrl(IncisiveConstants.DatasetsResource + (name == null ? "" : "/" + IncisiveUtil.EscapePath(name)));

        private string RequireRead()
        {
            if (string.IsNullOrWhiteSpace(_settings.ReadKey))
                throw new IncisiveException("An API ReadKey is required to get dataset results.");
            return _settings.ReadKey;
        }

        private string RequireMaster()
        {
            if (string.IsNullOrWhiteSpace(_settings.MasterKey))
                throw new IncisiveException("An API MasterKey is required to modify datasets.");
            return _settings.MasterKey;
        }

        private static T Deserialize<T>(JToken token) =>
            token.ToObject<T>(JsonSerializer.Create(SerializerSettings));

        public async Task<JObject> GetResultsAsync(string datasetName, string indexBy, string timeframe)
        {
            if (string.IsNullOrWhiteSpace(datasetName))
                throw new IncisiveException("A dataset name is required.");
            if (string.IsNullOrWhiteSpace(indexBy))
                throw new IncisiveException("A value to index by is required.");
            if (string.IsNullOrWhiteSpace(timeframe))
                throw new IncisiveException("A timeframe by is required.");

            var url = DatasetUrl(datasetName) + "/results?" + IncisiveUtil.BuildQueryString(new[]
            {
                new KeyValuePair<string, string>("index_by", indexBy),
                new KeyValuePair<string, string>("timeframe", timeframe),
            });
            var resp = await _transport.GetAsync(url, RequireRead()).ConfigureAwait(false);
            return IncisiveUtil.RequireObject(IncisiveUtil.CheckResponse(resp, "GetDatasetResults"), "GetDatasetResults");
        }

        public async Task<DatasetDefinition> GetDefinitionAsync(string datasetName)
        {
            if (string.IsNullOrWhiteSpace(datasetName))
                throw new IncisiveException("A dataset name is required.");
            var resp = await _transport.GetAsync(DatasetUrl(datasetName), RequireRead()).ConfigureAwait(false);
            var token = IncisiveUtil.RequireObject(IncisiveUtil.CheckResponse(resp, "GetDatasetDefinition"), "GetDatasetDefinition");
            return Deserialize<DatasetDefinition>(token);
        }

        public async Task<DatasetDefinitionCollection> ListDefinitionsAsync(int limit = 10, string afterName = null)
        {
            var parms = new List<KeyValuePair<string, string>>
            {
                new KeyValuePair<string, string>("limit", limit.ToString(System.Globalization.CultureInfo.InvariantCulture)),
            };
            if (!string.IsNullOrWhiteSpace(afterName))
                parms.Add(new KeyValuePair<string, string>("after_name", afterName));
            var resp = await _transport.GetAsync(DatasetUrl() + "?" + IncisiveUtil.BuildQueryString(parms), RequireRead())
                .ConfigureAwait(false);
            var token = IncisiveUtil.RequireObject(IncisiveUtil.CheckResponse(resp, "ListDatasetDefinitions"), "ListDatasetDefinitions");
            return Deserialize<DatasetDefinitionCollection>(token);
        }

        public async Task<IEnumerable<DatasetDefinition>> ListAllDefinitionsAsync()
        {
            var all = new List<DatasetDefinition>();
            var first = await ListDefinitionsAsync(MaxDatasetDefinitionListLimit).ConfigureAwait(false);
            if (null == first?.Datasets)
                throw new IncisiveException("Failed to fetch definition list");
            if (!first.Datasets.Any())
                return all;
            all.AddRange(first.Datasets);
            if (first.Count <= all.Count)
                return all;

            do
            {
                var next = await ListDefinitionsAsync(MaxDatasetDefinitionListLimit, all.Last().DatasetName).ConfigureAwait(false);
                if (null == next?.Datasets || !next.Datasets.Any())
                    throw new IncisiveException("Failed to fetch definition list");
                all.AddRange(next.Datasets);
            } while (first.Count > all.Count);

            return all;
        }

        public async Task DeleteDatasetAsync(string datasetName)
        {
            if (string.IsNullOrWhiteSpace(datasetName))
                throw new IncisiveException("A dataset name is required.");
            var resp = await _transport.DeleteAsync(DatasetUrl(datasetName), RequireMaster()).ConfigureAwait(false);
            IncisiveUtil.CheckResponse(resp, "DeleteDataset");
        }

        public async Task<DatasetDefinition> CreateDatasetAsync(DatasetDefinition dataset)
        {
            var key = RequireMaster();
            if (null == dataset)
                throw new IncisiveException("An instance of DatasetDefinition must be provided");
            dataset.Validate();

            var content = JsonConvert.SerializeObject(dataset, SerializerSettings);
            var resp = await _transport.PutAsync(DatasetUrl(dataset.DatasetName), key, content).ConfigureAwait(false);
            var token = IncisiveUtil.RequireObject(IncisiveUtil.CheckResponse(resp, "CreateDataset"), "CreateDataset");
            return Deserialize<DatasetDefinition>(token);
        }
    }
}
