"""Sort directions for ``order_by``."""

ASCENDING = "ASC"
DESCENDING = "DESC"


def is_valid_direction(direction_string):
    return direction_string in (ASCENDING, DESCENDING)
