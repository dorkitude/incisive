package incisive

import (
	"context"
	"fmt"
	"os"
	"testing"
	"time"
)

// Integration tests run against a real server only when INCISIVE_BASE_URL and
// INCISIVE_PROJECT_ID (plus the relevant keys) are set.
func integrationClient(t *testing.T, needs ...string) *Client {
	t.Helper()
	for _, v := range append([]string{"INCISIVE_BASE_URL", "INCISIVE_PROJECT_ID"}, needs...) {
		if os.Getenv(v) == "" {
			t.Skipf("%s not set; skipping integration test", v)
		}
	}
	c, err := NewClientFromEnv()
	if err != nil {
		t.Fatal(err)
	}
	return c
}

func TestIntegrationAddEventAndCount(t *testing.T) {
	c := integrationClient(t, "INCISIVE_WRITE_KEY", "INCISIVE_READ_KEY")
	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()

	collection := fmt.Sprintf("go_sdk_it_%d", time.Now().UnixNano())
	if err := c.AddEventContext(ctx, collection, map[string]interface{}{"n": 1, "tag": "a"}); err != nil {
		t.Fatalf("AddEvent: %v", err)
	}
	res, err := c.AddEventsContext(ctx, map[string][]interface{}{
		collection: {map[string]interface{}{"n": 2, "tag": "b"}, map[string]interface{}{"n": 3, "tag": "b"}},
	})
	if err != nil || res.Failed() != 0 {
		t.Fatalf("AddEvents: %v %+v", err, res)
	}

	params := QueryParams{Timeframe: RelativeTimeframe("this_1_days")}
	var count int64
	deadline := time.Now().Add(30 * time.Second)
	for time.Now().Before(deadline) {
		r, err := c.Count(ctx, collection, params)
		if err != nil {
			t.Fatalf("Count: %v", err)
		}
		if count, _ = r.Int64(); count >= 3 {
			break
		}
		time.Sleep(time.Second)
	}
	if count != 3 {
		t.Fatalf("count = %d, want 3", count)
	}

	sum, err := c.Sum(ctx, collection, "n", params)
	if err != nil {
		t.Fatalf("Sum: %v", err)
	}
	if f, _ := sum.Float64(); f != 6 {
		t.Fatalf("sum = %v, want 6", f)
	}

	grouped, err := c.Count(ctx, collection, QueryParams{
		Timeframe: RelativeTimeframe("this_1_days"),
		Filters:   []Filter{{PropertyName: "tag", Operator: "eq", PropertyValue: "b"}},
	})
	if err != nil {
		t.Fatalf("filtered Count: %v", err)
	}
	if n, _ := grouped.Int64(); n != 2 {
		t.Fatalf("filtered count = %d, want 2", n)
	}

	if os.Getenv("INCISIVE_MASTER_KEY") != "" {
		if err := c.DeleteCollection(ctx, collection); err != nil {
			t.Logf("cleanup DeleteCollection: %v", err)
		}
	}
}
