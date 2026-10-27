"""Low-level HTTP layer: builds URLs, attaches keys, checks responses."""

import json

import requests

from incisive import direction, exceptions, utilities
from incisive.utilities import IncisiveKeys, escape_segment, requires_key

DEFAULT_BASE_URL = "http://localhost:8080"
DEFAULT_API_VERSION = "3.0"


def _json_or_none(response):
    """Parsed JSON body, or None for an empty body (e.g. 204)."""
    if not response.content:
        return None
    return response.json()


class HTTPMethods:
    GET = "get"
    POST = "post"
    DELETE = "delete"
    PUT = "put"


class IncisiveApi:
    """Talks to the server. Shared by persistence strategies and the query helpers."""

    base_url = DEFAULT_BASE_URL
    api_version = DEFAULT_API_VERSION

    def __init__(self, project_id, write_key=None, read_key=None, base_url=None,
                 api_version=None, get_timeout=None, post_timeout=None, master_key=None):
        self.project_id = project_id
        self.write_key = write_key
        self.read_key = read_key
        self.master_key = master_key
        if base_url:
            self.base_url = base_url.rstrip("/")
        if api_version:
            self.api_version = api_version
        self.get_timeout = get_timeout
        self.post_timeout = post_timeout
        self.session = requests.Session()

    # -- plumbing ---------------------------------------------------------------

    def project_url(self, *segments):
        """``{base}/{version}/projects/{pid}/seg/seg`` with every segment escaped."""
        parts = [self.base_url, self.api_version, "projects", escape_segment(self.project_id)]
        parts.extend(escape_segment(s) for s in segments)
        return "/".join(parts)

    def fulfill(self, method, *args, **kwargs):
        return getattr(self.session, method)(*args, **kwargs)

    def _request(self, method, url, key, timeout=None, **kwargs):
        response = self.fulfill(method, url, headers=utilities.headers(key),
                                timeout=timeout, **kwargs)
        self._error_handling(response)
        return response

    def _error_handling(self, res):
        if res.status_code // 100 != 2:
            raise exceptions.IncisiveApiError(self._get_response_json(res),
                                              status_code=res.status_code)

    def _get_response_json(self, res):
        try:
            body = res.json()
        except ValueError:
            body = {
                "message": "The API did not respond with JSON, but: {0}".format(res.text[:1000]),
                "error_code": "{0}".format(res.status_code),
            }
        return body

    def _get_read_key(self):
        return self.read_key

    def _get_write_key(self):
        return self.write_key

    def _get_master_key(self):
        return self.master_key

    # -- events -----------------------------------------------------------------

    @requires_key(IncisiveKeys.WRITE)
    def post_event(self, event):
        url = self.project_url("events", event.event_collection)
        self._request(HTTPMethods.POST, url, self.write_key, timeout=self.post_timeout,
                      data=event.to_json())

    @requires_key(IncisiveKeys.WRITE)
    def post_events(self, events):
        url = self.project_url("events")
        response = self._request(HTTPMethods.POST, url, self.write_key,
                                 timeout=self.post_timeout, data=json.dumps(events))
        return self._get_response_json(response)

    @requires_key(IncisiveKeys.MASTER)
    def delete_events(self, event_collection, params):
        url = self.project_url("events", event_collection)
        self._request(HTTPMethods.DELETE, url, self.master_key, timeout=self.post_timeout,
                      params=params)
        return True

    @requires_key(IncisiveKeys.READ)
    def get_collection(self, event_collection):
        url = self.project_url("events", event_collection)
        return self._request(HTTPMethods.GET, url, self.read_key,
                             timeout=self.get_timeout).json()

    @requires_key(IncisiveKeys.READ)
    def get_all_collections(self):
        url = self.project_url("events")
        return self._request(HTTPMethods.GET, url, self.read_key,
                             timeout=self.get_timeout).json()

    # -- queries ----------------------------------------------------------------

    @staticmethod
    def _order_by_is_valid_or_none(params):
        if not params.get("order_by"):
            return True

        def well_formed(d):
            if not isinstance(d, dict) or not d.get("property_name"):
                return False
            if "direction" in d and not direction.is_valid_direction(d["direction"]):
                return False
            return set(d) <= {"property_name", "direction"}

        if not all(well_formed(d) for d in json.loads(params["order_by"])):
            return False
        return bool(params.get("group_by"))

    @staticmethod
    def _limit_is_valid_or_none(params):
        if not params.get("limit"):
            return True
        limit = params["limit"]
        if isinstance(limit, bool) or not isinstance(limit, int) or limit < 1:
            return False
        return "order_by" in params

    @requires_key(IncisiveKeys.READ)
    def query(self, analysis_type, params, all_keys=False):
        if not self._order_by_is_valid_or_none(params):
            raise ValueError("order_by given is invalid or is missing required group_by.")
        if not self._limit_is_valid_or_none(params):
            raise ValueError("limit given is invalid or is missing required order_by.")
        url = self.project_url("queries", analysis_type)
        body = self._request(HTTPMethods.GET, url, self.read_key, timeout=self.get_timeout,
                             params=params).json()
        return body if all_keys else body["result"]

    # -- access keys ------------------------------------------------------------

    @staticmethod
    def _access_key_body(name, is_active, permitted, options):
        return json.dumps({"name": name, "is_active": is_active,
                           "permitted": permitted, "options": options})

    @requires_key(IncisiveKeys.MASTER)
    def create_access_key(self, name, is_active=True, permitted=None, options=None):
        url = self.project_url("keys")
        data = self._access_key_body(name, is_active, permitted or [], options or {})
        return self._request(HTTPMethods.POST, url, self.master_key,
                             timeout=self.post_timeout, data=data).json()

    @requires_key(IncisiveKeys.MASTER)
    def list_access_keys(self):
        return self._request(HTTPMethods.GET, self.project_url("keys"), self.master_key,
                             timeout=self.get_timeout).json()

    @requires_key(IncisiveKeys.MASTER)
    def get_access_key(self, key):
        return self._request(HTTPMethods.GET, self.project_url("keys", key), self.master_key,
                             timeout=self.get_timeout).json()

    def _modify_access_key(self, key, change):
        current = self.get_access_key(key)
        body = {f: current[f] for f in ("name", "is_active", "permitted", "options")}
        change(body)
        return self.update_access_key_full(key, **body)

    @requires_key(IncisiveKeys.MASTER)
    def update_access_key_name(self, key, name):
        return self._modify_access_key(key, lambda b: b.update(name=name))

    @requires_key(IncisiveKeys.MASTER)
    def add_access_key_permissions(self, key, permissions):
        def change(b):
            merged = list(b["permitted"])
            merged.extend(p for p in permissions if p not in merged)
            b["permitted"] = merged
        return self._modify_access_key(key, change)

    @requires_key(IncisiveKeys.MASTER)
    def remove_access_key_permissions(self, key, permissions):
        def change(b):
            b["permitted"] = [p for p in b["permitted"] if p not in set(permissions)]
        return self._modify_access_key(key, change)

    @requires_key(IncisiveKeys.MASTER)
    def update_access_key_permissions(self, key, permissions):
        return self._modify_access_key(key, lambda b: b.update(permitted=permissions))

    @requires_key(IncisiveKeys.MASTER)
    def update_access_key_options(self, key, options):
        return self._modify_access_key(key, lambda b: b.update(options=options))

    @requires_key(IncisiveKeys.MASTER)
    def update_access_key_full(self, key, name, is_active, permitted, options):
        data = self._access_key_body(name, is_active, permitted, options)
        return self._request(HTTPMethods.POST, self.project_url("keys", key), self.master_key,
                             timeout=self.post_timeout, data=data).json()

    @requires_key(IncisiveKeys.MASTER)
    def revoke_access_key(self, key):
        return _json_or_none(self._request(HTTPMethods.POST,
                                           self.project_url("keys", key, "revoke"),
                                           self.master_key, timeout=self.post_timeout))

    @requires_key(IncisiveKeys.MASTER)
    def unrevoke_access_key(self, key):
        return _json_or_none(self._request(HTTPMethods.POST,
                                           self.project_url("keys", key, "unrevoke"),
                                           self.master_key, timeout=self.post_timeout))

    @requires_key(IncisiveKeys.MASTER)
    def delete_access_key(self, key):
        self._request(HTTPMethods.DELETE, self.project_url("keys", key), self.master_key,
                      timeout=self.post_timeout)
        return True
