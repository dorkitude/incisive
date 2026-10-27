package incisive

import (
	"context"
	"encoding/json"
	"net/http"
	"net/url"
	"strconv"
)

// SavedQueryInput is the body of a saved-query create/replace.
type SavedQueryInput struct {
	// RefreshRate in seconds; 0 means not cached. Values > 0 make it a
	// cached query.
	RefreshRate int `json:"refresh_rate"`
	// Query must set AnalysisType and EventCollection (or Steps).
	Query    QueryParams            `json:"query"`
	Metadata map[string]interface{} `json:"metadata,omitempty"`
}

// SavedQuery is a saved-query definition as returned by the server. Result is
// populated only by SavedQueryResult.
type SavedQuery struct {
	QueryName            string                 `json:"query_name"`
	RefreshRate          int                    `json:"refresh_rate"`
	Query                QueryParams            `json:"query"`
	Metadata             map[string]interface{} `json:"metadata"`
	Created              bool                   `json:"created,omitempty"`
	CreatedDate          string                 `json:"created_date,omitempty"`
	LastModifiedDate     string                 `json:"last_modified_date,omitempty"`
	UserLastModifiedDate string                 `json:"user_last_modified_date,omitempty"`
	URLs                 map[string]string      `json:"urls,omitempty"`
	RunInformation       json.RawMessage        `json:"run_information,omitempty"`
	Result               json.RawMessage        `json:"result,omitempty"`
	// Raw is the complete response body.
	Raw json.RawMessage `json:"-"`
}

type savedQueryAlias SavedQuery

// UnmarshalJSON implements json.Unmarshaler and keeps Raw.
func (s *SavedQuery) UnmarshalJSON(b []byte) error {
	var a savedQueryAlias
	if err := json.Unmarshal(b, &a); err != nil {
		return err
	}
	*s = SavedQuery(a)
	s.Raw = append(json.RawMessage(nil), b...)
	return nil
}

// SavedQueries lists all saved-query definitions (master key).
func (c *Client) SavedQueries(ctx context.Context) ([]SavedQuery, error) {
	var out []SavedQuery
	err := c.do(ctx, http.MethodGet, masterKey, c.projectURL("queries", "saved"), nil, nil, &out)
	return out, err
}

