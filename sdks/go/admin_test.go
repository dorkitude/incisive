package incisive

import (
	"context"
	"encoding/json"
	"net/http"
	"testing"
)

func TestSavedQueries(t *testing.T) {
	ctx := context.Background()
	def := `{"refresh_rate":0,"query_name":"daily revenue","query":{"analysis_type":"sum","event_collection":"purchases",
		"target_property":"price","timeframe":"this_2_weeks","group_by":["platform"],"timezone":null,"interval":null,
		"filters":[],"limit":20},"metadata":{"display_name":"Rev"},"urls":{"cached_query_url":"/x"},"run_information":null}`
	m := newMock(t, 200, def)
	c := m.client()

	sq, err := c.CreateSavedQuery(ctx, "daily revenue", SavedQueryInput{
		RefreshRate: 3600,
		Query: QueryParams{AnalysisType: AnalysisSum, EventCollection: "purchases", TargetProperty: "price",
			Timeframe: RelativeTimeframe("this_2_weeks"), GroupBy: GroupByProperties("platform", "country"),
			OrderBy: OrderBy{{PropertyName: "result", Direction: Descending}}, Limit: 20},
		Metadata: map[string]interface{}{"display_name": "Rev"},
	})
	if err != nil {
		t.Fatal(err)
	}
	r := m.last(t)
	assertReq(t, r, "PUT", "/1.0/projects/proj1/queries/saved/daily%20revenue", "MKEY")
	assertJSON(t, r.Body, `{"refresh_rate":3600,"metadata":{"display_name":"Rev"},"query":{"analysis_type":"sum",
		"event_collection":"purchases","target_property":"price","timeframe":"this_2_weeks",
		"group_by":["platform","country"],"order_by":[{"property_name":"result","direction":"DESC"}],"limit":20}}`)
	if sq.QueryName != "daily revenue" || sq.Query.AnalysisType != "sum" || sq.URLs["cached_query_url"] != "/x" || len(sq.Raw) == 0 {
		t.Fatalf("decoded %+v", sq)
	}

	if _, err := c.SavedQuery(ctx, "q1"); err != nil {
		t.Fatal(err)
	}
	assertReq(t, m.last(t), "GET", "/1.0/projects/proj1/queries/saved/q1", "MKEY")

	m.response = `{"query_name":"q1","result":[{"platform":"Web","result":3}],"query":{"analysis_type":"count"}}`
	res, err := c.SavedQueryResult(ctx, "q1")
	if err != nil {
		t.Fatal(err)
	}
	assertReq(t, m.last(t), "GET", "/1.0/projects/proj1/queries/saved/q1/result", "RKEY")
	var rows []map[string]interface{}
	if err := json.Unmarshal(res.Result, &rows); err != nil || rows[0]["platform"] != "Web" {
		t.Fatalf("result %s", res.Result)
	}

	m.response = `[` + def + `]`
	list, err := c.SavedQueries(ctx)
	if err != nil || len(list) != 1 {
		t.Fatalf("list %v %v", list, err)
	}
	assertReq(t, m.last(t), "GET", "/1.0/projects/proj1/queries/saved", "MKEY")

	m.status, m.response = 204, ``
	if err := c.DeleteSavedQuery(ctx, "q1"); err != nil {
		t.Fatal(err)
	}
	assertReq(t, m.last(t), "DELETE", "/1.0/projects/proj1/queries/saved/q1", "MKEY")
}

func TestUpdateSavedQueryAttributes(t *testing.T) {
	m := newMock(t, 200, ``)
	m.handler = func(w http.ResponseWriter, r *http.Request, body []byte) bool {
		if r.Method == "GET" {
			_, _ = w.Write([]byte(`{"query_name":"q","refresh_rate":0,"metadata":{"v":1},"urls":{"a":"b"},
				"last_modified_date":"x","query":{"analysis_type":"count","event_collection":"c",
				"timeframe":"this_7_days","group_by":null,"timezone":null,"interval":null,
				"filters":[{"property_name":"a","operator":"eq","property_value":1}]}}`))
			return true
		}
		_, _ = w.Write(body)
		return true
	}
	_, err := m.client().UpdateSavedQueryAttributes(context.Background(), "q", map[string]interface{}{
		"refresh_rate": 600,
		"query":        map[string]interface{}{"timeframe": "this_30_days", "interval": "daily"},
	})
	if err != nil {
		t.Fatal(err)
	}
	reqs := m.requests()
	if len(reqs) != 2 || reqs[0].Method != "GET" || reqs[1].Method != "PUT" {
		t.Fatalf("requests %+v", reqs)
	}
	assertReq(t, reqs[1], "PUT", "/1.0/projects/proj1/queries/saved/q", "MKEY")
	assertJSON(t, reqs[1].Body, `{"query_name":"q","refresh_rate":600,"metadata":{"v":1},"query":{
		"analysis_type":"count","event_collection":"c","timeframe":"this_30_days","interval":"daily",
		"filters":[{"property_name":"a","operator":"eq","property_value":1}]}}`)
}

