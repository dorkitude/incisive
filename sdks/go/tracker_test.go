package incisive

import (
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func trackedEvents(t *testing.T, m *mockServer) map[string][]map[string]any {
	t.Helper()
	var got map[string][]map[string]any
	if err := json.Unmarshal(m.last(t).Body, &got); err != nil {
		t.Fatal(err)
	}
	return got
}

func TestTrackerAddsAppTimestampAndAddons(t *testing.T) {
	m := newMock(t, 200, `{"signups":[{"success":true},{"success":true}]}`)
	tr := NewTracker(m.client(), TrackerOptions{App: map[string]any{"name": "shop"}, FlushInterval: time.Hour})
	tr.Track("signups", map[string]any{"plan": "pro", "client": map[string]any{"ip": "8.8.8.8", "user_agent": "curl/8"}})
	tr.Track("signups", map[string]any{"plan": "free", "app": map[string]any{"name": "other"}})
	if err := tr.Close(); err != nil {
		t.Fatal(err)
	}
	evs := trackedEvents(t, m)["signups"]
	if evs[0]["app"].(map[string]any)["name"] != "shop" || evs[1]["app"].(map[string]any)["name"] != "other" {
		t.Fatalf("app: %v", evs)
	}
	meta := evs[0]["incisive"].(map[string]any)
	if _, err := time.Parse(time.RFC3339, meta["timestamp"].(string)); err != nil {
		t.Fatal(err)
	}
	if n := len(meta["addons"].([]any)); n != 3 {
		t.Fatalf("want date, geo and ua add-ons, got %d", n)
	}
	if n := len(evs[1]["incisive"].(map[string]any)["addons"].([]any)); n != 1 {
		t.Fatalf("no client: only the date add-on, got %d", n)
	}
}

func TestNilTrackerIsANoOp(t *testing.T) {
	var tr *Tracker
	tr.Track("x", map[string]any{})
	h := tr.Middleware(RequestTracking{})(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.WriteHeader(204) }))
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, httptest.NewRequest("GET", "/", nil))
	if rec.Code != 204 || tr.Flush() != nil || tr.Close() != nil {
		t.Fatal("nil tracker should pass through")
	}
}

func TestMiddlewareRecordsRequests(t *testing.T) {
	m := newMock(t, 200, `{"api_requests":[{"success":true}]}`)
	tr := NewTracker(m.client(), TrackerOptions{FlushInterval: time.Hour})
	callers := make(chan struct{}, 1)
	h := tr.Middleware(RequestTracking{
		Route: func(r *http.Request) string { return "/api/v1/things/{id}" },
		Caller: func(r *http.Request) map[string]any {
			defer func() { callers <- struct{}{} }()
			return map[string]any{"key": "agent:" + strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer ")}
		},
	})(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = io.ReadAll(r.Body)
		w.WriteHeader(404)
		_, _ = io.WriteString(w, `{"success":false,"error":"Thing not found"}`)
	}))
	req := httptest.NewRequest("POST", "/api/v1/things/7?token=s3cret&q=hats", strings.NewReader(`{}`))
	req.Header.Set("Authorization", "Bearer 42")
	req.Header.Set("User-Agent", "python-requests/2.32")
	req.Header.Set("X-Forwarded-For", "203.0.113.9, 10.0.0.1")
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	if rec.Code != 404 || !strings.Contains(rec.Body.String(), "Thing not found") {
		t.Fatal("response must pass through unchanged")
	}
	<-callers
	time.Sleep(10 * time.Millisecond) // Track runs right after Caller returns
	if err := tr.Close(); err != nil {
		t.Fatal(err)
	}
	ev := trackedEvents(t, m)["api_requests"][0]
	if ev["endpoint"] != "POST /api/v1/things/{id}" {
		t.Fatalf("endpoint %v", ev["endpoint"])
	}
	rq := ev["request"].(map[string]any)
	q := rq["query"].(map[string]any)
	if q["token"] != "[redacted]" || q["q"] != "hats" || rq["api_version"] != "v1" || rq["auth"] != "bearer" || rq["path"] != "/api/v1/things/7" {
		t.Fatalf("request %v", rq)
	}
	rs := ev["response"].(map[string]any)
	if rs["status_code"] != 404.0 || rs["status_class"] != "4xx" || rs["success"] != false || rs["error"] != "Thing not found" {
		t.Fatalf("response %v", rs)
	}
	c := ev["client"].(map[string]any)
	if c["ip"] != "203.0.113.9" || c["library"] != "python-requests" {
		t.Fatalf("client %v", c)
	}
	if ev["caller"].(map[string]any)["key"] != "agent:42" {
		t.Fatalf("caller %v", ev["caller"])
	}
}

func TestBatchMaxPendingDrops(t *testing.T) {
	m := newMock(t, 200, `{}`)
	b := NewBatchClient(m.client(), 0, WithMaxPending(2))
	_ = b.AddEvent("a", 1)
	_ = b.AddEvent("a", 2)
	if err := b.AddEvent("a", 3); !errors.Is(err, ErrBatchFull) || b.Pending() != 2 {
		t.Fatalf("want ErrBatchFull with 2 pending, got %v, %d", err, b.Pending())
	}
}

func TestTrackerErrorsAreRateLimited(t *testing.T) {
	m := newMock(t, 200, `{}`)
	var errs []error
	tr := NewTracker(m.client(), TrackerOptions{FlushInterval: time.Hour, MaxPending: 1, OnError: func(err error) { errs = append(errs, err) }})
	for i := 0; i < 5; i++ {
		tr.Track("a", map[string]any{})
	}
	if len(errs) != 1 || !errors.Is(errs[0], ErrBatchFull) {
		t.Fatalf("want one ErrBatchFull report, got %v", errs)
	}
}