// SavedQuery fetches one saved-query definition (master key).
func (c *Client) SavedQuery(ctx context.Context, name string) (*SavedQuery, error) {
	var out SavedQuery
	if err := c.do(ctx, http.MethodGet, masterKey, c.projectURL("queries", "saved", name), nil, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// SavedQueryResult runs (or reads the cached result of) a saved query (read
// key). The returned definition has Result set.
func (c *Client) SavedQueryResult(ctx context.Context, name string) (*SavedQuery, error) {
	var out SavedQuery
	if err := c.do(ctx, http.MethodGet, readKey, c.projectURL("queries", "saved", name, "result"), nil, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// CreateSavedQuery creates or replaces a saved query with PUT (master key).
func (c *Client) CreateSavedQuery(ctx context.Context, name string, def SavedQueryInput) (*SavedQuery, error) {
	var out SavedQuery
	if err := c.do(ctx, http.MethodPut, masterKey, c.projectURL("queries", "saved", name), nil, def, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// UpdateSavedQuery replaces the full definition. Anything omitted is removed.
func (c *Client) UpdateSavedQuery(ctx context.Context, name string, def SavedQueryInput) (*SavedQuery, error) {
	return c.CreateSavedQuery(ctx, name, def)
}

// UpdateSavedQueryAttributes performs a partial update: it fetches the
// current definition, keeps refresh_rate, metadata and every non-empty query
// member, deep-merges attrs on top, and PUTs the result.
func (c *Client) UpdateSavedQueryAttributes(ctx context.Context, name string, attrs map[string]interface{}) (*SavedQuery, error) {
	var current map[string]interface{}
	if err := c.do(ctx, http.MethodGet, masterKey, c.projectURL("queries", "saved", name), nil, nil, &current); err != nil {
		return nil, err
	}
	next := map[string]interface{}{
		"query_name":   current["query_name"],
		"refresh_rate": current["refresh_rate"],
	}
	if md, ok := current["metadata"]; ok && !isEmpty(md) {
		next["metadata"] = md
	}
	q := map[string]interface{}{}
	if old, ok := current["query"].(map[string]interface{}); ok {
		for k, v := range old {
			if !isEmpty(v) {
				q[k] = v
			}
		}
	}
	next["query"] = q
	deepMerge(next, attrs)
	var out SavedQuery
	if err := c.do(ctx, http.MethodPut, masterKey, c.projectURL("queries", "saved", name), nil, next, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// DeleteSavedQuery removes a saved query (master key).
func (c *Client) DeleteSavedQuery(ctx context.Context, name string) error {
	return c.do(ctx, http.MethodDelete, masterKey, c.projectURL("queries", "saved", name), nil, nil, nil)
}

func isEmpty(v interface{}) bool {
	switch t := v.(type) {
	case nil:
		return true
	case string:
		return t == ""
	case bool:
		return !t
	case float64:
		return t == 0
	case []interface{}:
		return len(t) == 0
	case map[string]interface{}:
		return len(t) == 0
	}
	return false
}

func deepMerge(dst, src map[string]interface{}) {
	for k, v := range src {
		if sm, ok := v.(map[string]interface{}); ok {
			if dm, ok := dst[k].(map[string]interface{}); ok {
				deepMerge(dm, sm)
				continue
			}
			cp := map[string]interface{}{}
			deepMerge(cp, sm)
			dst[k] = cp
			continue
		}
		dst[k] = v
	}
}

// ---------------------------------------------------------------------------
// Cached datasets

// IndexBy is one or more index_by property names. A single name encodes as
// a string, several as an array.
type IndexBy []string

// MarshalJSON implements json.Marshaler.
func (i IndexBy) MarshalJSON() ([]byte, error) {
	if len(i) == 1 {
		return json.Marshal(i[0])
	}
	return json.Marshal([]string(i))
}

// UnmarshalJSON implements json.Unmarshaler.
func (i *IndexBy) UnmarshalJSON(b []byte) error {
	var s string
	if json.Unmarshal(b, &s) == nil {
		*i = IndexBy{s}
		return nil
	}
	var list []string
	if err := json.Unmarshal(b, &list); err != nil {
		return err
	}
	*i = list
	return nil
}

// CachedDatasetInput is the body of a cached-dataset create.
type CachedDatasetInput struct {
	DisplayName string      `json:"display_name"`
	Query       QueryParams `json:"query"`
	IndexBy     IndexBy     `json:"index_by"`
}

// CachedDataset is a cached-dataset definition.
type CachedDataset struct {
	DatasetName                 string          `json:"dataset_name"`
	DisplayName                 string          `json:"display_name"`
	ProjectID                   string          `json:"project_id,omitempty"`
	OrganizationID              string          `json:"organization_id,omitempty"`
	Query                       QueryParams     `json:"query"`
	IndexBy                     IndexBy         `json:"index_by"`
	LastScheduledDate           string          `json:"last_scheduled_date,omitempty"`
	LatestSubtimeframeAvailable string          `json:"latest_subtimeframe_available,omitempty"`
	MillisecondsBehind          int64           `json:"milliseconds_behind,omitempty"`
	Status                      string          `json:"status,omitempty"`
	Raw                         json.RawMessage `json:"-"`
}

type cachedDatasetAlias CachedDataset

// UnmarshalJSON implements json.Unmarshaler and keeps Raw.
func (d *CachedDataset) UnmarshalJSON(b []byte) error {
	var a cachedDatasetAlias
	if err := json.Unmarshal(b, &a); err != nil {
		return err
	}
	*d = CachedDataset(a)
	d.Raw = append(json.RawMessage(nil), b...)
	return nil
}

// CachedDatasetList is one page of dataset definitions.
type CachedDatasetList struct {
	Datasets    []CachedDataset `json:"datasets"`
	Count       int             `json:"count"`
	NextPageURL string          `json:"next_page_url"`
}

// ListOptions paginates CachedDatasets.
type ListOptions struct {
	Limit     int
	AfterName string
}

// CachedDatasets lists dataset definitions (read key).
func (c *Client) CachedDatasets(ctx context.Context, opts ListOptions) (*CachedDatasetList, error) {
	q := url.Values{}
	if opts.Limit > 0 {
		q.Set("limit", strconv.Itoa(opts.Limit))
	}
	if opts.AfterName != "" {
		q.Set("after_name", opts.AfterName)
	}
	var out CachedDatasetList
	if err := c.do(ctx, http.MethodGet, readKey, c.projectURL("datasets"), q, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// CachedDataset fetches one dataset definition (read key).
func (c *Client) CachedDataset(ctx context.Context, name string) (*CachedDataset, error) {
	var out CachedDataset
	if err := c.do(ctx, http.MethodGet, readKey, c.projectURL("datasets", name), nil, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// CreateCachedDataset creates a dataset with PUT (master key).
func (c *Client) CreateCachedDataset(ctx context.Context, name string, def CachedDatasetInput) (*CachedDataset, error) {
	var out CachedDataset
	if err := c.do(ctx, http.MethodPut, masterKey, c.projectURL("datasets", name), nil, def, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// DatasetResultsParams selects results from a cached dataset.
type DatasetResultsParams struct {
	// IndexBy is a string (single string-typed index) or any JSON-encodable
	// value such as map[string]interface{} for compound indexes.
	IndexBy   interface{}
	Timeframe *Timeframe
	// ZeroFill defaults to true server side.
	ZeroFill *bool
}

// CachedDatasetResults reads results with GET; index_by and timeframe are
// sent as query parameters (strings verbatim, other values JSON-encoded).
func (c *Client) CachedDatasetResults(ctx context.Context, name string, p DatasetResultsParams) (*Result, error) {
	q := url.Values{}
	switch v := p.IndexBy.(type) {
	case nil:
	case string:
		q.Set("index_by", v)
	default:
		b, err := json.Marshal(v)
		if err != nil {
			return nil, err
		}
		q.Set("index_by", string(b))
	}
	if p.Timeframe != nil {
		tf, err := p.Timeframe.queryValue()
		if err != nil {
			return nil, err
		}
		q.Set("timeframe", tf)
	}
	if p.ZeroFill != nil {
		q.Set("zero_fill", strconv.FormatBool(*p.ZeroFill))
	}
	var out Result
	if err := c.do(ctx, http.MethodGet, readKey, c.projectURL("datasets", name, "results"), q, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// DeleteCachedDataset removes a dataset (master key).
func (c *Client) DeleteCachedDataset(ctx context.Context, name string) error {
	return c.do(ctx, http.MethodDelete, masterKey, c.projectURL("datasets", name), nil, nil, nil)
}