func TestCachedDatasets(t *testing.T) {
	ctx := context.Background()
	m := newMock(t, 200, `{"dataset_name":"ds","display_name":"D","index_by":["product.id"],"query":{"analysis_type":"count"},"status":"Created"}`)
	c := m.client()
	ds, err := c.CreateCachedDataset(ctx, "ds", CachedDatasetInput{
		DisplayName: "D",
		IndexBy:     IndexBy{"product.id"},
		Query: QueryParams{AnalysisType: AnalysisCount, EventCollection: "purchases",
			Timeframe: RelativeTimeframe("this_500_days"), Interval: "daily", GroupBy: GroupByProperties("country")},
	})
	if err != nil {
		t.Fatal(err)
	}
	r := m.last(t)
	assertReq(t, r, "PUT", "/1.0/projects/proj1/datasets/ds", "MKEY")
	assertJSON(t, r.Body, `{"display_name":"D","index_by":"product.id","query":{"analysis_type":"count",
		"event_collection":"purchases","timeframe":"this_500_days","interval":"daily","group_by":"country"}}`)
	if ds.DatasetName != "ds" || ds.IndexBy[0] != "product.id" || ds.Status != "Created" {
		t.Fatalf("decoded %+v", ds)
	}

	if _, err := c.CachedDataset(ctx, "ds"); err != nil {
		t.Fatal(err)
	}
	assertReq(t, m.last(t), "GET", "/1.0/projects/proj1/datasets/ds", "RKEY")

	m.response = `{"datasets":[{"dataset_name":"a","index_by":"x"}],"count":1,"next_page_url":"/n"}`
	list, err := c.CachedDatasets(ctx, ListOptions{Limit: 10, AfterName: "z"})
	if err != nil || len(list.Datasets) != 1 || list.NextPageURL != "/n" {
		t.Fatalf("list %+v %v", list, err)
	}
	r = m.last(t)
	assertReq(t, r, "GET", "/1.0/projects/proj1/datasets", "RKEY")
	if r.Query["limit"][0] != "10" || r.Query["after_name"][0] != "z" {
		t.Fatalf("query %v", r.Query)
	}

	m.response = `{"result":[{"timeframe":{"start":"s","end":"e"},"value":5}]}`
	res, err := c.CachedDatasetResults(ctx, "ds", DatasetResultsParams{
		IndexBy:   map[string]interface{}{"product.id": "p1", "n": 5},
		Timeframe: AbsoluteTimeframe(mustTime("2026-01-01T00:00:00Z"), mustTime("2026-02-01T00:00:00Z")),
		ZeroFill:  Bool(false),
	})
	if err != nil {
		t.Fatal(err)
	}
	r = m.last(t)
	assertReq(t, r, "GET", "/1.0/projects/proj1/datasets/ds/results", "RKEY")
	assertJSON(t, []byte(r.Query["index_by"][0]), `{"product.id":"p1","n":5}`)
	assertJSON(t, []byte(r.Query["timeframe"][0]), `{"start":"2026-01-01T00:00:00.000Z","end":"2026-02-01T00:00:00.000Z"}`)
	if r.Query["zero_fill"][0] != "false" {
		t.Fatalf("zero_fill %v", r.Query)
	}
	if iv, err := res.Intervals(); err != nil || len(iv) != 1 {
		t.Fatalf("intervals %v %v", iv, err)
	}

	if _, err := c.CachedDatasetResults(ctx, "ds", DatasetResultsParams{IndexBy: "p1", Timeframe: RelativeTimeframe("this_3_days")}); err != nil {
		t.Fatal(err)
	}
	r = m.last(t)
	if r.Query["index_by"][0] != "p1" || r.Query["timeframe"][0] != "this_3_days" {
		t.Fatalf("query %v", r.Query)
	}

	if err := c.DeleteCachedDataset(ctx, "ds"); err != nil {
		t.Fatal(err)
	}
	assertReq(t, m.last(t), "DELETE", "/1.0/projects/proj1/datasets/ds", "MKEY")
}

func TestDiscovery(t *testing.T) {
	ctx := context.Background()
	m := newMock(t, 200, `[{"name":"purchases","properties":{"price":"num"},"url":"/u"}]`)
	c := m.client()
	cols, err := c.Collections(ctx)
	if err != nil || len(cols) != 1 || cols[0].Properties["price"] != "num" {
		t.Fatalf("collections %v %v", cols, err)
	}
	assertReq(t, m.last(t), "GET", "/1.0/projects/proj1/events", "RKEY")

	m.response = `{"properties":{"item.id":"num"}}`
	col, err := c.Collection(ctx, "a b")
	if err != nil || col.Name != "a b" || col.Properties["item.id"] != "num" {
		t.Fatalf("collection %v %v", col, err)
	}
	assertReq(t, m.last(t), "GET", "/1.0/projects/proj1/events/a%20b", "RKEY")

	m.response = `{"property_name":"item.id","type":"num","url":"/p"}`
	p, err := c.Property(ctx, "a", "item.id")
	if err != nil || p.Type != "num" {
		t.Fatalf("property %v %v", p, err)
	}
	assertReq(t, m.last(t), "GET", "/1.0/projects/proj1/events/a/properties/item.id", "RKEY")
}

