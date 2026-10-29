package incisive

import (
	"context"
	"errors"
	"sync"
	"time"
)

// ErrBatchClientClosed is returned by BatchClient.AddEvent after Close.
var ErrBatchClientClosed = errors.New("incisive: batch client is closed")

// ErrBatchFull is returned by BatchClient.AddEvent when WithMaxPending events
// are already buffered; the event is dropped.
var ErrBatchFull = errors.New("incisive: batch buffer is full; event dropped")

// BatchOption configures a BatchClient.
type BatchOption func(*BatchClient)

// WithMaxBatchSize triggers an immediate background flush once n events are
// buffered. n <= 0 (the default) means only the interval and explicit
// Flush/Close send events.
func WithMaxBatchSize(n int) BatchOption {
	return func(b *BatchClient) { b.maxBatch = n }
}

// WithMaxPending caps how many events may wait in the buffer, so a long
// outage cannot grow memory without bound. Beyond it AddEvent drops the event
// and returns ErrBatchFull. n <= 0 (the default) means no cap.
func WithMaxPending(n int) BatchOption {
	return func(b *BatchClient) { b.maxPending = n }
}

// WithErrorHandler receives errors from background (interval or size
// triggered) flushes, which otherwise have nowhere to go.
func WithErrorHandler(fn func(error)) BatchOption {
	return func(b *BatchClient) { b.onError = fn }
}

// WithFlushTimeout bounds each background flush request. Default 30s.
func WithFlushTimeout(d time.Duration) BatchOption {
	return func(b *BatchClient) { b.flushTimeout = d }
}

// BatchClient buffers events in memory and sends them as one multi-collection
// request every flush interval. It embeds *Client, so every other Client
// method is available unchanged. Events still buffered when the process exits
// without Close are lost.
type BatchClient struct {
	*Client

	interval     time.Duration
	maxBatch     int
	maxPending   int
	onError      func(error)
	flushTimeout time.Duration

	mu      sync.Mutex
	buf     map[string][]interface{}
	n       int
	closed  bool
	flushMu sync.Mutex // serializes sends so batches go out in order

	stop chan struct{}
	done chan struct{}
}

// NewBatchClient wraps client and starts the periodic flusher. A non-positive
// flushInterval disables the timer (flush manually or by size).
func NewBatchClient(client *Client, flushInterval time.Duration, opts ...BatchOption) *BatchClient {
	b := &BatchClient{
		Client:       client,
		interval:     flushInterval,
		flushTimeout: 30 * time.Second,
		buf:          make(map[string][]interface{}),
		stop:         make(chan struct{}),
		done:         make(chan struct{}),
	}
	for _, o := range opts {
		o(b)
	}
	go b.loop()
	return b
}

func (b *BatchClient) loop() {
	defer close(b.done)
	if b.interval <= 0 {
		<-b.stop
		return
	}
	t := time.NewTicker(b.interval)
	defer t.Stop()
	for {
		select {
		case <-t.C:
			b.backgroundFlush()
		case <-b.stop:
			return
		}
	}
}

func (b *BatchClient) backgroundFlush() {
	ctx, cancel := context.WithTimeout(context.Background(), b.flushTimeout)
	defer cancel()
	if err := b.FlushContext(ctx); err != nil && b.onError != nil {
		b.onError(err)
	}
}

// AddEvent buffers event for collection. It never performs I/O itself.
func (b *BatchClient) AddEvent(collection string, event interface{}) error {
	b.mu.Lock()
	if b.closed {
		b.mu.Unlock()
		return ErrBatchClientClosed
	}
	if b.maxPending > 0 && b.n >= b.maxPending {
		b.mu.Unlock()
		return ErrBatchFull
	}
	b.buf[collection] = append(b.buf[collection], event)
	b.n++
	full := b.maxBatch > 0 && b.n >= b.maxBatch
	b.mu.Unlock()
	if full {
		go b.backgroundFlush()
	}
	return nil
}

// AddEventContext buffers an event; ctx is accepted for signature parity
// with Client and is not used.
func (b *BatchClient) AddEventContext(_ context.Context, collection string, event interface{}) error {
	return b.AddEvent(collection, event)
}

// Pending reports how many events are buffered.
func (b *BatchClient) Pending() int {
	b.mu.Lock()
	defer b.mu.Unlock()
	return b.n
}

// Flush sends all buffered events now and waits for the request to finish.
func (b *BatchClient) Flush() error {
	return b.FlushContext(context.Background())
}

// FlushContext is Flush with a context. If the request fails the events are
// dropped and the error returned (no automatic retry, so a poison event cannot
// wedge the buffer). Partial rejections are reported as *BatchInsertError.
func (b *BatchClient) FlushContext(ctx context.Context) error {
	b.flushMu.Lock()
	defer b.flushMu.Unlock()

	b.mu.Lock()
	if b.n == 0 {
		b.mu.Unlock()
		return nil
	}
	batch := b.buf
	b.buf = make(map[string][]interface{})
	b.n = 0
	b.mu.Unlock()

	res, err := b.Client.AddEventsContext(ctx, batch)
	if err != nil {
		return err
	}
	if res.Failed() > 0 {
		return &BatchInsertError{Result: res}
	}
	return nil
}

// Close stops the timer and flushes remaining events. Further AddEvent calls
// return ErrBatchClientClosed. Close is idempotent.
func (b *BatchClient) Close() error {
	return b.CloseContext(context.Background())
}

// CloseContext is Close with a context bounding the final flush.
func (b *BatchClient) CloseContext(ctx context.Context) error {
	b.mu.Lock()
	already := b.closed
	b.closed = true
	b.mu.Unlock()
	if !already {
		close(b.stop)
		<-b.done
	}
	return b.FlushContext(ctx)
}
