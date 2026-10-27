package incisive

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
)

// Result is an analysis response. Value is the unwrapped "result" member;
// Envelope holds every top-level member (result, execution_metadata, and for
// funnels actors/steps).
type Result struct {
	Value    json.RawMessage
	Envelope map[string]json.RawMessage
}

// UnmarshalJSON implements json.Unmarshaler.
func (r *Result) UnmarshalJSON(b []byte) error {
	env := map[string]json.RawMessage{}
	if err := json.Unmarshal(b, &env); err != nil {
		return err
	}
	r.Envelope = env
	r.Value = env["result"]
	return nil
}

// Decode unmarshals the result value into v.
func (r *Result) Decode(v interface{}) error {
	if len(r.Value) == 0 {
		return fmt.Errorf("incisive: response has no result")
	}
	return json.Unmarshal(r.Value, v)
}

// Field unmarshals a top-level response member (e.g. "actors") into v.
func (r *Result) Field(name string, v interface{}) error {
	raw, ok := r.Envelope[name]
	if !ok {
		return fmt.Errorf("incisive: response has no %q member", name)
	}
	return json.Unmarshal(raw, v)
}

// Float64 decodes a scalar numeric result. A null result yields 0.
func (r *Result) Float64() (float64, error) {
	var f *float64
	if err := r.Decode(&f); err != nil {
		return 0, err
	}
	if f == nil {
		return 0, nil
	}
	return *f, nil
}

// Int64 decodes a scalar integer result such as a count.
func (r *Result) Int64() (int64, error) {
	var n json.Number
	if err := r.Decode(&n); err != nil {
		return 0, err
	}
	if n == "" {
		return 0, nil
	}
	if i, err := n.Int64(); err == nil {
		return i, nil
	}
	f, err := n.Float64()
	return int64(f), err
}

// IntervalResult is one element of an interval (time series) result.
type IntervalResult struct {
	Timeframe Timeframe       `json:"timeframe"`
	Value     json.RawMessage `json:"value"`
}

// Intervals decodes a result produced with Interval set.
func (r *Result) Intervals() ([]IntervalResult, error) {
	var out []IntervalResult
	err := r.Decode(&out)
	return out, err
}

// ExecutionMetadata is returned when QueryParams.IncludeMetadata is set.
type ExecutionMetadata struct {
	TotalProcessingTime    float64 `json:"total_processing_time"`
	EventsScanned          int64   `json:"events_scanned"`
	PropertiesPerEvent     int64   `json:"properties_per_event"`
	TotalPropertiesScanned int64   `json:"total_properties_scanned"`
}

// ExecutionMetadata returns the execution_metadata member, or nil if absent.
func (r *Result) ExecutionMetadata() (*ExecutionMetadata, error) {
	if _, ok := r.Envelope["execution_metadata"]; !ok {
		return nil, nil
	}
	var m ExecutionMetadata
	if err := r.Field("execution_metadata", &m); err != nil {
		return nil, err
	}
	return &m, nil
}

// Query runs any analysis type by POSTing params as JSON to
// /queries/{analysisType} with the read key.
func (c *Client) Query(ctx context.Context, analysisType string, params QueryParams) (*Result, error) {
	params.AnalysisType = ""
	var q url.Values
	if params.IncludeMetadata {
		q = url.Values{"include_metadata": {"true"}}
	}
	var out Result
	if err := c.do(ctx, http.MethodPost, readKey, c.projectURL("queries", analysisType), q, params, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

func (c *Client) targeted(ctx context.Context, analysis, collection, target string, p QueryParams) (*Result, error) {
	p.EventCollection = collection
	p.TargetProperty = target
	return c.Query(ctx, analysis, p)
}

// Count returns the number of events in collection.
func (c *Client) Count(ctx context.Context, collection string, params QueryParams) (*Result, error) {
	params.EventCollection = collection
	return c.Query(ctx, AnalysisCount, params)
}

// CountUnique returns the number of distinct values of targetProperty.
func (c *Client) CountUnique(ctx context.Context, collection, targetProperty string, params QueryParams) (*Result, error) {
	return c.targeted(ctx, AnalysisCountUnique, collection, targetProperty, params)
}

// Sum returns the sum of targetProperty.
func (c *Client) Sum(ctx context.Context, collection, targetProperty string, params QueryParams) (*Result, error) {
	return c.targeted(ctx, AnalysisSum, collection, targetProperty, params)
}

// Average returns the mean of targetProperty.
func (c *Client) Average(ctx context.Context, collection, targetProperty string, params QueryParams) (*Result, error) {
	return c.targeted(ctx, AnalysisAverage, collection, targetProperty, params)
}

// Minimum returns the smallest value of targetProperty.
func (c *Client) Minimum(ctx context.Context, collection, targetProperty string, params QueryParams) (*Result, error) {
	return c.targeted(ctx, AnalysisMinimum, collection, targetProperty, params)
}

// Maximum returns the largest value of targetProperty.
func (c *Client) Maximum(ctx context.Context, collection, targetProperty string, params QueryParams) (*Result, error) {
	return c.targeted(ctx, AnalysisMaximum, collection, targetProperty, params)
}

// Median returns the median of targetProperty.
func (c *Client) Median(ctx context.Context, collection, targetProperty string, params QueryParams) (*Result, error) {
	return c.targeted(ctx, AnalysisMedian, collection, targetProperty, params)
}

// Percentile returns the given percentile (0-100) of targetProperty.
func (c *Client) Percentile(ctx context.Context, collection, targetProperty string, percentile float64, params QueryParams) (*Result, error) {
	params.Percentile = &percentile
	return c.targeted(ctx, AnalysisPercentile, collection, targetProperty, params)
}

// SelectUnique returns the distinct values of targetProperty.
func (c *Client) SelectUnique(ctx context.Context, collection, targetProperty string, params QueryParams) (*Result, error) {
	return c.targeted(ctx, AnalysisSelectUnique, collection, targetProperty, params)
}

// StandardDeviation returns the standard deviation of targetProperty.
func (c *Client) StandardDeviation(ctx context.Context, collection, targetProperty string, params QueryParams) (*Result, error) {
	return c.targeted(ctx, AnalysisStandardDeviation, collection, targetProperty, params)
}

// Extraction returns raw events (or, with params.Email set, schedules an
// emailed extraction and returns a status string).
func (c *Client) Extraction(ctx context.Context, collection string, params QueryParams) (*Result, error) {
	params.EventCollection = collection
	return c.Query(ctx, AnalysisExtraction, params)
}

// Funnel runs a funnel over steps. params may carry a shared Timeframe and
// Timezone that steps inherit.
func (c *Client) Funnel(ctx context.Context, steps []FunnelStep, params QueryParams) (*Result, error) {
	params.Steps = steps
	return c.Query(ctx, AnalysisFunnel, params)
}

// MultiAnalysis runs several named analyses over the same collection.
func (c *Client) MultiAnalysis(ctx context.Context, collection string, analyses map[string]Analysis, params QueryParams) (*Result, error) {
	params.EventCollection = collection
	params.Analyses = analyses
	return c.Query(ctx, AnalysisMultiAnalysis, params)
}
