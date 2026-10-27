package io.github.dorkitude.incisive.client.query;

/** Filter operators; {@link #toString()} is the wire value. */
public enum FilterOperator {
    EQUAL_TO("eq"),
    NOT_EQUAL("ne"),
    LESS_THAN("lt"),
    LESS_THAN_EQUAL("lte"),
    GREATER_THAN("gt"),
    GREATER_THAN_EQUAL("gte"),
    EXISTS("exists"),
    IN("in"),
    CONTAINS("contains"),
    NOT_CONTAINS("not_contains"),
    WITHIN("within"),
    REGEX("regex");

    private final String text;

    FilterOperator(String text) {
        this.text = text;
    }

    @Override
    public String toString() {
        return text;
    }

    /** Looks up an operator by wire value; null if unknown. */
    public static FilterOperator fromString(String operator) {
        for (FilterOperator op : values()) {
            if (op.text.equals(operator)) {
                return op;
            }
        }
        return null;
    }
}