func TestDeletes(t *testing.T) {
	ctx := context.Background()
	m := newMock(t, 204, ``)
	c := m.client()

	err := c.DeleteEvents(ctx, "purchases", DeleteParams{
		Filters:   []Filter{{PropertyName: "user.id", Operator: "eq", PropertyValue: "u1"}},
		Timeframe: RelativeTimeframe("this_7_days"),
		Timezone:  OffsetTimezone(-28800),
	})
	if err != nil {
		t.Fatal(err)
	}
	r := m.last(t)
	assertReq(t, r, "DELETE", "/1.0/projects/proj1/events/purchases", "MKEY")
	assertJSON(t, []byte(r.Query["filters"][0]), `[{"property_name":"user.id","operator":"eq","property_value":"u1"}]`)
	if r.Query["timeframe"][0] != "this_7_days" || r.Query["timezone"][0] != "-28800" || len(r.Body) != 0 {
		t.Fatalf("query %v body %q", r.Query, r.Body)
	}

	if err := c.DeleteCollection(ctx, "old/stuff"); err != nil {
		t.Fatal(err)
	}
	r = m.last(t)
	assertReq(t, r, "DELETE", "/1.0/projects/proj1/events/old%2Fstuff", "MKEY")
	if len(r.Query) != 0 {
		t.Fatalf("query %v", r.Query)
	}

	if err := c.DeleteProperty(ctx, "c", "user.email"); err != nil {
		t.Fatal(err)
	}
	assertReq(t, m.last(t), "DELETE", "/1.0/projects/proj1/events/c/properties/user.email", "MKEY")
}

func TestAccessKeys(t *testing.T) {
	ctx := context.Background()
	key := `{"key":"K1","name":"dash","is_active":true,"permitted":["queries"],"options":{"queries":{"filters":[]}}}`
	m := newMock(t, 200, key)
	c := m.client()

	ak, err := c.CreateAccessKey(ctx, AccessKeyInput{Name: "dash", Permitted: []string{PermitQueries, PermitCachedQueries},
		Options: map[string]interface{}{"cached_queries": map[string]interface{}{"allowed": []string{"q1"}}}})
	if err != nil || ak.Key != "K1" || !ak.IsActive {
		t.Fatalf("create %+v %v", ak, err)
	}
	r := m.last(t)
	assertReq(t, r, "POST", "/1.0/projects/proj1/keys", "MKEY")
	assertJSON(t, r.Body, `{"name":"dash","is_active":true,"permitted":["queries","cached_queries"],
		"options":{"cached_queries":{"allowed":["q1"]}}}`)

	m.response = `{"keys":[` + key + `]}`
	keys, err := c.ListAccessKeys(ctx, AccessKeyListOptions{Name: "dash", Page: 1, PerPage: 10})
	if err != nil || len(keys) != 1 {
		t.Fatalf("list %v %v", keys, err)
	}
	r = m.last(t)
	assertReq(t, r, "GET", "/1.0/projects/proj1/keys", "MKEY")
	if r.Query["name"][0] != "dash" || r.Query["page"][0] != "1" || r.Query["per_page"][0] != "10" {
		t.Fatalf("query %v", r.Query)
	}

	m.response = key
	if _, err := c.GetAccessKey(ctx, "K1"); err != nil {
		t.Fatal(err)
	}
	assertReq(t, m.last(t), "GET", "/1.0/projects/proj1/keys/K1", "MKEY")

	if _, err := c.UpdateAccessKey(ctx, "K1", AccessKeyInput{Name: "n", IsActive: Bool(false)}); err != nil {
		t.Fatal(err)
	}
	r = m.last(t)
	assertReq(t, r, "POST", "/1.0/projects/proj1/keys/K1", "MKEY")
	assertJSON(t, r.Body, `{"name":"n","is_active":false,"permitted":[],"options":{}}`)

	if _, err := c.RevokeAccessKey(ctx, "K1"); err != nil {
		t.Fatal(err)
	}
	assertReq(t, m.last(t), "POST", "/1.0/projects/proj1/keys/K1/revoke", "MKEY")
	if _, err := c.UnrevokeAccessKey(ctx, "K1"); err != nil {
		t.Fatal(err)
	}
	assertReq(t, m.last(t), "POST", "/1.0/projects/proj1/keys/K1/unrevoke", "MKEY")

	m.status, m.response = 204, ``
	if err := c.DeleteAccessKey(ctx, "K1"); err != nil {
		t.Fatal(err)
	}
	assertReq(t, m.last(t), "DELETE", "/1.0/projects/proj1/keys/K1", "MKEY")
}
