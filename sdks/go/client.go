// Package incisive is a Go client for the incisive analytics server.
//
// It covers event tracking (single events, multi-collection batches and a
// background BatchClient that flushes on an interval), every analysis type,
// saved queries, cached datasets, schema discovery, deletes, access-key
// management and scoped-key encryption.
//
// A Client is configured with a struct literal; zero values get defaults:
//
//	c := &incisive.Client{ProjectID: "PROJECT_ID", WriteKey: "WRITE_KEY"}
//	err := c.AddEvent("purchases", map[string]any{"price": 7.13})
package incisive

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
)

// Version is the SDK version reported in the User-Agent header.
const Version = "0.1.0"

const (
	// DefaultBaseURL is used when Client.BaseURL is empty.
	DefaultBaseURL = "http://localhost:8080"
	// DefaultAPIVersion is used when Client.APIVersion is empty.
	DefaultAPIVersion = "1.0"
)

// Errors returned before any request is made when a required key is missing.
var (
	ErrMissingProjectID = errors.New("incisive: project id is not set")
	ErrMissingWriteKey  = errors.New("incisive: write key is not set")
	ErrMissingReadKey   = errors.New("incisive: read key (or master key) is not set")
	ErrMissingMasterKey = errors.New("incisive: master key is not set")
)

// Client talks to one project on an incisive server. It is safe for
// concurrent use once configured; do not mutate fields while requests are in
// flight.
type Client struct {
	// ProjectID is the project every request is scoped to.
	ProjectID string
	// WriteKey authorizes event writes.
	WriteKey string
	// ReadKey authorizes analyses, discovery and saved-query results. When it
	// is empty, MasterKey is used instead.
	ReadKey string
	// MasterKey authorizes deletes, saved-query/dataset management and access
	// key management.
	MasterKey string

	// BaseURL is the server root, e.g. "https://analytics.example.com".
	// Defaults to DefaultBaseURL.
	BaseURL string
	// APIVersion is the path prefix version. Defaults to DefaultAPIVersion.
	APIVersion string
	// HTTPClient performs requests. Defaults to http.DefaultClient.
	HTTPClient *http.Client
	// UserAgent overrides the default "incisive-go/<Version>" User-Agent.
	UserAgent string
}

// NewClientFromEnv builds a Client from INCISIVE_PROJECT_ID,
// INCISIVE_WRITE_KEY, INCISIVE_READ_KEY, INCISIVE_MASTER_KEY,
// INCISIVE_BASE_URL and INCISIVE_API_VERSION. Only the project id is required.
func NewClientFromEnv() (*Client, error) {
	c := &Client{
		ProjectID:  os.Getenv("INCISIVE_PROJECT_ID"),
		WriteKey:   os.Getenv("INCISIVE_WRITE_KEY"),
		ReadKey:    os.Getenv("INCISIVE_READ_KEY"),
		MasterKey:  os.Getenv("INCISIVE_MASTER_KEY"),
		BaseURL:    os.Getenv("INCISIVE_BASE_URL"),
		APIVersion: os.Getenv("INCISIVE_API_VERSION"),
	}
	if c.ProjectID == "" {
		return nil, fmt.Errorf("%w (set INCISIVE_PROJECT_ID)", ErrMissingProjectID)
	}
	return c, nil
}

// APIError is returned for any non-2xx response.
type APIError struct {
	// Status is the HTTP status code.
	Status int
	// Message is the server's "message" field, or a description of the
	// non-JSON body.
	Message string
	// ErrorCode is the server's "error_code" field. For non-JSON bodies it is
	// the status code as a string.
	ErrorCode string
	// Body is the raw response body (truncated to 64 KiB).
	Body []byte
}

func (e *APIError) Error() string {
	return fmt.Sprintf("incisive: HTTP %d: %s (error_code=%s)", e.Status, e.Message, e.ErrorCode)
}

type keyKind int

const (
	writeKey keyKind = iota
	readKey
	masterKey
)

