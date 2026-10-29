"""The main client object."""

import base64
import copy
import json

from incisive import cached_datasets, exceptions, persistence_strategies, saved_queries
from incisive.api import IncisiveApi
from incisive.persistence_strategies import BasePersistenceStrategy

# The reserved metadata object every event may carry on the wire.
RESERVED_PROPERTY = "incisive"


class Event:
    """One event bound for a collection; ``timestamp`` (a datetime) overrides the event time."""

    def __init__(self, project_id, event_collection, event_body, timestamp=None):
        self.project_id = project_id
        self.event_collection = event_collection
        self.event_body = event_body
        self.timestamp = timestamp

    def to_dict(self):
        body = copy.deepcopy(self.event_body)
        if self.timestamp:
            meta = body.setdefault(RESERVED_PROPERTY, {})
            meta["timestamp"] = self.timestamp.isoformat()
        return body

    def to_json(self):
        return json.dumps(self.to_dict())


# Parameters that are JSON-encoded when sent as query-string values.
_ALWAYS_JSON = ("filters", "analyses", "steps", "property_names")
# Parameters that are JSON-encoded only when given as a list/dict.
_JSON_IF_STRUCTURED = ("timeframe", "group_by")
_PARAM_ORDER = ("event_collection", "timeframe", "timezone", "interval", "filters", "group_by",
                "order_by", "limit", "target_property", "latest", "email", "analyses", "steps",
                "property_names", "percentile", "max_age")


