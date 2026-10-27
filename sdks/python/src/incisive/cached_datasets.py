"""Cached datasets: ``/projects/{pid}/datasets``."""

import json

from incisive.api import HTTPMethods
from incisive.saved_queries import _json_or_placeholder
from incisive.utilities import IncisiveKeys, requires_key


class CachedDatasetsInterface:
    def __init__(self, api):
        self.api = api

    def _url(self, *segments):
        return self.api.project_url("datasets", *segments)

    def _call(self, method, url, key, **kwargs):
        return _json_or_placeholder(self.api._request(method, url, key, **kwargs))

    def _get_read_key(self):
        return self.api.read_key

    def _get_write_key(self):
        return self.api.write_key

    def _get_master_key(self):
        return self.api.master_key

    @requires_key(IncisiveKeys.READ)
    def all(self):
        """List datasets. Uses the read key (see README for the difference from upstream)."""
        return self._call(HTTPMethods.GET, self._url(), self.api.read_key)

    @requires_key(IncisiveKeys.READ)
    def get(self, dataset_name):
        return self._call(HTTPMethods.GET, self._url(dataset_name), self.api.read_key)

    @requires_key(IncisiveKeys.MASTER)
    def create(self, dataset_name, query, index_by, display_name):
        payload = {"query": query, "index_by": index_by, "display_name": display_name}
        return self._call(HTTPMethods.PUT, self._url(dataset_name), self.api.master_key,
                          data=json.dumps(payload))

    @requires_key(IncisiveKeys.READ)
    def results(self, dataset_name, index_by, timeframe):
        params = {
            "index_by": index_by if isinstance(index_by, str) else json.dumps(index_by),
            "timeframe": timeframe if isinstance(timeframe, str) else json.dumps(timeframe),
        }
        return self._call(HTTPMethods.GET, self._url(dataset_name, "results"),
                          self.api.read_key, params=params)

    @requires_key(IncisiveKeys.MASTER)
    def delete(self, dataset_name):
        self._call(HTTPMethods.DELETE, self._url(dataset_name), self.api.master_key)
        return True
