# Incisive .NET SDK

Client library for the Incisive analytics server. It records events, runs analyses, manages
saved/cached queries and cached datasets, and generates scoped keys.

- NuGet id / assembly / namespace: `Incisive`
- Targets `net8.0` and `netstandard2.0`; depends on Newtonsoft.Json (results are `JObject`/`JArray`/`dynamic`).

## Configure

```csharp
using Incisive;

var settings = new ProjectSettingsProvider(
    projectId: "PROJECT_ID",
    masterKey: "MASTER_KEY",   // deletes, saved queries, dataset admin
    writeKey:  "WRITE_KEY",    // recording events
    readKey:   "READ_KEY",     // queries, schemas
    serverUrl: "http://localhost:8080"); // default; the API version (3.0) is appended

var client = new IncisiveClient(settings);
```

Or from the environment: `new ProjectSettingsProviderEnv()` reads `INCISIVE_PROJECT_ID`,
`INCISIVE_MASTER_KEY`, `INCISIVE_WRITE_KEY`, `INCISIVE_READ_KEY`, `INCISIVE_BASE_URL` and
optionally `INCISIVE_API_VERSION`. `ProjectSettingsProviderFile(path)` reads a JSON file that uses
those names as keys.

Requests go to `{serverUrl}/3.0/projects/{projectId}/...` with the key in the `Authorization`
header. Pass an `HttpMessageHandler` to the constructor for proxies, logging or tests.

## Record events

```csharp
client.AddEvent("purchases", new { item = "widget", price = 12.5 });
await client.AddEventsAsync("purchases", new object[] { new { price = 1 }, new { price = 2 } });

client.AddGlobalProperty("app_version", "1.4.2");
client.AddGlobalProperty("sent_at", new DynamicPropertyValue(() => DateTime.UtcNow));

client.AddEvent("visits", new { ip = "1.2.3.4" }, new[] { AddOn.IpToGeo("ip", "geo") });
```

Each event gets a reserved metadata object with a UTC `timestamp` (unless you set one) and any
add-ons. Bulk inserts that partially fail throw `IncisiveBulkException` with `FailedEvents`.

Event caching: `new IncisiveClient(settings, new EventCacheMemory())` queues events instead of
sending them; `SendCachedEvents()` flushes them in bulk batches of 1000 per collection.

## Query

```csharp
var tf = QueryRelativeTimeframe.ThisNDays(7);
string count = client.Query(QueryType.Count(), "purchases", null, tf);
string total = client.Query(QueryType.Sum(), "purchases", "price", tf,
    new[] { new QueryFilter("price", QueryFilter.FilterOperator.GreaterThan(), 1) });
string p90 = client.Query(QueryType.Percentile(90), "purchases", "price", tf);

var byPlan  = client.QueryGroup(QueryType.Count(), "purchases", null, "plan", tf);
var daily   = client.QueryInterval(QueryType.Average(), "purchases", "price", tf, QueryInterval.Daily());
var both    = client.QueryIntervalGroup(QueryType.Count(), "purchases", null, "plan", tf, QueryInterval.Daily());
var events  = client.QueryExtractResource("purchases", tf, latest: 100);
var funnel  = client.QueryFunnel(new[] {
    new FunnelStep { EventCollection = "signup", ActorProperty = "user.id", Timeframe = tf },
    new FunnelStep { EventCollection = "purchase", ActorProperty = "user.id", Timeframe = tf },
});
var multi   = client.QueryMultiAnalysis("purchases", new[] {
    new MultiAnalysisParam("count", MultiAnalysisParam.Metric.Count()),
    new MultiAnalysisParam("revenue", MultiAnalysisParam.Metric.Sum("price")),
}, tf);
JObject raw = client.Query("count", new Dictionary<string, string> { ["event_collection"] = "purchases" });
```

Analyses: `Count`, `CountUnique`, `Minimum`, `Maximum`, `Average`, `Sum`, `Median`,
`Percentile(p)`, `StandardDeviation`, `SelectUnique` (joined with ","), plus extraction, funnel and
multi-analysis (each with group-by / interval / interval+group-by variants). Absolute timeframes:
`new QueryAbsoluteTimeframe(start, end)`. All queries are `GET` requests with parameters in the
query string; JSON-valued parameters (`filters`, `timeframe`, `steps`, `analyses`) are compact JSON,
percent-encoded. Queries use the master key when configured, otherwise the read key.

## Saved and cached queries

```csharp
client.CreateSavedQuery("weekly-revenue", new {
    refresh_rate = 0,
    query = new { analysis_type = "sum", event_collection = "purchases", target_property = "price", timeframe = "this_7_days" },
});
client.CreateCachedQuery("hourly-count", new { analysis_type = "count", event_collection = "purchases", timeframe = "this_1_days" }, refreshRate: 3600);
JObject result = client.GetSavedQueryResult("weekly-revenue");
JArray all = client.ListSavedQueries();
client.DeleteSavedQuery("weekly-revenue");
```

## Cached datasets

`CreateDataset(DatasetDefinition)`, `GetDatasetDefinition(name)`, `ListDatasetDefinitions(limit, afterName)`,
`ListAllDatasetDefinitions()`, `QueryDataset(name, indexBy, timeframe)`, `DeleteDataset(name)`.

## Collections and schemas

`GetSchemas()` (all collections), `GetSchema(collection)`, `DeleteCollection(collection)` (master key).

## Scoped keys

```csharp
string scoped = ScopedKey.Encrypt(masterKey, new {
    allowed_operations = new[] { "read" },
    filters = new[] { new { property_name = "account_id", @operator = "eq", property_value = 42 } },
});
string json = ScopedKey.Decrypt(masterKey, scoped);
```

AES-256-CBC with PKCS7 padding; the AES key is the master key hex-decoded (64-char keys) or its
UTF-8 bytes (32-char keys). Output is upper-case hex of a random 16-byte IV followed by the ciphertext.

## Errors

Every error derives from `IncisiveException`, which carries `StatusCode`, `ErrorCode` and
`ServerMessage` when they came from a response. A JSON body `{"error_code": ..., "message": ...}`
maps to a typed exception: `IncisiveInvalidApiKeyException`, `IncisiveResourceNotFoundException`,
`IncisiveNamespaceTypeException`, `IncisiveInvalidEventException`,
`IncisiveListsOfNonPrimitivesNotAllowedException`, `IncisiveInvalidBatchException`,
`IncisiveInternalServerErrorException`, `IncisiveInvalidPropertyNameException`,
`IncisiveInvalidReservedNamespacePropertyException`; unknown codes raise `IncisiveException("<code> : <message>")`.
Other non-2xx responses raise `IncisiveException` with the status code.

## Tests

.NET does not need to be installed; use the pinned SDK image:

```sh
docker run --rm -v "$PWD":/w -w /w mcr.microsoft.com/dotnet/sdk:8.0.404 dotnet test Incisive.sln
```

Unit tests run against an in-process mock HTTP server on `127.0.0.1`. Integration tests run only
when `INCISIVE_BASE_URL` and `INCISIVE_PROJECT_ID` (plus `INCISIVE_WRITE_KEY`, `INCISIVE_READ_KEY`,
optionally `INCISIVE_MASTER_KEY`) are set; add `--network host` and `-e INCISIVE_...` to reach a
local server.
