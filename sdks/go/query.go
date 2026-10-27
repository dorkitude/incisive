package incisive

import (
	"bytes"
	"encoding/json"
	"fmt"
	"time"
)

// Analysis types accepted by Client.Query and in saved queries.
const (
	AnalysisCount             = "count"
	AnalysisCountUnique       = "count_unique"
	AnalysisSum               = "sum"
	AnalysisAverage           = "average"
	AnalysisMinimum           = "minimum"
	AnalysisMaximum           = "maximum"
	AnalysisMedian            = "median"
	AnalysisPercentile        = "percentile"
	AnalysisSelectUnique      = "select_unique"
	AnalysisStandardDeviation = "standard_deviation"
	AnalysisExtraction        = "extraction"
	AnalysisFunnel            = "funnel"
	AnalysisMultiAnalysis     = "multi_analysis"
)

// Order directions for OrderBy.
const (
	Ascending  = "ASC"
	Descending = "DESC"
)

// ---------------------------------------------------------------------------
// Timeframe

// Timeframe is either relative ("this_7_days", "previous_2_weeks") or
// absolute (Start/End ISO-8601 strings). It encodes to a JSON string or a
// {"start","end"} object respectively.
type Timeframe struct {
	Relative string
	Start    string
	End      string
}

// RelativeTimeframe returns a rolling timeframe such as "this_14_days".
func RelativeTimeframe(expr string) *Timeframe { return &Timeframe{Relative: expr} }

// AbsoluteTimeframe returns a fixed [start, end) timeframe.
func AbsoluteTimeframe(start, end time.Time) *Timeframe {
	return &Timeframe{Start: Timestamp(start), End: Timestamp(end)}
}

// IsRelative reports whether t is a relative timeframe.
func (t *Timeframe) IsRelative() bool { return t.Relative != "" }

// MarshalJSON implements json.Marshaler.
func (t Timeframe) MarshalJSON() ([]byte, error) {
	if t.Relative != "" {
		return json.Marshal(t.Relative)
	}
	return json.Marshal(struct {
		Start string `json:"start"`
		End   string `json:"end"`
	}{t.Start, t.End})
}

// UnmarshalJSON implements json.Unmarshaler.
func (t *Timeframe) UnmarshalJSON(b []byte) error {
	b = bytes.TrimSpace(b)
	if len(b) > 0 && b[0] == '"' {
		*t = Timeframe{}
		return json.Unmarshal(b, &t.Relative)
	}
	var abs struct {
		Start string `json:"start"`
		End   string `json:"end"`
	}
	if err := json.Unmarshal(b, &abs); err != nil {
		return err
	}
	*t = Timeframe{Start: abs.Start, End: abs.End}
	return nil
}

// queryValue is the query-string form: relative as-is, absolute as JSON.
func (t *Timeframe) queryValue() (string, error) {
	if t.Relative != "" {
		return t.Relative, nil
	}
	b, err := json.Marshal(t)
	return string(b), err
}

// ---------------------------------------------------------------------------
// Timezone

// Timezone is a named zone ("US/Pacific") or a fixed offset in seconds
// (-28800). It encodes to a JSON string or number.
type Timezone struct {
	Name          string
	OffsetSeconds int
}

// NamedTimezone returns a named IANA timezone.
func NamedTimezone(name string) *Timezone { return &Timezone{Name: name} }

// OffsetTimezone returns a fixed UTC offset in seconds.
func OffsetTimezone(seconds int) *Timezone { return &Timezone{OffsetSeconds: seconds} }

// MarshalJSON implements json.Marshaler.
func (z Timezone) MarshalJSON() ([]byte, error) {
	if z.Name != "" {
		return json.Marshal(z.Name)
	}
	return json.Marshal(z.OffsetSeconds)
}

// UnmarshalJSON implements json.Unmarshaler.
func (z *Timezone) UnmarshalJSON(b []byte) error {
	b = bytes.TrimSpace(b)
	*z = Timezone{}
	if len(b) > 0 && b[0] == '"' {
		return json.Unmarshal(b, &z.Name)
	}
	var f float64
	if err := json.Unmarshal(b, &f); err != nil {
		return err
	}
	z.OffsetSeconds = int(f)
	return nil
}

func (z *Timezone) queryValue() string {
	if z.Name != "" {
		return z.Name
	}
	return fmt.Sprint(z.OffsetSeconds)
}

// ---------------------------------------------------------------------------
// Filters

// Filter narrows the events an analysis considers. For an "or" filter set
// Operator to "or" and list the alternatives in Operands.
type Filter struct {
	PropertyName  string
	Operator      string
	PropertyValue interface{}
	Operands      []Filter
}

// Or combines filters so an event matches if any operand matches.
func Or(operands ...Filter) Filter { return Filter{Operator: "or", Operands: operands} }

