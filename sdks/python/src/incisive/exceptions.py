"""Exception types raised by the incisive client."""


class BaseIncisiveClientError(Exception):
    """Base class for every error raised by this package."""

    _message = ""

    def __str__(self):
        return self._message


class InvalidProjectIdError(BaseIncisiveClientError):
    def __init__(self, project_id):
        super().__init__(project_id)
        self.project_id = project_id
        self._message = "Invalid project id: {0}".format(project_id)


class InvalidPersistenceStrategyError(BaseIncisiveClientError):
    def __init__(self):
        super().__init__()
        self._message = (
            "Invalid persistence strategy. A persistence strategy must be an "
            "instance of BasePersistenceStrategy."
        )


class IncisiveApiError(BaseIncisiveClientError):
    """Raised for any non-2xx response from the server.

    ``api_error`` holds the decoded error body (``{"message", "error_code", ...}``),
    ``status_code`` the HTTP status (None if unknown).
    """

    def __init__(self, api_error, status_code=None):
        super().__init__(api_error)
        if not isinstance(api_error, dict):
            api_error = {"message": str(api_error), "error_code": str(status_code)}
        self.api_error = api_error
        self.status_code = status_code
        self.message = api_error.get("message")
        self.error_code = api_error.get("error_code")
        msg = "Error from incisive API. Details:\n Message: {0}\nCode: {1}".format(
            self.message, self.error_code
        )
        if "stacktrace_id" in api_error:
            msg = "{0}\nStacktrace ID: {1}".format(msg, api_error["stacktrace_id"])
        if "unique_id" in api_error:
            msg = "{0}\nUnique ID: {1}".format(msg, api_error["unique_id"])
        self._message = msg


class InvalidEnvironmentError(BaseIncisiveClientError):
    def __init__(self, message):
        super().__init__(message)
        self._message = message
