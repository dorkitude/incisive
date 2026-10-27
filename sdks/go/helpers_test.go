package incisive

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"reflect"
	"sync"
	"testing"
)

// captured is one request seen by the mock server.
type captured struct {
	Method  string
	RawPath string // escaped path as sent on the wire
	Query   map[string][]string
	Header  http.Header
	Body    []byte
}

type mockServer struct {
	*httptest.Server
	mu       sync.Mutex
	reqs     []captured
	status   int
	response string
	handler  func(w http.ResponseWriter, r *http.Request, body []byte) bool
}

func newMock(t *testing.T, status int, response string) *mockServer {
	t.Helper()
	m := &mockServer{status: status, response: response}
	m.Server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		m.mu.Lock()
		m.reqs = append(m.reqs, captured{
			Method:  r.Method,
			RawPath: r.URL.EscapedPath(),
			Query:   r.URL.Query(),
			Header:  r.Header.Clone(),
			Body:    body,
		})
		h := m.handler
		status, resp := m.status, m.response
		m.mu.Unlock()
		if h != nil && h(w, r, body) {
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(status)
		_, _ = io.WriteString(w, resp)
	}))
	t.Cleanup(m.Close)
	return m
}

func (m *mockServer) client() *Client {
	return &Client{
		ProjectID: "proj1",
		WriteKey:  "WKEY",
		ReadKey:   "RKEY",
		MasterKey: "MKEY",
		BaseURL:   m.URL,
	}
}

func (m *mockServer) requests() []captured {
	m.mu.Lock()
	defer m.mu.Unlock()
	return append([]captured(nil), m.reqs...)
}

func (m *mockServer) last(t *testing.T) captured {
	t.Helper()
	r := m.requests()
	if len(r) == 0 {
		t.Fatal("no request captured")
	}
	return r[len(r)-1]
}

// assertJSON compares JSON structurally.
func assertJSON(t *testing.T, got []byte, want string) {
	t.Helper()
	var g, w interface{}
	if err := json.Unmarshal(got, &g); err != nil {
		t.Fatalf("got invalid JSON %q: %v", got, err)
	}
	if err := json.Unmarshal([]byte(want), &w); err != nil {
		t.Fatalf("bad want JSON %q: %v", want, err)
	}
	if !reflect.DeepEqual(g, w) {
		t.Fatalf("JSON mismatch\n got: %s\nwant: %s", got, want)
	}
}

func assertReq(t *testing.T, r captured, method, path, key string) {
	t.Helper()
	if r.Method != method {
		t.Errorf("method = %s, want %s", r.Method, method)
	}
	if r.RawPath != path {
		t.Errorf("path = %s, want %s", r.RawPath, path)
	}
	if got := r.Header.Get("Authorization"); got != key {
		t.Errorf("Authorization = %q, want %q", got, key)
	}
	if ua := r.Header.Get("User-Agent"); ua != "incisive-go/"+Version {
		t.Errorf("User-Agent = %q", ua)
	}
	if len(r.Body) > 0 && r.Header.Get("Content-Type") != "application/json" {
		t.Errorf("Content-Type = %q", r.Header.Get("Content-Type"))
	}
	if len(r.Body) == 0 && r.Header.Get("Content-Type") != "" {
		t.Errorf("unexpected Content-Type %q on bodiless request", r.Header.Get("Content-Type"))
	}
}
