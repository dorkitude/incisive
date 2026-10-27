package incisive

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"
)

// EventMetadata models the reserved metadata object that every event may
// carry. Embed it in an event struct with the tag `json:"keen"`.
type EventMetadata struct {
	// Timestamp overrides the server-assigned event time. Use Timestamp()
	// to format a time.Time.
	Timestamp string `json:"timestamp,omitempty"`
	// Addons configures server-side data enrichment.
	Addons []Addon `json:"addons,omitempty"`
}

// Addon is a data-enrichment addon declaration.
type Addon struct {
	Name   string            `json:"name"`
	Input  map[string]string `json:"input"`
	Output string            `json:"output"`
}

// Timestamp formats t as ISO-8601 UTC with millisecond precision, the format
// the server expects for event timestamps and absolute timeframes.
func Timestamp(t time.Time) string {
	return t.UTC().Format("2006-01-02T15:04:05.000Z")
}

// EventResult is the per-event status returned by a multi-event insert.
type EventResult struct {
	Success bool            `json:"success"`
	Error   json.RawMessage `json:"error,omitempty"`
}

// BatchResult maps collection name to one EventResult per submitted event.
type BatchResult map[string][]EventResult

// Failed reports how many events the server rejected.
func (r BatchResult) Failed() int {
	n := 0
	for _, list := range r {
		for _, e := range list {
			if !e.Success {
				n++
			}
		}
	}
	return n
}

// AddEvent records one event in collection. event is JSON-encoded.
func (c *Client) AddEvent(collection string, event interface{}) error {
	return c.AddEventContext(context.Background(), collection, event)
}

// AddEventContext is AddEvent with a context.
func (c *Client) AddEventContext(ctx context.Context, collection string, event interface{}) error {
	return c.do(ctx, http.MethodPost, writeKey, c.projectURL("events", collection), nil, event, nil)
}

// AddEvents records events in several collections with a single request.
// Like the original tracking-only client, it returns an error only when the
// request itself fails; use AddEventsContext to inspect per-event results.
func (c *Client) AddEvents(events map[string][]interface{}) error {
	_, err := c.AddEventsContext(context.Background(), events)
	return err
}

// AddEventsContext records a multi-collection batch and returns the server's
// per-event results.
func (c *Client) AddEventsContext(ctx context.Context, events map[string][]interface{}) (BatchResult, error) {
	var out BatchResult
	if err := c.do(ctx, http.MethodPost, writeKey, c.projectURL("events"), nil, events, &out); err != nil {
		return nil, err
	}
	return out, nil
}

// BatchInsertError can be returned by BatchClient flushes when the server
// accepted the request but rejected some events.
type BatchInsertError struct {
	Result BatchResult
}

func (e *BatchInsertError) Error() string {
	return fmt.Sprintf("incisive: %d event(s) rejected in batch insert", e.Result.Failed())
}