// MarshalJSON implements json.Marshaler. property_value is always emitted
// for non-"or" filters, so false/0/null values survive.
func (f Filter) MarshalJSON() ([]byte, error) {
	if f.Operator == "or" {
		ops := f.Operands
		if ops == nil {
			ops = []Filter{}
		}
		return json.Marshal(struct {
			Operator string   `json:"operator"`
			Operands []Filter `json:"operands"`
		}{f.Operator, ops})
	}
	return json.Marshal(struct {
		PropertyName  string      `json:"property_name"`
		Operator      string      `json:"operator"`
		PropertyValue interface{} `json:"property_value"`
	}{f.PropertyName, f.Operator, f.PropertyValue})
}

// UnmarshalJSON implements json.Unmarshaler.
func (f *Filter) UnmarshalJSON(b []byte) error {
	var raw struct {
		PropertyName  string      `json:"property_name"`
		Operator      string      `json:"operator"`
		PropertyValue interface{} `json:"property_value"`
		Operands      []Filter    `json:"operands"`
	}
	if err := json.Unmarshal(b, &raw); err != nil {
		return err
	}
	*f = Filter{raw.PropertyName, raw.Operator, raw.PropertyValue, raw.Operands}
	return nil
}

// ---------------------------------------------------------------------------
// Group by

// GroupBy is the list of group_by clauses. A single plain property encodes
// as a bare string; anything else encodes as an array whose plain entries are
// strings and bucket/range entries are objects.
type GroupBy []GroupByClause

// GroupByClause groups by a property, optionally into numeric buckets or
// explicit ranges.
type GroupByClause struct {
	PropertyName      string
	Buckets           *Buckets
	Ranges            []Range
	Alias             string
	AllowRangeOverlap bool
}

// Buckets groups numeric values into fixed-width buckets.
type Buckets struct {
	Size   float64  `json:"size"`
	Offset *float64 `json:"offset,omitempty"`
}

// Range is one explicit group_by range. Nil From/To mean -Inf/+Inf.
type Range struct {
	From       *float64 `json:"from,omitempty"`
	To         *float64 `json:"to,omitempty"`
	FromClosed *bool    `json:"from_closed,omitempty"`
	ToClosed   *bool    `json:"to_closed,omitempty"`
	Key        string   `json:"key,omitempty"`
}

// GroupByProperties groups by one or more plain properties.
func GroupByProperties(props ...string) GroupBy {
	g := make(GroupBy, len(props))
	for i, p := range props {
		g[i] = GroupByClause{PropertyName: p}
	}
	return g
}

func (c GroupByClause) plain() bool {
	return c.Buckets == nil && c.Ranges == nil && c.Alias == "" && !c.AllowRangeOverlap
}

// MarshalJSON implements json.Marshaler.
func (c GroupByClause) MarshalJSON() ([]byte, error) {
	if c.plain() {
		return json.Marshal(c.PropertyName)
	}
	return json.Marshal(struct {
		PropertyName      string   `json:"property_name"`
		Buckets           *Buckets `json:"buckets,omitempty"`
		Ranges            []Range  `json:"ranges,omitempty"`
		Alias             string   `json:"alias,omitempty"`
		AllowRangeOverlap bool     `json:"allow_range_overlap,omitempty"`
	}{c.PropertyName, c.Buckets, c.Ranges, c.Alias, c.AllowRangeOverlap})
}

// UnmarshalJSON implements json.Unmarshaler.
func (c *GroupByClause) UnmarshalJSON(b []byte) error {
	b = bytes.TrimSpace(b)
	if len(b) > 0 && b[0] == '"' {
		*c = GroupByClause{}
		return json.Unmarshal(b, &c.PropertyName)
	}
	var raw struct {
		PropertyName      string   `json:"property_name"`
		Buckets           *Buckets `json:"buckets"`
		Ranges            []Range  `json:"ranges"`
		Alias             string   `json:"alias"`
		AllowRangeOverlap bool     `json:"allow_range_overlap"`
	}
	if err := json.Unmarshal(b, &raw); err != nil {
		return err
	}
	*c = GroupByClause{raw.PropertyName, raw.Buckets, raw.Ranges, raw.Alias, raw.AllowRangeOverlap}
	return nil
}

// MarshalJSON implements json.Marshaler.
func (g GroupBy) MarshalJSON() ([]byte, error) {
	if len(g) == 1 && g[0].plain() {
		return json.Marshal(g[0].PropertyName)
	}
	return json.Marshal([]GroupByClause(g))
}

// UnmarshalJSON implements json.Unmarshaler.
func (g *GroupBy) UnmarshalJSON(b []byte) error {
	b = bytes.TrimSpace(b)
	if string(b) == "null" {
		*g = nil
		return nil
	}
	if len(b) > 0 && b[0] != '[' {
		var c GroupByClause
		if err := json.Unmarshal(b, &c); err != nil {
			return err
		}
		*g = GroupBy{c}
		return nil
	}
	var list []GroupByClause
	if err := json.Unmarshal(b, &list); err != nil {
		return err
	}
	*g = list
	return nil
}

// ---------------------------------------------------------------------------
// Order by

// OrderBy is the list of order_by clauses, always encoded as an array of
// {"property_name","direction"} objects.
type OrderBy []OrderByClause