func (c *Client) key(k keyKind) (string, error) {
	switch k {
	case writeKey:
		if c.WriteKey == "" {
			return "", ErrMissingWriteKey
		}
		return c.WriteKey, nil
	case readKey:
		if c.ReadKey != "" {
			return c.ReadKey, nil
		}
		if c.MasterKey != "" {
			return c.MasterKey, nil
		}
		return "", ErrMissingReadKey
	default:
		if c.MasterKey == "" {
			return "", ErrMissingMasterKey
		}
		return c.MasterKey, nil
	}
}

func (c *Client) httpClient() *http.Client {
	if c.HTTPClient != nil {
		return c.HTTPClient
	}
	return http.DefaultClient
}

// projectURL returns {base}/{version}/projects/{id} followed by the given
// path segments, each path-escaped.
func (c *Client) projectURL(segments ...string) string {
	base := c.BaseURL
	if base == "" {
		base = DefaultBaseURL
	}
	ver := c.APIVersion
	if ver == "" {
		ver = DefaultAPIVersion
	}
	var b strings.Builder
	b.WriteString(strings.TrimRight(base, "/"))
	b.WriteString("/")
	b.WriteString(ver)
	b.WriteString("/projects/")
	b.WriteString(url.PathEscape(c.ProjectID))
	for _, s := range segments {
		b.WriteString("/")
		b.WriteString(url.PathEscape(s))
	}
	return b.String()
}

// do sends a request and decodes a 2xx JSON response into out (if non-nil).
// body is JSON-encoded when non-nil.
func (c *Client) do(ctx context.Context, method string, k keyKind, u string, query url.Values, body, out any) error {
	if c.ProjectID == "" {
		return ErrMissingProjectID
	}
	key, err := c.key(k)
	if err != nil {
		return err
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if len(query) > 0 {
		u += "?" + query.Encode()
	}
	var rdr io.Reader
	if body != nil {
		buf, err := json.Marshal(body)
		if err != nil {
			return fmt.Errorf("incisive: encoding request body: %w", err)
		}
		rdr = bytes.NewReader(buf)
	}
	req, err := http.NewRequestWithContext(ctx, method, u, rdr)
	if err != nil {
		return err
	}
	req.Header.Set("Authorization", key)
	req.Header.Set("Accept", "application/json")
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	ua := c.UserAgent
	if ua == "" {
		ua = "incisive-go/" + Version
	}
	req.Header.Set("User-Agent", ua)

	resp, err := c.httpClient().Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode/100 != 2 {
		return parseAPIError(resp)
	}
	if out == nil {
		_, _ = io.Copy(io.Discard, resp.Body)
		return nil
	}
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return err
	}
	if len(bytes.TrimSpace(data)) == 0 {
		return nil
	}
	if err := json.Unmarshal(data, out); err != nil {
		return fmt.Errorf("incisive: decoding response: %w", err)
	}
	return nil
}

func parseAPIError(resp *http.Response) error {
	data, _ := io.ReadAll(io.LimitReader(resp.Body, 64<<10))
	e := &APIError{Status: resp.StatusCode, Body: data}
	var env struct {
		Message   string          `json:"message"`
		ErrorCode json.RawMessage `json:"error_code"`
	}
	if err := json.Unmarshal(data, &env); err == nil && (env.Message != "" || len(env.ErrorCode) > 0) {
		e.Message = env.Message
		e.ErrorCode = rawToString(env.ErrorCode)
		return e
	}
	snippet := string(data)
	if len(snippet) > 1000 {
		snippet = snippet[:1000]
	}
	e.Message = "the API did not respond with JSON, but: " + snippet
	e.ErrorCode = strconv.Itoa(resp.StatusCode)
	return e
}

// rawToString renders a JSON string as its contents and anything else
// (numbers, null) as its literal text.
func rawToString(r json.RawMessage) string {
	if len(r) == 0 || string(r) == "null" {
		return ""
	}
	var s string
	if json.Unmarshal(r, &s) == nil {
		return s
	}
	return string(r)
}
