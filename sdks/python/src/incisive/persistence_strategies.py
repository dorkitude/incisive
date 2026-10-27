"""How added events are persisted. Only the direct (synchronous HTTP) strategy is functional."""


class BasePersistenceStrategy:
    def persist(self, event):
        raise NotImplementedError()

    def batch_persist(self, events):
        raise NotImplementedError()


class DirectPersistenceStrategy(BasePersistenceStrategy):
    """Sends events straight to the server, in-line."""

    def __init__(self, api):
        super().__init__()
        self.api = api

    def persist(self, event):
        self.api.post_event(event)

    def batch_persist(self, events):
        return self.api.post_events(events)


class RedisPersistenceStrategy(BasePersistenceStrategy):
    """Placeholder, not implemented (kept for API parity)."""


class FilePersistenceStrategy(BasePersistenceStrategy):
    """Placeholder, not implemented (kept for API parity)."""
