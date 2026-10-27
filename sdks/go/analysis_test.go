package incisive

import (
	"context"
	"encoding/json"
	"testing"
	"time"
)

func TestCountFullParams(t *testing.T) {
	m := newMock(t, 200, `{"result":42,"execution_metadata":{"total_processing_time":0.5,"events_scanned":100,"properties_per_event":2,"total_properties_scanned":200}}`)
	res, err := m.client().Count(context.Background(), "page views", QueryParams{
		Timeframe: RelativeTimeframe("this_14_days"),
		Timezone:  NamedTimezone("US/Pacific"),
		Filters: []Filter{
			{PropertyName: "price", Operator: "gte", PropertyValue: 0.99},
			{PropertyName: "on_sale", Operator: "eq", PropertyValue: false},
			Or(
				Filter{PropertyName: "tier", Operator: "eq", PropertyValue: "premium"},
				Filter{PropertyName: "n", Operator: "exists", PropertyValue: true},
			),
		},
		GroupBy:         GroupByProperties("user.email"),
		Interval:        "daily",
		OrderBy:         OrderBy{{PropertyName: "result", Direction: Descending}},
		Limit:           5,
		IncludeMetadata: true,
	})
	if err != nil {
		t.Fatal(err)
	}
	r := m.last(t)
	assertReq(t, r, "POST", "/3.0/projects/proj1/queries/count", "RKEY")
	if got := r.Query["include_metadata"]; len(got) != 1 || got[0] != "true" {
		t.Errorf("include_metadata = %v", got)
	}
	assertJSON(t, r.Body, `{
		"event_collection":"page views",
		"timeframe":"this_14_days",
		"timezone":"US/Pacific",
		"filters":[
			{"property_name":"price","operator":"gte","property_value":0.99},
			{"property_name":"on_sale","operator":"eq","property_value":false},
			{"operator":"or","operands":[
				{"property_name":"tier","operator":"eq","property_value":"premium"},
				{"property_name":"n","operator":"exists","property_value":true}
			]}
		],
		"group_by":"user.email",
		"interval":"daily",
		"order_by":[{"property_name":"result","direction":"DESC"}],
		"limit":5
	}`)
	n, err := res.Int64()
	if err != nil || n != 42 {
		t.Fatalf("Int64 = %d, %v", n, err)
	}
	md, err := res.ExecutionMetadata()
	if err != nil || md == nil || md.EventsScanned != 100 || md.TotalProcessingTime != 0.5 {
		t.Fatalf("metadata %+v %v", md, err)
	}
}

func TestAbsoluteTimeframeOffsetTimezoneAndComplexGroupBy(t *testing.T) {
	m := newMock(t, 200, `{"result":[]}`)
	start := time.Date(2012, 8, 13, 19, 0, 0, 0, time.UTC)
	end := time.Date(2013, 9, 20, 19, 0, 0, 0, time.UTC)
	_, err := m.client().Sum(context.Background(), "purchases", "price", QueryParams{
		Timeframe: AbsoluteTimeframe(start, end),
		Timezone:  OffsetTimezone(-28800),
		GroupBy: GroupBy{
			{PropertyName: "platform"},
			{PropertyName: "temperature.celsius", Buckets: &Buckets{Size: 10, Offset: Float(5)}, Alias: "temperature"},
			{PropertyName: "item.price", Alias: "price_ranges", AllowRangeOverlap: true, Ranges: []Range{
				{To: Float(1), ToClosed: Bool(true)},
				{From: Float(2), To: Float(20), FromClosed: Bool(false)},
				{From: Float(100), Key: "very_expensive"},
			}},
		},
		Extra: map[string]interface{}{"future_param": []int{1, 2}},
	})
	if err != nil {
		t.Fatal(err)
	}
	r := m.last(t)
	assertReq(t, r, "POST", "/3.0/projects/proj1/queries/sum", "RKEY")
	if len(r.Query) != 0 {
		t.Errorf("unexpected query %v", r.Query)
	}
	assertJSON(t, r.Body, `{
		"event_collection":"purchases",
		"target_property":"price",
		"timeframe":{"start":"2012-08-13T19:00:00.000Z","end":"2013-09-20T19:00:00.000Z"},
		"timezone":-28800,
		"group_by":[
			"platform",
			{"property_name":"temperature.celsius","buckets":{"size":10,"offset":5},"alias":"temperature"},
			{"property_name":"item.price","alias":"price_ranges","allow_range_overlap":true,"ranges":[
				{"to":1,"to_closed":true},
				{"from":2,"to":20,"from_closed":false},
				{"from":100,"key":"very_expensive"}
			]}
		],
		"future_param":[1,2]
	}`)
}

