package incisive

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"sync"
	"time"
	"unicode/utf8"
)

// Tracker records events from a server, the Keen way: one collection per
// kind of action, each event denormalized with everything known about who
// acted, what happened and where. It never blocks or fails the caller: events
// are buffered by a BatchClient and sent in the background, and are dropped
// when the server is unreachable or the buffer is full.
//
// Every event gets an "app" object (TrackerOptions.App), the time it was
// tracked as incisive.timestamp, and the add-ons from StandardAddons for its
// client.ip and client.user_agent.
//
// A nil *Tracker is valid and does nothing, so apps can turn tracking off
// (in tests, say) by not creating one.
type Tracker struct {
	batch *BatchClient
	app   map[string]any

	onError func(error)
	mu      sync.Mutex
	lastErr time.Time
}

// TrackerOptions configures a Tracker. Zero values get defaults.
type TrackerOptions struct {
	// App is added to every event as "app" unless the event has one,
	// e.g. {"name": "shop", "version": "1.4.2"}.
	App map[string]any
	// FlushInterval is how often buffered events are sent. Default 2s.
	FlushInterval time.Duration
	// MaxBatchSize sends early once this many events are buffered. Default 500.
	MaxBatchSize int
	// MaxPending caps buffered events; more are dropped. Default 10000.
	MaxPending int
	// OnError receives send failures and dropped events, at most once a
	// minute so an outage cannot flood logs. Default: ignore.
	OnError func(error)
}

// NewTracker starts a Tracker that sends events with client's write key.
func NewTracker(client *Client, opts TrackerOptions) *Tracker {
	if opts.FlushInterval <= 0 {
		opts.FlushInterval = 2 * time.Second
	}
	if opts.MaxBatchSize <= 0 {
		opts.MaxBatchSize = 500
	}
	if opts.MaxPending <= 0 {
		opts.MaxPending = 10000
	}
	t := &Tracker{app: opts.App, onError: opts.OnError}
	t.batch = NewBatchClient(client, opts.FlushInterval,
		WithMaxBatchSize(opts.MaxBatchSize), WithMaxPending(opts.MaxPending), WithErrorHandler(t.report))
	return t
}

func (t *Tracker) report(err error) {
	if t.onError == nil {
		return
	}
	t.mu.Lock()
	quiet := time.Since(t.lastErr) < time.Minute
	if !quiet {
		t.lastErr = time.Now()
	}
	t.mu.Unlock()
	if !quiet {
		t.onError(err)
	}
}

// Track queues an event. props is modified: "app" and the reserved
// "incisive" object are added.
func (t *Tracker) Track(collection string, props map[string]any) {
	if t == nil {
		return
	}
	if _, ok := props["app"]; !ok && t.app != nil {
		props["app"] = t.app
	}
	var ip, ua bool
	if c, ok := props["client"].(map[string]any); ok {
		ip = c["ip"] != nil && c["ip"] != ""
		ua = c["user_agent"] != nil && c["user_agent"] != ""
	}
	props["incisive"] = EventMetadata{Timestamp: Timestamp(time.Now()), Addons: StandardAddons(ip, ua)}
	if err := t.batch.AddEvent(collection, props); err != nil {
		t.report(fmt.Errorf("dropped a %s event: %w", collection, err))
	}
}

// Flush sends buffered events now.
func (t *Tracker) Flush() error {
	if t == nil {
		return nil
	}
	return t.batch.Flush()
}

// Close sends buffered events and stops the Tracker.
func (t *Tracker) Close() error {
	if t == nil {
		return nil
	}
	return t.batch.Close()
}

// StandardAddons returns the enrichment add-ons for an event: the time parts
// of incisive.timestamp as timestamp_info, and, when the event has them,
// geolocation of client.ip as ip_geo_info and the parsed client.user_agent as
// tech.
func StandardAddons(hasIP, hasUserAgent bool) []Addon {
	a := []Addon{{Name: "incisive:date_time_parser", Input: map[string]string{"date_time": "incisive.timestamp"}, Output: "timestamp_info"}}
	if hasIP {
		a = append(a, Addon{Name: "incisive:ip_to_geo", Input: map[string]string{"ip": "client.ip"}, Output: "ip_geo_info"})
	}
	if hasUserAgent {
		a = append(a, Addon{Name: "incisive:ua_parser", Input: map[string]string{"ua_string": "client.user_agent"}, Output: "tech"})
	}
	return a
}

// ---- request tracking ----

// RequestTracking configures Tracker.Middleware.
type RequestTracking struct {
	// Collection receives one event per request. Default "api_requests".
	Collection string
	// Route names the route, e.g. "/api/v1/artworks/{id}", so requests group
	// by endpoint rather than by path. It is called after the handler
	// returns, when routers like chi know the matched pattern. Default: the
	// request path.
	Route func(r *http.Request) string
	// Caller describes the account behind the request, added as "caller".
	// It runs in its own goroutine after the response is written, so slow
	// lookups don't delay responses. The request's body and context are
	// finished by then; use its headers, URL and cookies.
	Caller func(r *http.Request) map[string]any
}

// SensitiveParams are query parameters whose values are never recorded.
var SensitiveParams = map[string]bool{"token": true, "api_key": true, "apikey": true, "key": true, "password": true, "secret": true, "code": true, "claim": true, "claimtoken": true}

