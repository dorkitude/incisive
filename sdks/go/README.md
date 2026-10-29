# incisive Go SDK

A Go client for the incisive analytics server. It uses only the standard library.

```
go get github.com/dorkitude/incisive/sdks/go
```

```go
import incisive "github.com/dorkitude/incisive/sdks/go"
```

## Configuration

```go
c := &incisive.Client{
    ProjectID: "PROJECT_ID",
    WriteKey:  "WRITE_KEY",
    ReadKey:   "READ_KEY",   // falls back to MasterKey when empty
    MasterKey: "MASTER_KEY",
    BaseURL:   "https://analytics.example.com", // default http://localhost:8080
    // APIVersion: "1.0" (default), HTTPClient: &http.Client{Timeout: 10 * time.Second}, UserAgent: "..."
}

// Or read INCISIVE_PROJECT_ID, INCISIVE_WRITE_KEY, INCISIVE_READ_KEY,
// INCISIVE_MASTER_KEY, INCISIVE_BASE_URL (and INCISIVE_API_VERSION):
c, err := incisive.NewClientFromEnv()
```

Every request sends the key as a raw `Authorization` header. Bodies are JSON.

## Tracking events

```go
type Purchase struct {
    Item  string                 `json:"item"`
    Price float64                `json:"price"`
    Meta  incisive.EventMetadata `json:"incisive"` // reserved metadata object (optional)
}

err := c.AddEvent("purchases", Purchase{Item: "ball", Price: 3.5,
    Meta: incisive.EventMetadata{Timestamp: incisive.Timestamp(time.Now())}})

err = c.AddEvents(map[string][]interface{}{
    "purchases": {Purchase{Item: "a"}, Purchase{Item: "b"}},
    "signups":   {map[string]string{"plan": "pro"}},
})

// Context-aware variants; AddEventsContext also returns per-event results.
res, err := c.AddEventsContext(ctx, batch)
fmt.Println(res.Failed())
```

### Background batching

```go
b := incisive.NewBatchClient(c, 10*time.Second,
    incisive.WithMaxBatchSize(500),                       // optional
    incisive.WithErrorHandler(func(err error) { log.Println(err) })) // optional
defer b.Close() // stops the timer and flushes what's left

b.AddEvent("purchases", Purchase{Item: "ball"}) // buffered, never blocks on I/O
b.Flush()                                        // send now and wait
```

`BatchClient` embeds `*Client`, so all other methods are available on it. Events still
buffered when the process exits without `Close` are lost.

### Server-side tracking

`Tracker` records events from a server without ever slowing down or failing a request: events
are buffered (at most 10,000 by default), sent every 2 seconds, and dropped if the server can't
be reached. Each event gets an `app` object, a timestamp, and enrichment add-ons that turn
`client.ip` into `ip_geo_info` and `client.user_agent` into `tech`.

```go
tracker := incisive.NewTracker(c, incisive.TrackerOptions{
    App:     map[string]any{"name": "shop"},
    OnError: func(err error) { log.Println("analytics:", err) }, // at most once a minute
})
defer tracker.Close()

tracker.Track("purchases", map[string]any{
    "item":   map[string]any{"name": "hat", "price": 25},
    "caller": map[string]any{"key": "user:42", "plan": "pro"},
    "client": incisive.ClientFacts(r), // ip, user_agent, library
})
```

`tracker.Middleware` records every API request as an event in `api_requests`: the endpoint, the
request (secrets in the query string are redacted), the response status, size, duration and
error message, the client, and whoever made the call:

```go
mux.Handle("/api/", tracker.Middleware(incisive.RequestTracking{
    Route:  func(r *http.Request) string { return chi.RouteContext(r.Context()).RoutePattern() },
    Caller: func(r *http.Request) map[string]any { return lookUpAccount(r.Header.Get("Authorization")) },
})(apiHandler))
```

`Caller` runs after the response has been sent, so a database lookup there costs the client
nothing. A nil `*Tracker` does nothing, which makes it easy to switch off in tests.

## Analyses

All analyses POST a JSON body to `/1.0/projects/{id}/queries/{analysis}` with the read
key and return a `*Result` (`Value` is the unwrapped `result`, `Envelope` the whole body).

```go
tf := incisive.RelativeTimeframe("this_14_days")

r, err := c.Count(ctx, "purchases", incisive.QueryParams{Timeframe: tf})
n, _ := r.Int64()

r, err = c.Sum(ctx, "purchases", "price", incisive.QueryParams{
    Timeframe: incisive.AbsoluteTimeframe(start, end),
    Timezone:  incisive.NamedTimezone("US/Pacific"), // or incisive.OffsetTimezone(-28800)
    Filters: []incisive.Filter{
        {PropertyName: "price", Operator: "gte", PropertyValue: 0.99},
        incisive.Or(
            incisive.Filter{PropertyName: "tier", Operator: "eq", PropertyValue: "gold"},
            incisive.Filter{PropertyName: "vip", Operator: "eq", PropertyValue: true},
        ),
    },
    GroupBy:  incisive.GroupByProperties("platform"),
    Interval: "daily",
    OrderBy:  incisive.OrderBy{{PropertyName: "result", Direction: incisive.Descending}},
    Limit:    10,
    IncludeMetadata: true,
})
intervals, _ := r.Intervals()
```