func TestEveryAnalysisRoute(t *testing.T) {
	m := newMock(t, 200, `{"result":1.5}`)
	c := m.client()
	ctx := context.Background()
	tf := QueryParams{Timeframe: RelativeTimeframe("this_7_days")}
	cases := []struct {
		name string
		run  func() (*Result, error)
		body string
	}{
		{"count_unique", func() (*Result, error) { return c.CountUnique(ctx, "c", "user.id", tf) },
			`{"event_collection":"c","target_property":"user.id","timeframe":"this_7_days"}`},
		{"sum", func() (*Result, error) { return c.Sum(ctx, "c", "p", tf) },
			`{"event_collection":"c","target_property":"p","timeframe":"this_7_days"}`},
		{"average", func() (*Result, error) { return c.Average(ctx, "c", "p", tf) },
			`{"event_collection":"c","target_property":"p","timeframe":"this_7_days"}`},
		{"minimum", func() (*Result, error) { return c.Minimum(ctx, "c", "p", tf) },
			`{"event_collection":"c","target_property":"p","timeframe":"this_7_days"}`},
		{"maximum", func() (*Result, error) { return c.Maximum(ctx, "c", "p", tf) },
			`{"event_collection":"c","target_property":"p","timeframe":"this_7_days"}`},
		{"median", func() (*Result, error) { return c.Median(ctx, "c", "p", tf) },
			`{"event_collection":"c","target_property":"p","timeframe":"this_7_days"}`},
		{"percentile", func() (*Result, error) { return c.Percentile(ctx, "c", "p", 99.99, tf) },
			`{"event_collection":"c","target_property":"p","percentile":99.99,"timeframe":"this_7_days"}`},
		{"select_unique", func() (*Result, error) { return c.SelectUnique(ctx, "c", "p", tf) },
			`{"event_collection":"c","target_property":"p","timeframe":"this_7_days"}`},
		{"standard_deviation", func() (*Result, error) { return c.StandardDeviation(ctx, "c", "p", tf) },
			`{"event_collection":"c","target_property":"p","timeframe":"this_7_days"}`},
		{"extraction", func() (*Result, error) {
			return c.Extraction(ctx, "c", QueryParams{Timeframe: RelativeTimeframe("this_7_days"),
				PropertyNames: []string{"a", "b.c"}, Latest: 10, Email: "ops@example.com",
				ContentType: "application/json", ContentEncoding: "gzip"})
		}, `{"event_collection":"c","timeframe":"this_7_days","property_names":["a","b.c"],"latest":10,
			"email":"ops@example.com","content_type":"application/json","content_encoding":"gzip"}`},
		{"multi_analysis", func() (*Result, error) {
			return c.MultiAnalysis(ctx, "c", map[string]Analysis{
				"unique users": {AnalysisType: AnalysisCountUnique, TargetProperty: "user.id"},
				"total":        {AnalysisType: AnalysisCount},
				"p50":          {AnalysisType: AnalysisPercentile, TargetProperty: "x", Percentile: Float(50)},
			}, tf)
		}, `{"event_collection":"c","timeframe":"this_7_days","analyses":{
			"unique users":{"analysis_type":"count_unique","target_property":"user.id"},
			"total":{"analysis_type":"count"},
			"p50":{"analysis_type":"percentile","target_property":"x","percentile":50}}}`},
		{"funnel", func() (*Result, error) {
			return c.Funnel(ctx, []FunnelStep{
				{EventCollection: "signed up", ActorProperty: "visitor.guid", Timeframe: RelativeTimeframe("this_7_days")},
				{EventCollection: "completed profile", ActorProperty: "user.guid", Optional: true,
					Filters: []Filter{{PropertyName: "x", Operator: "eq", PropertyValue: nil}}},
				{EventCollection: "referred user", ActorProperty: "user.guid", Inverted: true, WithActors: true,
					Timezone: NamedTimezone("UTC")},
			}, QueryParams{Timeframe: RelativeTimeframe("this_30_days")})
		}, `{"timeframe":"this_30_days","steps":[
			{"event_collection":"signed up","actor_property":"visitor.guid","timeframe":"this_7_days"},
			{"event_collection":"completed profile","actor_property":"user.guid","optional":true,
				"filters":[{"property_name":"x","operator":"eq","property_value":null}]},
			{"event_collection":"referred user","actor_property":"user.guid","inverted":true,"with_actors":true,"timezone":"UTC"}]}`},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			res, err := tc.run()
			if err != nil {
				t.Fatal(err)
			}
			r := m.last(t)
			assertReq(t, r, "POST", "/3.0/projects/proj1/queries/"+tc.name, "RKEY")
			assertJSON(t, r.Body, tc.body)
			f, err := res.Float64()
			if err != nil || f != 1.5 {
				t.Fatalf("Float64 = %v %v", f, err)
			}
		})
	}
}

