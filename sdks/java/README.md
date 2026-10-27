# incisive-java

Java 11+ client for the incisive analytics server: event tracking (single, async, queued
batches), analyses (count, sums, percentiles, funnels, multi-analysis, ...), saved/cached queries,
cached datasets and scoped keys. The only runtime dependency is Jackson.

```xml
<dependency>
  <groupId>io.github.dorkitude.incisive</groupId>
  <artifactId>incisive-java</artifactId>
  <version>0.1.0</version>
</dependency>
```

## Configuration

| Setting | Builder method | Environment variable | Default |
|---|---|---|---|
| Base URL | `withBaseUrl(...)` | `INCISIVE_BASE_URL` | `http://localhost:8080` |
| API version | `withApiVersion(...)` | none | `1.0` |
| Project id | `IncisiveProject(...)` | `INCISIVE_PROJECT_ID` | none |
| Write key | `IncisiveProject(...)` | `INCISIVE_WRITE_KEY` | none |
| Read key | `IncisiveProject(...)` | `INCISIVE_READ_KEY` | none |
| Master key | `IncisiveProject(...)` | `INCISIVE_MASTER_KEY` | none |

`new IncisiveProject()` reads the project from the environment. Both client builders also pick up
a default project and base URL from the environment when none is given.

Keys are sent as the raw value of the `Authorization` header. The write key is used for events.
The read key is used for analyses, saved query results and dataset reads. The master key is used
for saved query and dataset management.

## Tracking

```java
import io.github.dorkitude.incisive.client.*;

IncisiveClient client = new JavaIncisiveClientBuilder()
        .withBaseUrl("http://localhost:8080")
        .withDefaultProject(new IncisiveProject("PROJECT_ID", "WRITE_KEY", "READ_KEY"))
        .build();
IncisiveClient.initialize(client);          // optional shared instance: IncisiveClient.client()

Map<String, Object> event = new HashMap<>();
event.put("item", "golden widget");
event.put("price", 25);

client.addEvent("purchases", event);                 // synchronous
client.addEventAsync("purchases", event);            // on the publish executor
client.queueEvent("purchases", event);               // stored in the event store...
client.sendQueuedEvents();                           // ...and sent in one batch request
```

All overloads accept an optional `IncisiveProject`, a map of reserved metadata (merged into the
reserved metadata object, e.g. `timestamp`) and an `IncisiveCallback` /
`IncisiveDetailedCallback`. Failures go to the callback; `setDebugMode(true)` makes them throw.

Other client features:
- `setGlobalProperties(Map)` and `setGlobalPropertiesEvaluator(collection -> Map)` add properties
  to every event.
- Queued events live in an `IncisiveEventStore`. `RamEventStore` is the default. Plug in your own
  store (for example a file-backed one on Android) with `withEventStore`.
- `setMaxAttempts(n)` sets how many batch attempts an event gets before it is dropped.
- Builder hooks: `withHttpHandler`, `withJsonHandler`, `withPublishExecutor`,
  `withNetworkStatusHandler` (return false while offline and queued events are kept),
  `withConnectTimeout`, `withReadTimeout`, and `setProxy(...)`.

A timestamp in the form `yyyy-MM-dd'T'HH:mm:ss.SSSZ` is added unless you supply one.

## Queries

```java
import io.github.dorkitude.incisive.client.query.*;

IncisiveQueryClient queries = new IncisiveQueryClient.Builder(
        new IncisiveProject("PROJECT_ID", null, "READ_KEY", "MASTER_KEY"))
        .withBaseUrl("http://localhost:8080")
        .build();

long n = queries.count("purchases", new RelativeTimeframe("this_7_days"));
double total = queries.sum("purchases", "price", new RelativeTimeframe("this_7_days", "UTC"));

QueryResult byCountry = queries.execute(new Query.Builder(QueryType.COUNT)
        .withEventCollection("purchases")
        .withFilter("price", FilterOperator.GREATER_THAN, 10)
        .withGroupBy("country")
        .withInterval("daily")
        .withTimeframe(new AbsoluteTimeframe("2026-01-01T00:00:00Z", "2026-02-01T00:00:00Z"))
        .build());
```

Shortcuts: `count`, `countUnique`, `minimum`, `maximum`, `average`, `median`, `percentile`, `sum`,
`selectUnique` and `standardDeviation`. `execute(...)` also accepts:
- `SingleAnalysis`, a strictly validated single analysis.
- `MultiAnalysis` with `SubAnalysis` entries. The result is a `MultiAnalysisResult`.
- `Funnel` with `FunnelStep`s. The result is a `FunnelResult`, with actors if any step asks for
  `with_actors`.

Results are `LongResult`, `DoubleResult`, `StringResult`, `ListResult`, `IntervalResult` or
`GroupByResult` (keyed by `Group`).

Saved and cached queries:

```java
SavedQueries saved = queries.getSavedQueriesInterface();
saved.createSavedQuery("weekly-purchases", query, "Weekly purchases");
saved.createCachedQuery("weekly-cached", query, RefreshRate.fromHours(6));
QueryResult r = saved.getResult("weekly-purchases");
saved.setDisplayName("weekly-purchases", "Renamed");
saved.deleteQuery("weekly-purchases");
```

Cached datasets: `queries.getCachedDatasetsClient()` provides `create`, `getDefinition`,
`getDefinitions(limit, afterName)`, `getResults(definition, indexByValues, timeframe)` and
`delete`.

## Errors

- `ServerException` is thrown for any non-2xx response. `getMessage()` is the raw body.
  `getStatusCode()`, `getErrorCode()` and `getServerMessage()` expose the HTTP status and the
  `error_code` / `message` fields of the JSON error.
- `IncisiveQueryClientException` is thrown for a 2xx body that carries `error_code` and no
  `result`, and for responses it cannot parse.
- `InvalidEventException`, `InvalidEventCollectionException` and `NoWriteKeyException` are
  thrown when an event fails validation on the client.

## Scoped keys

```java
Map<String, Object> options = Map.of("allowed_operations", List.of("read"));
String scoped = ScopedKeys.encrypt(masterKey, options);
Map<String, Object> decoded = ScopedKeys.decrypt(masterKey, scoped);
```

The cipher is AES-256-CBC with PKCS#7 padding and a random IV. The output is the uppercase hex of
the IV followed by the uppercase hex of the ciphertext. A 64-character key is hex-decoded. Any
other key is padded to a multiple of 32 characters and its UTF-8 bytes are used.

## Running tests

Java is not required locally; use the pinned Maven image:

```sh
docker run --rm -v incisive-m2:/root/.m2 -v "$PWD":/w -w /w \
  maven:3.9.9-eclipse-temurin-21 mvn -B test
```

The unit tests run against an in-process HTTP server and check the exact wire format.
`IntegrationTest` runs only when `INCISIVE_BASE_URL`, `INCISIVE_PROJECT_ID`,
`INCISIVE_WRITE_KEY` and `INCISIVE_READ_KEY` are set:

```sh
docker run --rm --network host -v incisive-m2:/root/.m2 -v "$PWD":/w -w /w \
  -e INCISIVE_BASE_URL=http://localhost:8080 -e INCISIVE_PROJECT_ID=... \
  -e INCISIVE_WRITE_KEY=... -e INCISIVE_READ_KEY=... \
  maven:3.9.9-eclipse-temurin-21 mvn -B test
```
