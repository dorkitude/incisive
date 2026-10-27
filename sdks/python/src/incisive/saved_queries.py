"""Saved queries: ``/projects/{pid}/queries/saved``."""

import json
from collections.abc import Mapping

from incisive.api import HTTPMethods
from incisive.utilities import IncisiveKeys, requires_key


def _json_or_placeholder(response):
    try:
        return response.json()
    except ValueError:
        return "No JSON available."


class SavedQueriesInterface:
    def __init__(self, api):
        self.api = api

    def _url(self, *segments):
        return self.api.project_url("queries", "saved", *segments)

    def _call(self, method, url, key, **kwargs):
        return _json_or_placeholder(self.api._request(method, url, key, **kwargs))

    def _get_read_key(self):
        return self.api.read_key

    def _get_write_key(self):
        return self.api.write_key

    def _get_master_key(self):
        return self.api.master_key

    @requires_key(IncisiveKeys.MASTER)
    def all(self):
        """List every saved query (master key)."""
        return self._call(HTTPMethods.GET, self._url(), self.api.master_key)

    @requires_key(IncisiveKeys.MASTER)
    def get(self, query_name):
        """Fetch one saved query definition (master key)."""
        return self._call(HTTPMethods.GET, self._url(query_name), self.api.master_key)

    @requires_key(IncisiveKeys.READ)
    def results(self, query_name):
        """Run/fetch a saved query's results (read key). Returns the whole body."""
        return self._call(HTTPMethods.GET, self._url(query_name, "result"), self.api.read_key)

    @requires_key(IncisiveKeys.MASTER)
    def create(self, query_name, saved_query):
        """PUT a saved query. ``saved_query`` may be a dict or an already-encoded JSON string."""
        payload = saved_query if isinstance(saved_query, str) else json.dumps(saved_query)
        return self._call(HTTPMethods.PUT, self._url(query_name), self.api.master_key,
                          data=payload)

    @requires_key(IncisiveKeys.MASTER)
    def update_full(self, query_name, saved_query_full_definition):
        """Replace the whole definition (alias of create)."""
        return self.create(query_name, saved_query_full_definition)

    @requires_key(IncisiveKeys.MASTER)
    def update(self, query_name, saved_query_attributes):
        """Fetch the current definition, deep-merge the given attributes, PUT it back.

        Server-computed fields (urls, run_information, ...) are dropped, as are
        empty/None values inside ``query``; ``metadata`` is kept when present.
        """
        old = self.get(query_name)
        new = {
            "query_name": old["query_name"],
            "refresh_rate": old["refresh_rate"],
            "query": {k: v for k, v in old["query"].items() if v},
        }
        if old.get("metadata"):
            new["metadata"] = old["metadata"]
        new = self._deep_update(new, saved_query_attributes)
        return self.create(query_name, new)

    @requires_key(IncisiveKeys.MASTER)
    def delete(self, query_name):
        self._call(HTTPMethods.DELETE, self._url(query_name), self.api.master_key)
        return True

    @staticmethod
    def _deep_update(mapping, updates):
        if not isinstance(mapping, Mapping):
            mapping = {}
        for key, value in updates.items():
            if isinstance(value, Mapping):
                mapping[key] = SavedQueriesInterface._deep_update(mapping.get(key, {}), value)
            else:
                mapping[key] = value
        return mapping
