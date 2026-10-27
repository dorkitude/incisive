package incisive

import (
	"context"
	"errors"
	"net/http"
	"testing"
	"time"
)

func TestAddEventWireFormat(t *testing.T) {
	m := newMock(t, 201, `{"created":true}`)
	c := m.client()
	type ev struct {
		Price float64       `json:"price"`
		Meta  EventMetadata `json:"keen"`
	}
	ts := time.Date(2026, 1, 2, 3, 4, 5, 6e6, time.FixedZone("x", 3600))
	if err := c.AddEvent("my purchases/eu?x#y", ev{Price: 7.13, Meta: EventMetadata{Timestamp: Timestamp(ts)}}); err != nil {
		t.Fatal(err)
	}
	r := m.last(t)
	assertReq(t, r, "POST", "/3.0/projects/proj1/events/my%20purchases%2Feu%3Fx%23y", "WKEY")
	assertJSON(t, r.Body, `{"price":7.13,"keen":{"timestamp":"2026-01-02T02:04:05.006Z"}}`)
}

func TestAddEventsReturnsPerEventResults(t *testing.T) {
	m := newMock(t, 200, `{"a":[{"success":true}],"b":[{"success":true},{"success":false,"error":{"name":"X","description":"bad"}}]}`)
	c := m.client()
	events := map[string][]interface{}{
		"a": {map[string]int{"n": 1}},
		"b": {map[string]int{"n": 2}, map[string]int{"n": 3}},
	}
	res, err := c.AddEventsContext(context.Background(), events)
	if err != nil {
		t.Fatal(err)
	}
	if res.Failed() != 1 || len(res["b"]) != 2 {
		t.Fatalf("unexpected result %+v", res)
	}
	r := m.last(t)
	assertReq(t, r, "POST", "/3.0/projects/proj1/events", "WKEY")
	assertJSON(t, r.Body, `{"a":[{"n":1}],"b":[{"n":2},{"n":3}]}`)

	// Upstream-compatible signature: only transport errors surface.
	if err := c.AddEvents(events); err != nil {
		t.Fatal(err)
	}
}

func TestBaseURLAndVersion(t *testing.T) {
	m := newMock(t, 201, `{}`)
	c := m.client()
	c.BaseURL = m.URL + "/"
	c.APIVersion = "3.1"
	if err := c.AddEvent("x", map[string]int{}); err != nil {
		t.Fatal(err)
	}
	if p := m.last(t).RawPath; p != "/3.1/projects/proj1/events/x" {
		t.Fatalf("path %s", p)
	}
}

func TestCustomHTTPClientAndUserAgent(t *testing.T) {
	m := newMock(t, 201, `{}`)
	c := m.client()
	used := false
	c.HTTPClient = &http.Client{Transport: roundTripFunc(func(r *http.Request) (*http.Response, error) {
		used = true
		return http.DefaultTransport.RoundTrip(r)
	})}
	c.UserAgent = "my-app/1"
	if err := c.AddEvent("x", 1); err != nil {
		t.Fatal(err)
	}
	if !used || m.last(t).Header.Get("User-Agent") != "my-app/1" {
		t.Fatal("custom client/user agent not used")
	}
}

type roundTripFunc func(*http.Request) (*http.Response, error)

func (f roundTripFunc) RoundTrip(r *http.Request) (*http.Response, error) { return f(r) }

func TestAPIErrorJSON(t *testing.T) {
	m := newMock(t, 400, `{"message":"You must specify a timeframe.","error_code":"TimeframeMissingError"}`)
	_, err := m.client().Count(context.Background(), "x", QueryParams{})
	var apiErr *APIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("want *APIError, got %T %v", err, err)
	}
	if apiErr.Status != 400 || apiErr.ErrorCode != "TimeframeMissingError" || apiErr.Message != "You must specify a timeframe." {
		t.Fatalf("bad error %+v", apiErr)
	}
}

func TestAPIErrorNumericCode(t *testing.T) {
	m := newMock(t, 401, `{"message":"bad key","error_code":401}`)
	err := m.client().AddEvent("x", 1)
	var apiErr *APIError
	if !errors.As(err, &apiErr) || apiErr.ErrorCode != "401" || apiErr.Status != 401 {
		t.Fatalf("bad error %#v", err)
	}
}

func TestAPIErrorNonJSON(t *testing.T) {
	m := newMock(t, 502, `<html>bad gateway</html>`)
	err := m.client().AddEvent("x", 1)
	var apiErr *APIError
	if !errors.As(err, &apiErr) {
		t.Fatalf("want *APIError, got %v", err)
	}
	if apiErr.Status != 502 || apiErr.ErrorCode != "502" || apiErr.Message == "" || string(apiErr.Body) != `<html>bad gateway</html>` {
		t.Fatalf("bad error %+v", apiErr)
	}
}

func TestMissingKeys(t *testing.T) {
	m := newMock(t, 200, `{}`)
	ctx := context.Background()
	c := &Client{ProjectID: "p", BaseURL: m.URL}
	if err := c.AddEvent("x", 1); !errors.Is(err, ErrMissingWriteKey) {
		t.Errorf("got %v", err)
	}
	if _, err := c.Count(ctx, "x", QueryParams{}); !errors.Is(err, ErrMissingReadKey) {
		t.Errorf("got %v", err)
	}
	if err := c.DeleteCollection(ctx, "x"); !errors.Is(err, ErrMissingMasterKey) {
		t.Errorf("got %v", err)
	}
	if err := (&Client{WriteKey: "w"}).AddEvent("x", 1); !errors.Is(err, ErrMissingProjectID) {
		t.Errorf("got %v", err)
	}
	if len(m.requests()) != 0 {
		t.Fatal("requests sent despite missing keys")
	}
}

func TestReadKeyFallsBackToMaster(t *testing.T) {
	m := newMock(t, 200, `{"result":1}`)
	c := m.client()
	c.ReadKey = ""
	if _, err := c.Count(context.Background(), "x", QueryParams{}); err != nil {
		t.Fatal(err)
	}
	if k := m.last(t).Header.Get("Authorization"); k != "MKEY" {
		t.Fatalf("Authorization %q", k)
	}
}

func TestContextCancellation(t *testing.T) {
	m := newMock(t, 200, `{}`)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := m.client().AddEventContext(ctx, "x", 1); !errors.Is(err, context.Canceled) {
		t.Fatalf("got %v", err)
	}
}

func TestNewClientFromEnv(t *testing.T) {
	t.Setenv("INCISIVE_PROJECT_ID", "")
	if _, err := NewClientFromEnv(); !errors.Is(err, ErrMissingProjectID) {
		t.Fatalf("got %v", err)
	}
	t.Setenv("INCISIVE_PROJECT_ID", "p")
	t.Setenv("INCISIVE_WRITE_KEY", "w")
	t.Setenv("INCISIVE_READ_KEY", "r")
	t.Setenv("INCISIVE_MASTER_KEY", "m")
	t.Setenv("INCISIVE_BASE_URL", "http://example.test")
	c, err := NewClientFromEnv()
	if err != nil {
		t.Fatal(err)
	}
	if c.ProjectID != "p" || c.WriteKey != "w" || c.ReadKey != "r" || c.MasterKey != "m" || c.BaseURL != "http://example.test" {
		t.Fatalf("bad client %+v", c)
	}
	if got := (&Client{ProjectID: "p"}).projectURL("events"); got != "http://localhost:8080/3.0/projects/p/events" {
		t.Fatalf("default url %s", got)
	}
}