func TestResultShapes(t *testing.T) {
	m := newMock(t, 200, `{"result":[{"timeframe":{"start":"2026-01-01T00:00:00.000Z","end":"2026-01-02T00:00:00.000Z"},"value":[{"platform":"Web","result":3}]}]}`)
	res, err := m.client().Count(context.Background(), "c", QueryParams{Interval: "daily"})
	if err != nil {
		t.Fatal(err)
	}
	iv, err := res.Intervals()
	if err != nil || len(iv) != 1 || iv[0].Timeframe.Start != "2026-01-01T00:00:00.000Z" {
		t.Fatalf("intervals %+v %v", iv, err)
	}
	var groups []map[string]interface{}
	if err := json.Unmarshal(iv[0].Value, &groups); err != nil || groups[0]["platform"] != "Web" {
		t.Fatalf("groups %v %v", groups, err)
	}

	m2 := newMock(t, 200, `{"result":[3,1,0],"actors":[["a","b","c"],["a"],null],"steps":[]}`)
	fres, err := m2.client().Funnel(context.Background(), nil, QueryParams{})
	if err != nil {
		t.Fatal(err)
	}
	var counts []int
	var actors [][]string
	if err := fres.Decode(&counts); err != nil || len(counts) != 3 {
		t.Fatalf("counts %v %v", counts, err)
	}
	if err := fres.Field("actors", &actors); err != nil || len(actors[0]) != 3 || actors[2] != nil {
		t.Fatalf("actors %v %v", actors, err)
	}
	if md, err := fres.ExecutionMetadata(); md != nil || err != nil {
		t.Fatalf("expected no metadata")
	}

	m3 := newMock(t, 200, `{"result":null}`)
	nres, _ := m3.client().Average(context.Background(), "c", "p", QueryParams{})
	if f, err := nres.Float64(); f != 0 || err != nil {
		t.Fatalf("null result %v %v", f, err)
	}
}

func TestQueryParamsRoundTrip(t *testing.T) {
	src := `{"analysis_type":"sum","event_collection":"purchases","target_property":"price",
		"filters":[{"operator":"gte","property_name":"price","property_value":1}],
		"group_by":["platform"],"order_by":[{"direction":"DESC","property_name":"result"}],
		"timeframe":{"start":"a","end":"b"},"timezone":null,"interval":null,"limit":20}`
	var q QueryParams
	if err := json.Unmarshal([]byte(src), &q); err != nil {
		t.Fatal(err)
	}
	if q.AnalysisType != "sum" || q.Timeframe.Start != "a" || q.Timezone != nil || len(q.GroupBy) != 1 ||
		q.OrderBy[0].Direction != "DESC" || q.Filters[0].PropertyValue != 1.0 || q.Limit != 20 {
		t.Fatalf("decoded %+v", q)
	}
	for _, s := range []string{`"result"`, `{"property_name":"result"}`, `["result",{"property_name":"x","direction":"ASC"}]`} {
		var o OrderBy
		if err := json.Unmarshal([]byte(s), &o); err != nil || o[0].PropertyName != "result" {
			t.Fatalf("order_by %s: %v %v", s, o, err)
		}
	}
	var z Timezone
	if err := json.Unmarshal([]byte(`-3600`), &z); err != nil || z.OffsetSeconds != -3600 {
		t.Fatal("timezone offset decode")
	}
	var g GroupBy
	if err := json.Unmarshal([]byte(`"a"`), &g); err != nil || g[0].PropertyName != "a" {
		t.Fatal("group_by string decode")
	}
}