Methods: `Count`, `CountUnique`, `Sum`, `Average`, `Minimum`, `Maximum`, `Median`,
`Percentile`, `SelectUnique`, `StandardDeviation`, `Extraction` (set `Email`,
`PropertyNames`, `Latest`, `ContentType`, `ContentEncoding` as needed), `Funnel`
(`[]FunnelStep`), `MultiAnalysis` (`map[string]Analysis`), and the generic
`Query(ctx, analysisType, params)`. Group-by buckets and ranges are supported via
`GroupByClause{Buckets: ..., Ranges: ...}`. Unmodelled parameters can be passed in
`QueryParams.Extra`.

## Saved queries and cached datasets

```go
c.CreateSavedQuery(ctx, "revenue", incisive.SavedQueryInput{RefreshRate: 3600,
    Query: incisive.QueryParams{AnalysisType: incisive.AnalysisSum,
        EventCollection: "purchases", TargetProperty: "price", Timeframe: tf}})
c.SavedQueries(ctx); c.SavedQuery(ctx, "revenue"); c.SavedQueryResult(ctx, "revenue")
c.UpdateSavedQuery(ctx, "revenue", def)                                  // full replace
c.UpdateSavedQueryAttributes(ctx, "revenue", map[string]interface{}{"refresh_rate": 600}) // partial
c.DeleteSavedQuery(ctx, "revenue")

c.CreateCachedDataset(ctx, "ds", incisive.CachedDatasetInput{DisplayName: "D",
    IndexBy: incisive.IndexBy{"product.id"}, Query: q})
c.CachedDatasets(ctx, incisive.ListOptions{Limit: 10}); c.CachedDataset(ctx, "ds")
c.CachedDatasetResults(ctx, "ds", incisive.DatasetResultsParams{IndexBy: "p1", Timeframe: tf})
c.DeleteCachedDataset(ctx, "ds")
```

## Discovery, deletes, access keys

```go
c.Collections(ctx); c.Collection(ctx, "purchases"); c.Property(ctx, "purchases", "price")

c.DeleteEvents(ctx, "purchases", incisive.DeleteParams{Filters: f, Timeframe: tf})
c.DeleteCollection(ctx, "purchases")
c.DeleteProperty(ctx, "purchases", "user.email")

k, _ := c.CreateAccessKey(ctx, incisive.AccessKeyInput{Name: "dashboard",
    Permitted: []string{incisive.PermitQueries}})
c.ListAccessKeys(ctx, incisive.AccessKeyListOptions{}); c.GetAccessKey(ctx, k.Key)
c.UpdateAccessKey(ctx, k.Key, in); c.RevokeAccessKey(ctx, k.Key)
c.UnrevokeAccessKey(ctx, k.Key); c.DeleteAccessKey(ctx, k.Key)
```

## Scoped keys

```go
scoped, err := incisive.EncryptScopedKey(masterKey, incisive.ScopedKeyOptions{
    AllowedOperations: []string{"read"},
    Filters: []incisive.Filter{{PropertyName: "account.id", Operator: "eq", PropertyValue: "42"}},
})
opts, err := incisive.DecryptScopedKey(masterKey, scoped)
```

AES-CBC with a random IV, output `hex(iv) + hex(ciphertext)`. A 64-character hex key is
used as a 256-bit key with 16-byte padding; other keys (legacy 32-character form) are used
as raw bytes padded to 32 with 32-byte plaintext padding. Already-aligned input gets no
padding block. This is byte-compatible with the other established SDKs' scoped keys.

## Errors

Non-2xx responses return `*incisive.APIError{Status, Message, ErrorCode, Body}`; use
`errors.As`. Missing keys return `ErrMissingWriteKey`, `ErrMissingReadKey`,
`ErrMissingMasterKey` or `ErrMissingProjectID` before any request is made.

## Tests

```
go vet ./...
go test ./...
```

Unit tests use an in-process `httptest` server. The integration test runs only when
`INCISIVE_BASE_URL`, `INCISIVE_PROJECT_ID`, `INCISIVE_WRITE_KEY` and `INCISIVE_READ_KEY`
are set (and cleans up with `INCISIVE_MASTER_KEY` if present):

```
INCISIVE_BASE_URL=http://localhost:8080 INCISIVE_PROJECT_ID=... \
INCISIVE_WRITE_KEY=... INCISIVE_READ_KEY=... go test -run Integration -v ./...
```
