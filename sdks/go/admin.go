package incisive

import (
	"context"
	"encoding/json"
	"net/http"
	"net/url"
	"strconv"
)

// ---------------------------------------------------------------------------
// Discovery

// CollectionSchema describes an event collection and its inferred property
// types ("string", "num", "bool", "list", ...).
type CollectionSchema struct {
	Name       string            `json:"name"`
	Properties map[string]string `json:"properties"`
	URL        string            `json:"url,omitempty"`
}

// PropertySchema describes one property of a collection.
type PropertySchema struct {
	PropertyName string `json:"property_name"`
	Type         string `json:"type"`
	URL          string `json:"url,omitempty"`
}

// Collections lists every collection with its schema (read key).
func (c *Client) Collections(ctx context.Context) ([]CollectionSchema, error) {
	var out []CollectionSchema
	err := c.do(ctx, http.MethodGet, readKey, c.projectURL("events"), nil, nil, &out)
	return out, err
}

// Collection returns one collection's schema (read key).
func (c *Client) Collection(ctx context.Context, collection string) (*CollectionSchema, error) {
	var out CollectionSchema
	if err := c.do(ctx, http.MethodGet, readKey, c.projectURL("events", collection), nil, nil, &out); err != nil {
		return nil, err
	}
	if out.Name == "" {
		out.Name = collection
	}
	return &out, nil
}

// Property returns one property's schema (read key).
func (c *Client) Property(ctx context.Context, collection, property string) (*PropertySchema, error) {
	var out PropertySchema
	if err := c.do(ctx, http.MethodGet, readKey, c.projectURL("events", collection, "properties", property), nil, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// ---------------------------------------------------------------------------
// Deletes

// DeleteParams selects which events DeleteEvents removes.
type DeleteParams struct {
	Filters   []Filter
	Timeframe *Timeframe
	Timezone  *Timezone
}

// DeleteEvents deletes events in collection matching p (master key). The
// parameters travel in the query string: filters as JSON, timeframe as the
// relative string or absolute JSON object, timezone verbatim. With an empty
// DeleteParams this is equivalent to DeleteCollection.
func (c *Client) DeleteEvents(ctx context.Context, collection string, p DeleteParams) error {
	q := url.Values{}
	if len(p.Filters) > 0 {
		b, err := json.Marshal(p.Filters)
		if err != nil {
			return err
		}
		q.Set("filters", string(b))
	}
	if p.Timeframe != nil {
		tf, err := p.Timeframe.queryValue()
		if err != nil {
			return err
		}
		q.Set("timeframe", tf)
	}
	if p.Timezone != nil {
		q.Set("timezone", p.Timezone.queryValue())
	}
	return c.do(ctx, http.MethodDelete, masterKey, c.projectURL("events", collection), q, nil, nil)
}

// DeleteCollection drops a collection and all its events (master key).
func (c *Client) DeleteCollection(ctx context.Context, collection string) error {
	return c.do(ctx, http.MethodDelete, masterKey, c.projectURL("events", collection), nil, nil, nil)
}

// DeleteProperty removes a property from every event in a collection
// (master key).
func (c *Client) DeleteProperty(ctx context.Context, collection, property string) error {
	return c.do(ctx, http.MethodDelete, masterKey, c.projectURL("events", collection, "properties", property), nil, nil, nil)
}

// ---------------------------------------------------------------------------
// Access keys

// Access-key permission names.
const (
	PermitWrites          = "writes"
	PermitQueries         = "queries"
	PermitSavedQueries    = "saved_queries"
	PermitCachedQueries   = "cached_queries"
	PermitDatasets        = "datasets"
	PermitSchema          = "schema"
	PermitQueryDefinition = "query_definition"
)

// AccessKey is a project access key.
type AccessKey struct {
	Key       string                 `json:"key"`
	Name      string                 `json:"name"`
	IsActive  bool                   `json:"is_active"`
	Permitted []string               `json:"permitted"`
	Options   map[string]interface{} `json:"options"`
}

// AccessKeyInput is the body for creating or fully replacing an access key.
type AccessKeyInput struct {
	Name string `json:"name"`
	// IsActive defaults to true when nil.
	IsActive  *bool                  `json:"is_active"`
	Permitted []string               `json:"permitted"`
	Options   map[string]interface{} `json:"options"`
}

func (in AccessKeyInput) body() AccessKeyInput {
	if in.IsActive == nil {
		in.IsActive = Bool(true)
	}
	if in.Permitted == nil {
		in.Permitted = []string{}
	}
	if in.Options == nil {
		in.Options = map[string]interface{}{}
	}
	return in
}

// CreateAccessKey creates an access key with POST /keys (master key).
func (c *Client) CreateAccessKey(ctx context.Context, in AccessKeyInput) (*AccessKey, error) {
	var out AccessKey
	if err := c.do(ctx, http.MethodPost, masterKey, c.projectURL("keys"), nil, in.body(), &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// AccessKeyListOptions filters and paginates ListAccessKeys.
type AccessKeyListOptions struct {
	Name    string
	Page    int
	PerPage int
}

// ListAccessKeys lists access keys (master key).
func (c *Client) ListAccessKeys(ctx context.Context, opts AccessKeyListOptions) ([]AccessKey, error) {
	q := url.Values{}
	if opts.Name != "" {
		q.Set("name", opts.Name)
	}
	if opts.Page > 0 {
		q.Set("page", strconv.Itoa(opts.Page))
	}
	if opts.PerPage > 0 {
		q.Set("per_page", strconv.Itoa(opts.PerPage))
	}
	var out struct {
		Keys []AccessKey `json:"keys"`
	}
	if err := c.do(ctx, http.MethodGet, masterKey, c.projectURL("keys"), q, nil, &out); err != nil {
		return nil, err
	}
	return out.Keys, nil
}

// GetAccessKey fetches one access key (master key).
func (c *Client) GetAccessKey(ctx context.Context, key string) (*AccessKey, error) {
	var out AccessKey
	if err := c.do(ctx, http.MethodGet, masterKey, c.projectURL("keys", key), nil, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// UpdateAccessKey replaces name, is_active, permitted and options with
// POST /keys/{key} (master key).
func (c *Client) UpdateAccessKey(ctx context.Context, key string, in AccessKeyInput) (*AccessKey, error) {
	var out AccessKey
	if err := c.do(ctx, http.MethodPost, masterKey, c.projectURL("keys", key), nil, in.body(), &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// RevokeAccessKey deactivates a key with POST /keys/{key}/revoke.
func (c *Client) RevokeAccessKey(ctx context.Context, key string) (*AccessKey, error) {
	var out AccessKey
	if err := c.do(ctx, http.MethodPost, masterKey, c.projectURL("keys", key, "revoke"), nil, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// UnrevokeAccessKey reactivates a key with POST /keys/{key}/unrevoke.
func (c *Client) UnrevokeAccessKey(ctx context.Context, key string) (*AccessKey, error) {
	var out AccessKey
	if err := c.do(ctx, http.MethodPost, masterKey, c.projectURL("keys", key, "unrevoke"), nil, nil, &out); err != nil {
		return nil, err
	}
	return &out, nil
}

// DeleteAccessKey deletes a key with DELETE /keys/{key} (master key).
func (c *Client) DeleteAccessKey(ctx context.Context, key string) error {
	return c.do(ctx, http.MethodDelete, masterKey, c.projectURL("keys", key), nil, nil, nil)
}
