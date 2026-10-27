namespace Incisive
{
    /// <summary>Wire-level constants shared by the client.</summary>
    public static class IncisiveConstants
    {
        /// <summary>Default server address (no API version).</summary>
        public const string DefaultBaseUrl = "http://localhost:8080";

        /// <summary>Default HTTP API version path segment.</summary>
        public const string ApiVersion = "3.0";

        /// <summary>SDK version, used in the User-Agent header.</summary>
        public const string SdkVersion = "0.1.0";

        /// <summary>Maximum number of events sent in one bulk request when flushing the cache.</summary>
        public const int BulkBatchSize = 1000;

        /// <summary>
        /// Name of the reserved metadata object that every event carries
        /// (holds the timestamp and data enrichment add-ons).
        /// </summary>
        public const string ReservedNamespace = "keen";

        // Resources
        public const string EventsResource = "events";
        public const string QueriesResource = "queries";
        public const string SavedQueriesResource = "saved";
        public const string DatasetsResource = "datasets";

        // Analyses
        public const string QueryCount = "count";
        public const string QueryCountUnique = "count_unique";
        public const string QueryMinimum = "minimum";
        public const string QueryMaximum = "maximum";
        public const string QueryAverage = "average";
        public const string QuerySum = "sum";
        public const string QueryMedian = "median";
        public const string QueryPercentile = "percentile";
        public const string QueryStandardDeviation = "standard_deviation";
        public const string QuerySelectUnique = "select_unique";
        public const string QueryExtraction = "extraction";
        public const string QueryFunnel = "funnel";
        public const string QueryMultiAnalysis = "multi_analysis";

        // Query parameters
        public const string QueryParmEventCollection = "event_collection";
        public const string QueryParmTargetProperty = "target_property";
        public const string QueryParmTimeframe = "timeframe";
        public const string QueryParmGroupBy = "group_by";
        public const string QueryParmInterval = "interval";
        public const string QueryParmTimezone = "timezone";
        public const string QueryParmFilters = "filters";
        public const string QueryParmEmail = "email";
        public const string QueryParmLatest = "latest";
        public const string QueryParmSteps = "steps";
        public const string QueryParmAnalyses = "analyses";
        public const string QueryParmPercentile = "percentile";

        // Environment variables read by ProjectSettingsProviderEnv / ProjectSettingsProviderFile
        public const string EnvProjectId = "INCISIVE_PROJECT_ID";
        public const string EnvMasterKey = "INCISIVE_MASTER_KEY";
        public const string EnvWriteKey = "INCISIVE_WRITE_KEY";
        public const string EnvReadKey = "INCISIVE_READ_KEY";
        public const string EnvBaseUrl = "INCISIVE_BASE_URL";
        public const string EnvApiVersion = "INCISIVE_API_VERSION";
    }
}
