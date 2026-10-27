"""incisive: Python client for the incisive analytics server.

Module-level usage configures one shared client lazily::

    import incisive
    incisive.project_id = "my-project"
    incisive.write_key = "..."
    incisive.add_event("purchases", {"price": 10})

Unset settings fall back to the INCISIVE_PROJECT_ID, INCISIVE_WRITE_KEY,
INCISIVE_READ_KEY, INCISIVE_MASTER_KEY and INCISIVE_BASE_URL environment variables.
The shared client is built on first use; set ``incisive._client = None`` to rebuild it.
"""

import os

from incisive.client import Event, IncisiveClient
from incisive.exceptions import (
    BaseIncisiveClientError,
    IncisiveApiError,
    InvalidEnvironmentError,
    InvalidPersistenceStrategyError,
    InvalidProjectIdError,
)
from incisive.utilities import VERSION as __version__

__all__ = [
    "IncisiveClient", "Event", "BaseIncisiveClientError", "IncisiveApiError",
    "InvalidEnvironmentError", "InvalidPersistenceStrategyError", "InvalidProjectIdError",
]

_client = None
project_id = None
write_key = None
read_key = None
master_key = None
base_url = None


def _initialize_client_from_environment():
    """Build the shared client from module globals / environment, once."""
    global _client, project_id, write_key, read_key, master_key, base_url
    if _client is not None:
        return
    project_id = project_id or os.environ.get("INCISIVE_PROJECT_ID")
    write_key = write_key or os.environ.get("INCISIVE_WRITE_KEY")
    read_key = read_key or os.environ.get("INCISIVE_READ_KEY")
    master_key = master_key or os.environ.get("INCISIVE_MASTER_KEY")
    base_url = base_url or os.environ.get("INCISIVE_BASE_URL")
    if not project_id:
        raise InvalidEnvironmentError(
            "Please set the INCISIVE_PROJECT_ID environment variable or set incisive.project_id!")
    _client = IncisiveClient(project_id, write_key=write_key, read_key=read_key,
                             master_key=master_key, base_url=base_url)


def _shared():
    _initialize_client_from_environment()
    return _client


def add_event(event_collection, body, timestamp=None):
    _shared().add_event(event_collection, body, timestamp=timestamp)


def add_events(events):
    return _shared().add_events(events)


def generate_image_beacon(event_collection, body, timestamp=None):
    return _shared().generate_image_beacon(event_collection, body, timestamp=timestamp)


def count(event_collection, timeframe=None, timezone=None, interval=None, filters=None,
          group_by=None, order_by=None, max_age=None, limit=None):
    return _shared().count(event_collection, timeframe=timeframe, timezone=timezone,
                           interval=interval, filters=filters, group_by=group_by,
                           order_by=order_by, max_age=max_age, limit=limit)


def _targeted(name):
    def analysis(event_collection, target_property, timeframe=None, timezone=None,
                 interval=None, filters=None, group_by=None, order_by=None, max_age=None,
                 limit=None):
        return getattr(_shared(), name)(
            event_collection, target_property, timeframe=timeframe, timezone=timezone,
            interval=interval, filters=filters, group_by=group_by, order_by=order_by,
            max_age=max_age, limit=limit)

    analysis.__name__ = name
    analysis.__doc__ = "Module-level shortcut for IncisiveClient.{0}.".format(name)
    return analysis


sum = _targeted("sum")  # noqa: A001 - mirrors the established module API
minimum = _targeted("minimum")
maximum = _targeted("maximum")
average = _targeted("average")
median = _targeted("median")
count_unique = _targeted("count_unique")
select_unique = _targeted("select_unique")


def percentile(event_collection, target_property, percentile, timeframe=None, timezone=None,
               interval=None, filters=None, group_by=None, order_by=None, max_age=None,
               limit=None):
    return _shared().percentile(event_collection, target_property, percentile,
                                timeframe=timeframe, timezone=timezone, interval=interval,
                                filters=filters, group_by=group_by, order_by=order_by,
                                max_age=max_age, limit=limit)


def extraction(event_collection, timeframe=None, timezone=None, filters=None, latest=None,
               email=None, property_names=None):
    return _shared().extraction(event_collection, timeframe=timeframe, timezone=timezone,
                                filters=filters, latest=latest, email=email,
                                property_names=property_names)


def funnel(*args, **kwargs):
    return _shared().funnel(*args, **kwargs)


def multi_analysis(event_collection, analyses, timeframe=None, interval=None, timezone=None,
                   filters=None, group_by=None, order_by=None, max_age=None, limit=None):
    return _shared().multi_analysis(event_collection, analyses, timeframe=timeframe,
                                    interval=interval, timezone=timezone, filters=filters,
                                    group_by=group_by, order_by=order_by, max_age=max_age,
                                    limit=limit)


def delete_events(*args, **kwargs):
    return _shared().delete_events(*args, **kwargs)


def get_collection(*args, **kwargs):
    return _shared().get_collection(*args, **kwargs)


def get_all_collections():
    return _shared().get_all_collections()


def create_access_key(name, is_active=True, permitted=None, options=None):
    return _shared().create_access_key(name=name, is_active=is_active,
                                       permitted=permitted, options=options)


def list_access_keys():
    return _shared().list_access_keys()


def get_access_key(key):
    return _shared().get_access_key(key)


def update_access_key_name(key, name):
    return _shared().update_access_key_name(key, name)


def add_access_key_permissions(key, permissions):
    return _shared().add_access_key_permissions(key, permissions)


def remove_access_key_permissions(key, permissions):
    return _shared().remove_access_key_permissions(key, permissions)


def update_access_key_permissions(key, permissions):
    return _shared().update_access_key_permissions(key, permissions)


def update_access_key_options(key, options):
    return _shared().update_access_key_options(key, options)


def update_access_key_full(key, name, is_active, permitted, options):
    return _shared().update_access_key_full(key, name, is_active, permitted, options)


def revoke_access_key(key):
    return _shared().revoke_access_key(key)


def unrevoke_access_key(key):
    return _shared().unrevoke_access_key(key)


def delete_access_key(key):
    return _shared().delete_access_key(key)