// Middleware returns net/http middleware that records every request: the
// "endpoint" ("POST /api/v1/messages"), the "request" (method, path, route,
// api_version, query with SensitiveParams redacted, auth scheme, content type
// and length, host, referrer), the "response" (status_code, status_class,
// success, bytes, duration_ms, and the error message of a failed response),
// the "client" (ClientFacts) and the "caller".
func (t *Tracker) Middleware(o RequestTracking) func(http.Handler) http.Handler {
	if o.Collection == "" {
		o.Collection = "api_requests"
	}
	return func(next http.Handler) http.Handler {
		if t == nil {
			return next
		}
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			started := time.Now()
			rec := &responseRecorder{ResponseWriter: w}
			next.ServeHTTP(rec, r)
			route := r.URL.Path
			if o.Route != nil {
				if s := o.Route(r); s != "" {
					route = s
				}
			}
			ev := requestEvent(r, rec, started, route)
			if o.Caller == nil {
				t.Track(o.Collection, ev)
				return
			}
			go func() {
				ev["caller"] = o.Caller(r)
				t.Track(o.Collection, ev)
			}()
		})
	}
}

// responseRecorder notes what a handler wrote, keeping the start of error bodies.
type responseRecorder struct {
	http.ResponseWriter
	status  int
	bytes   int
	errBody []byte
}

func (w *responseRecorder) WriteHeader(code int) {
	if w.status == 0 {
		w.status = code
	}
	w.ResponseWriter.WriteHeader(code)
}

func (w *responseRecorder) Write(b []byte) (int, error) {
	if w.status == 0 {
		w.status = http.StatusOK
	}
	if w.status >= 400 && len(w.errBody) < 4096 {
		w.errBody = append(w.errBody, b[:min(len(b), 4096-len(w.errBody))]...)
	}
	n, err := w.ResponseWriter.Write(b)
	w.bytes += n
	return n, err
}

func (w *responseRecorder) Flush() {
	if f, ok := w.ResponseWriter.(http.Flusher); ok {
		f.Flush()
	}
}

func (w *responseRecorder) Unwrap() http.ResponseWriter { return w.ResponseWriter }

// Truncate shortens s to at most n characters, marking the cut with "…".
func Truncate(s string, n int) string {
	if utf8.RuneCountInString(s) <= n {
		return s
	}
	return string([]rune(s)[:n]) + "…"
}

// ClientFacts describes who sent a request: "ip" (the first X-Forwarded-For
// address, else the peer), "user_agent", and "library": "browser" for
// browsers, else the first word of the user agent (e.g. "python-requests",
// "curl"), or "none".
func ClientFacts(r *http.Request) map[string]any {
	ip := ""
	if xff := r.Header.Get("X-Forwarded-For"); xff != "" {
		ip = strings.TrimSpace(strings.Split(xff, ",")[0])
	} else {
		ip = r.RemoteAddr
		if i := strings.LastIndex(ip, ":"); i != -1 {
			ip = ip[:i]
		}
		ip = strings.Trim(ip, "[]")
	}
	ua := r.Header.Get("User-Agent")
	library := "none"
	switch {
	case ua == "":
	case strings.HasPrefix(ua, "Mozilla/"):
		library = "browser"
	default:
		library = "other"
		if f := strings.FieldsFunc(ua, func(c rune) bool { return c == '/' || c == ' ' || c == ';' }); len(f) > 0 {
			library = strings.ToLower(Truncate(f[0], 40))
		}
	}
	return map[string]any{"ip": ip, "user_agent": Truncate(ua, 500), "library": library}
}

func requestEvent(r *http.Request, w *responseRecorder, started time.Time, route string) map[string]any {
	status := w.status
	if status == 0 {
		status = http.StatusOK
	}
	query := map[string]any{}
	for k, v := range r.URL.Query() {
		if SensitiveParams[strings.ToLower(k)] {
			query[k] = "[redacted]"
		} else {
			query[k] = Truncate(strings.Join(v, ","), 200)
		}
	}
	version := "unversioned"
	for _, seg := range strings.Split(route, "/") {
		if len(seg) > 1 && seg[0] == 'v' && strings.Trim(seg[1:], "0123456789") == "" {
			version = seg
			break
		}
	}
	auth := "none"
	switch h := r.Header.Get("Authorization"); {
	case strings.HasPrefix(strings.ToLower(h), "bearer "):
		auth = "bearer"
	case h != "":
		auth = "authorization"
	case r.Header.Get("X-Api-Key") != "":
		auth = "x-api-key"
	case r.URL.Query().Get("token") != "" || r.URL.Query().Get("api_key") != "":
		auth = "query"
	}
	response := map[string]any{
		"status_code":  status,
		"status_class": fmt.Sprintf("%dxx", status/100),
		"success":      status < 400,
		"bytes":        w.bytes,
		"duration_ms":  float64(time.Since(started).Microseconds()) / 1000,
	}
	if status >= 400 && len(w.errBody) > 0 {
		var body struct {
			Error   string `json:"error"`
			Message string `json:"message"`
		}
		_ = json.Unmarshal(w.errBody, &body)
		msg := body.Error
		if msg == "" {
			msg = body.Message
		}
		if msg == "" {
			msg = strings.TrimSpace(string(w.errBody))
		}
		response["error"] = Truncate(msg, 300)
	}
	return map[string]any{
		"endpoint": r.Method + " " + route,
		"request": map[string]any{
			"method":         r.Method,
			"path":           Truncate(r.URL.Path, 300),
			"route":          route,
			"api_version":    version,
			"query":          query,
			"auth":           auth,
			"content_type":   r.Header.Get("Content-Type"),
			"content_length": r.ContentLength,
			"host":           r.Host,
			"referrer":       Truncate(r.Referer(), 500),
		},
		"response": response,
		"client":   ClientFacts(r),
	}
}