// OrderByClause orders grouped results by "result" or a group_by property.
type OrderByClause struct {
	PropertyName string `json:"property_name"`
	Direction    string `json:"direction,omitempty"`
}

// UnmarshalJSON accepts a bare string, a single object or an array.
func (o *OrderBy) UnmarshalJSON(b []byte) error {
	b = bytes.TrimSpace(b)
	switch {
	case string(b) == "null":
		*o = nil
		return nil
	case len(b) > 0 && b[0] == '"':
		var s string
		if err := json.Unmarshal(b, &s); err != nil {
			return err
		}
		*o = OrderBy{{PropertyName: s}}
		return nil
	case len(b) > 0 && b[0] == '{':
		var c OrderByClause
		if err := json.Unmarshal(b, &c); err != nil {
			return err
		}
		*o = OrderBy{c}
		return nil
	}
	var items []json.RawMessage
	if err := json.Unmarshal(b, &items); err != nil {
		return err
	}
	out := make(OrderBy, 0, len(items))
	for _, it := range items {
		var one OrderBy
		if err := one.UnmarshalJSON(it); err != nil {
			return err
		}
		out = append(out, one...)
	}
	*o = out
	return nil
}

// ---------------------------------------------------------------------------
// Multi-analysis and funnels

// Analysis is one named sub-analysis of a multi_analysis query.
type Analysis struct {
	AnalysisType   string   `json:"analysis_type"`
	TargetProperty string   `json:"target_property,omitempty"`
	Percentile     *float64 `json:"percentile,omitempty"`
}

// FunnelStep is one step of a funnel. Timeframe/Timezone are inherited from
// the funnel when nil.
type FunnelStep struct {
	EventCollection string     `json:"event_collection"`
	ActorProperty   string     `json:"actor_property"`
	Timeframe       *Timeframe `json:"timeframe,omitempty"`
	Timezone        *Timezone  `json:"timezone,omitempty"`
	Filters         []Filter   `json:"filters,omitempty"`
	Inverted        bool       `json:"inverted,omitempty"`
	Optional        bool       `json:"optional,omitempty"`
	WithActors      bool       `json:"with_actors,omitempty"`
}

// ---------------------------------------------------------------------------
// Query parameters

// QueryParams holds every analysis parameter. Methods such as Count fill in
// EventCollection/TargetProperty/etc. from their positional arguments, so
// callers usually set only Timeframe, Filters, GroupBy and friends.
type QueryParams struct {
	// AnalysisType is only sent inside saved-query and cached-dataset
	// definitions; ad-hoc queries put it in the URL.
	AnalysisType    string              `json:"analysis_type,omitempty"`
	EventCollection string              `json:"event_collection,omitempty"`
	TargetProperty  string              `json:"target_property,omitempty"`
	Percentile      *float64            `json:"percentile,omitempty"`
	Timeframe       *Timeframe          `json:"timeframe,omitempty"`
	Timezone        *Timezone           `json:"timezone,omitempty"`
	Filters         []Filter            `json:"filters,omitempty"`
	GroupBy         GroupBy             `json:"group_by,omitempty"`
	Interval        string              `json:"interval,omitempty"`
	OrderBy         OrderBy             `json:"order_by,omitempty"`
	Limit           int                 `json:"limit,omitempty"`
	Analyses        map[string]Analysis `json:"analyses,omitempty"`
	Steps           []FunnelStep        `json:"steps,omitempty"`

	// Extraction-only.
	PropertyNames   []string `json:"property_names,omitempty"`
	Latest          int      `json:"latest,omitempty"`
	Email           string   `json:"email,omitempty"`
	ContentType     string   `json:"content_type,omitempty"`
	ContentEncoding string   `json:"content_encoding,omitempty"`

	// IncludeMetadata adds include_metadata=true to the query string so the
	// response carries execution_metadata.
	IncludeMetadata bool `json:"-"`

	// Extra is merged into the JSON body last, for parameters this SDK does
	// not model yet. It is not populated when decoding.
	Extra map[string]interface{} `json:"-"`
}

type queryParamsAlias QueryParams

// MarshalJSON implements json.Marshaler, merging Extra.
func (p QueryParams) MarshalJSON() ([]byte, error) {
	b, err := json.Marshal(queryParamsAlias(p))
	if err != nil || len(p.Extra) == 0 {
		return b, err
	}
	m := map[string]json.RawMessage{}
	if err := json.Unmarshal(b, &m); err != nil {
		return nil, err
	}
	for k, v := range p.Extra {
		enc, err := json.Marshal(v)
		if err != nil {
			return nil, err
		}
		m[k] = enc
	}
	return json.Marshal(m)
}

// UnmarshalJSON implements json.Unmarshaler.
func (p *QueryParams) UnmarshalJSON(b []byte) error {
	var a queryParamsAlias
	if err := json.Unmarshal(b, &a); err != nil {
		return err
	}
	*p = QueryParams(a)
	return nil
}

// Float returns a pointer to f, for optional numeric fields.
func Float(f float64) *float64 { return &f }

// Bool returns a pointer to b, for optional boolean fields.
func Bool(b bool) *bool { return &b }
