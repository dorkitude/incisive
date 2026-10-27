"""Shared helpers: version, headers, key requirements, path escaping."""

from functools import wraps
from urllib.parse import quote

from incisive import exceptions

VERSION = "0.1.0"


def version():
    return VERSION


def headers(api_key):
    """Standard request headers. The key goes verbatim in Authorization."""
    if isinstance(api_key, bytes):
        api_key = api_key.decode("utf-8")
    return {
        "Content-Type": "application/json",
        "Authorization": api_key or "",
        "User-Agent": "incisive-python/{0}".format(VERSION),
    }


def escape_segment(value):
    """Percent-escape one URL path segment (``/`` included)."""
    return quote(str(value), safe="")


class IncisiveKeys:
    READ = "read"
    WRITE = "write"
    MASTER = "master"


def _throw_key_missing(key, relying_on_master):
    message = (
        "The incisive API requires a {0} key to perform this operation. "
        "Please set a '{0}_key' when initializing the client."
    ).format(key)
    if relying_on_master:
        message += ' The "master_key" is set, but one should prefer the key with least privilege.'
    raise exceptions.InvalidEnvironmentError(message)


def requires_key(key_type):
    """Decorator: fail fast with InvalidEnvironmentError if the needed key is unset.

    The decorated object must provide _get_read_key/_get_write_key/_get_master_key.
    """

    def decorator(func):
        @wraps(func)
        def wrapper(self, *args, **kwargs):
            if key_type == IncisiveKeys.READ and not self._get_read_key():
                _throw_key_missing(IncisiveKeys.READ, bool(self._get_master_key()))
            elif key_type == IncisiveKeys.WRITE and not self._get_write_key():
                _throw_key_missing(IncisiveKeys.WRITE, bool(self._get_master_key()))
            elif key_type == IncisiveKeys.MASTER and not self._get_master_key():
                _throw_key_missing(IncisiveKeys.MASTER, False)
            return func(self, *args, **kwargs)

        return wrapper

    return decorator