class IncisiveClient:
    """Entry point. Needs a project id plus whichever keys the operations you use require.

    GET and POST requests time out after 305 seconds by default.
    """

    def __init__(self, project_id, write_key=None, read_key=None, persistence_strategy=None,
                 api_class=IncisiveApi, get_timeout=305, post_timeout=305, master_key=None,
                 base_url=None, api_version=None):
        self.check_project_id(project_id)
        self.api = api_class(project_id, write_key=write_key, read_key=read_key,
                             get_timeout=get_timeout, post_timeout=post_timeout,
                             master_key=master_key, base_url=base_url, api_version=api_version)
        if persistence_strategy is not None and not isinstance(persistence_strategy,
                                                               BasePersistenceStrategy):
            raise exceptions.InvalidPersistenceStrategyError()
        if persistence_strategy is None:
            persistence_strategy = persistence_strategies.DirectPersistenceStrategy(self.api)
        self.project_id = project_id
        self.persistence_strategy = persistence_strategy
        self.get_timeout = get_timeout
        self.post_timeout = post_timeout
        self.saved_queries = saved_queries.SavedQueriesInterface(self.api)
        self.cached_datasets = cached_datasets.CachedDatasetsInterface(self.api)

    @staticmethod
    def check_project_id(project_id):
        if not project_id or not isinstance(project_id, str):
            raise exceptions.InvalidProjectIdError(project_id)

    # -- recording ----------------------------------------------------------------

    def add_event(self, event_collection, event_body, timestamp=None):
        """Record one event (POST /events/{collection})."""
        self.persistence_strategy.persist(
            Event(self.project_id, event_collection, event_body, timestamp=timestamp))

    def add_events(self, events):
        """Record a batch: ``{"collection": [event, ...], ...}`` (POST /events).

        Returns the server's per-event result body.
        """
        return self.persistence_strategy.batch_persist(events)

    def generate_image_beacon(self, event_collection, event_body, timestamp=None):
        """URL that records the event when fetched with GET (api_key + base64 data params)."""
        event_json = Event(self.project_id, event_collection, event_body,
                           timestamp=timestamp).to_json()
        write_key = self.api.write_key
        if isinstance(write_key, bytes):
            write_key = write_key.decode("utf-8")
        data = base64.b64encode(event_json.encode("utf-8")).decode("ascii")
        return "{0}?api_key={1}&data={2}".format(
            self.api.project_url("events", event_collection), write_key, data)

    # -- deletes / discovery ------------------------------------------------------

    def delete_events(self, event_collection, timeframe=None, timezone=None, filters=None):
        """DELETE /events/{collection}; with no filters/timeframe the whole collection goes."""
        params = self.get_params(timeframe=timeframe, timezone=timezone, filters=filters)
        return self.api.delete_events(event_collection, params)

    def get_collection(self, event_collection):
        return self.api.get_collection(event_collection)

    def get_all_collections(self):
        return self.api.get_all_collections()

    # -- access keys --------------------------------------------------------------

    def create_access_key(self, name, is_active=True, permitted=None, options=None):
        return self.api.create_access_key(name=name, is_active=is_active,
                                          permitted=permitted or [], options=options or {})

    def list_access_keys(self):
        return self.api.list_access_keys()

    def get_access_key(self, key):
        return self.api.get_access_key(key)

    def update_access_key_name(self, key, name):
        return self.api.update_access_key_name(key, name)

    def add_access_key_permissions(self, key, permissions):
        return self.api.add_access_key_permissions(key, permissions)

    def remove_access_key_permissions(self, key, permissions):
        return self.api.remove_access_key_permissions(key, permissions)

    def update_access_key_permissions(self, key, permissions):
        return self.api.update_access_key_permissions(key, permissions)

    def update_access_key_options(self, key, options):
        return self.api.update_access_key_options(key, options)

    def update_access_key_full(self, key, name, is_active, permitted, options):
        return self.api.update_access_key_full(key, name, is_active, permitted, options)

    def revoke_access_key(self, key):
        return self.api.revoke_access_key(key)

    def unrevoke_access_key(self, key):
        return self.api.unrevoke_access_key(key)

    def delete_access_key(self, key):
        return self.api.delete_access_key(key)

    # -- analyses -----------------------------------------------------------------

    def _simple(self, analysis, event_collection, target_property=None, timeframe=None,
                timezone=None, interval=None, filters=None, group_by=None, order_by=None,
                max_age=None, limit=None, percentile=None):
        params = self.get_params(event_collection=event_collection, timeframe=timeframe,
                                 timezone=timezone, interval=interval, filters=filters,
                                 group_by=group_by, order_by=order_by,
                                 target_property=target_property, max_age=max_age,
                                 limit=limit, percentile=percentile)
        return self.api.query(analysis, params)

    def count(self, event_collection, timeframe=None, timezone=None, interval=None,
              filters=None, group_by=None, order_by=None, max_age=None, limit=None):
        return self._simple("count", event_collection, timeframe=timeframe, timezone=timezone,
                            interval=interval, filters=filters, group_by=group_by,
                            order_by=order_by, max_age=max_age, limit=limit)

    def sum(self, event_collection, target_property, timeframe=None, timezone=None,
            interval=None, filters=None, group_by=None, order_by=None, max_age=None, limit=None):
        return self._simple("sum", event_collection, target_property, timeframe, timezone,
                            interval, filters, group_by, order_by, max_age, limit)

    def minimum(self, event_collection, target_property, timeframe=None, timezone=None,
                interval=None, filters=None, group_by=None, order_by=None, max_age=None,
                limit=None):
        return self._simple("minimum", event_collection, target_property, timeframe, timezone,
                            interval, filters, group_by, order_by, max_age, limit)

    def maximum(self, event_collection, target_property, timeframe=None, timezone=None,
                interval=None, filters=None, group_by=None, order_by=None, max_age=None,
                limit=None):
        return self._simple("maximum", event_collection, target_property, timeframe, timezone,
                            interval, filters, group_by, order_by, max_age, limit)

    def average(self, event_collection, target_property, timeframe=None, timezone=None,
                interval=None, filters=None, group_by=None, order_by=None, max_age=None,
                limit=None):
        return self._simple("average", event_collection, target_property, timeframe, timezone,
                            interval, filters, group_by, order_by, max_age, limit)

    def median(self, event_collection, target_property, timeframe=None, timezone=None,
               interval=None, filters=None, group_by=None, order_by=None, max_age=None,
               limit=None):
        return self._simple("median", event_collection, target_property, timeframe, timezone,
                            interval, filters, group_by, order_by, max_age, limit)

    def percentile(self, event_collection, target_property, percentile, timeframe=None,
                   timezone=None, interval=None, filters=None, group_by=None, order_by=None,
                   max_age=None, limit=None):
        return self._simple("percentile", event_collection, target_property, timeframe,
                            timezone, interval, filters, group_by, order_by, max_age, limit,
                            percentile=percentile)

    def count_unique(self, event_collection, target_property, timeframe=None, timezone=None,
                     interval=None, filters=None, group_by=None, order_by=None, max_age=None,
                     limit=None):
        return self._simple("count_unique", event_collection, target_property, timeframe,
                            timezone, interval, filters, group_by, order_by, max_age, limit)

    def select_unique(self, event_collection, target_property, timeframe=None, timezone=None,
                      interval=None, filters=None, group_by=None, order_by=None, max_age=None,
                      limit=None):
        return self._simple("select_unique", event_collection, target_property, timeframe,
                            timezone, interval, filters, group_by, order_by, max_age, limit)

    def extraction(self, event_collection, timeframe=None, timezone=None, filters=None,
                   latest=None, email=None, property_names=None):
        """Raw events (or, with ``email``, an asynchronous emailed extraction)."""
        params = self.get_params(event_collection=event_collection, timeframe=timeframe,
                                 timezone=timezone, filters=filters, latest=latest, email=email,
                                 property_names=property_names)
        return self.api.query("extraction", params)

    def funnel(self, steps, timeframe=None, timezone=None, max_age=None, all_keys=False):
        """Funnel over ``steps``; ``all_keys=True`` returns the whole body (result, steps, actors)."""
        params = self.get_params(steps=steps, timeframe=timeframe, timezone=timezone,
                                 max_age=max_age)
        return self.api.query("funnel", params, all_keys=all_keys)

    def multi_analysis(self, event_collection, analyses, timeframe=None, interval=None,
                       timezone=None, filters=None, group_by=None, order_by=None, max_age=None,
                       limit=None):
        params = self.get_params(event_collection=event_collection, timeframe=timeframe,
                                 interval=interval, timezone=timezone, filters=filters,
                                 group_by=group_by, order_by=order_by, analyses=analyses,
                                 max_age=max_age, limit=limit)
        return self.api.query("multi_analysis", params)

    def get_params(self, event_collection=None, timeframe=None, timezone=None, interval=None,
                   filters=None, group_by=None, order_by=None, target_property=None,
                   latest=None, email=None, analyses=None, steps=None, property_names=None,
                   percentile=None, max_age=None, limit=None):
        """Build the query-string dict. Falsy values are omitted.

        filters/analyses/steps/property_names are JSON-encoded; timeframe and group_by
        are JSON-encoded when structured; order_by is always a JSON array.
        """
        given = dict(event_collection=event_collection, timeframe=timeframe, timezone=timezone,
                     interval=interval, filters=filters, group_by=group_by, order_by=order_by,
                     limit=limit, target_property=target_property, latest=latest, email=email,
                     analyses=analyses, steps=steps, property_names=property_names,
                     percentile=percentile, max_age=max_age)
        params = {}
        for name in _PARAM_ORDER:
            value = given[name]
            if not value:
                continue
            if name in _ALWAYS_JSON:
                value = json.dumps(value)
            elif name in _JSON_IF_STRUCTURED and isinstance(value, (dict, list)):
                value = json.dumps(value)
            elif name == "order_by":
                value = json.dumps(value if isinstance(value, list) else [value])
            params[name] = value
        return params
