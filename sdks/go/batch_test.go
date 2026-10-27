package incisive

import (
	"errors"
	"sync/atomic"
	"testing"
	"time"
)

func mustTime(s string) time.Time {
	t, err := time.Parse(time.RFC3339, s)
	if err != nil {
		panic(err)
	}
	return t
}

func TestBatchClientFlushAndClose(t *testing.T) {
	m := newMock(t, 200, `{"a":[{"success":true},{"success":true}],"b":[{"success":true}]}`)
	b := NewBatchClient(m.client(), 0)
	_ = b.AddEvent("a", map[string]int{"n": 1})
	_ = b.AddEvent("b", map[string]int{"n": 2})
	_ = b.AddEvent("a", map[string]int{"n": 3})
	if b.Pending() != 3 || len(m.requests()) != 0 {
		t.Fatal("events should be buffered, not sent")
	}
	if err := b.Flush(); err != nil {
		t.Fatal(err)
	}
	r := m.last(t)
	assertReq(t, r, "POST", "/1.0/projects/proj1/events", "WKEY")
	assertJSON(t, r.Body, `{"a":[{"n":1},{"n":3}],"b":[{"n":2}]}`)
	if b.Pending() != 0 {
		t.Fatal("buffer not cleared")
	}
	if err := b.Flush(); err != nil || len(m.requests()) != 1 {
		t.Fatal("empty flush should not send")
	}

	_ = b.AddEvent("a", 1)
	if err := b.Close(); err != nil {
		t.Fatal(err)
	}
	if len(m.requests()) != 2 {
		t.Fatal("Close should flush")
	}
	if err := b.AddEvent("a", 1); !errors.Is(err, ErrBatchClientClosed) {
		t.Fatalf("got %v", err)
	}
	if err := b.Close(); err != nil {
		t.Fatal("Close should be idempotent")
	}
}

func TestBatchClientInterval(t *testing.T) {
	m := newMock(t, 200, `{"a":[{"success":true}]}`)
	b := NewBatchClient(m.client(), 20*time.Millisecond)
	defer b.Close()
	_ = b.AddEvent("a", 1)
	deadline := time.Now().Add(2 * time.Second)
	for len(m.requests()) == 0 && time.Now().Before(deadline) {
		time.Sleep(5 * time.Millisecond)
	}
	if len(m.requests()) != 1 {
		t.Fatal("interval flush did not happen")
	}
}

func TestBatchClientMaxSizeAndErrorHandler(t *testing.T) {
	m := newMock(t, 200, `{"a":[{"success":true},{"success":false}]}`)
	var errs atomic.Int32
	b := NewBatchClient(m.client(), 0, WithMaxBatchSize(2), WithErrorHandler(func(err error) {
		var bie *BatchInsertError
		if errors.As(err, &bie) && bie.Result.Failed() == 1 {
			errs.Add(1)
		}
	}))
	defer b.Close()
	_ = b.AddEvent("a", 1)
	_ = b.AddEvent("a", 2)
	deadline := time.Now().Add(2 * time.Second)
	for errs.Load() == 0 && time.Now().Before(deadline) {
		time.Sleep(5 * time.Millisecond)
	}
	if len(m.requests()) != 1 || errs.Load() != 1 {
		t.Fatalf("requests=%d errs=%d", len(m.requests()), errs.Load())
	}
}

func TestBatchClientFlushErrorAndEmbeddedClient(t *testing.T) {
	m := newMock(t, 500, `{"message":"boom","error_code":"InternalServerError"}`)
	b := NewBatchClient(m.client(), 0)
	defer b.Close()
	_ = b.AddEvent("a", 1)
	var apiErr *APIError
	if err := b.Flush(); !errors.As(err, &apiErr) || apiErr.Status != 500 {
		t.Fatalf("got %v", err)
	}
	if b.Pending() != 0 {
		t.Fatal("failed batch should be dropped")
	}
	// Embedded *Client methods pass straight through.
	if err := b.AddEvents(map[string][]interface{}{"a": {1}}); !errors.As(err, &apiErr) {
		t.Fatalf("got %v", err)
	}
}
