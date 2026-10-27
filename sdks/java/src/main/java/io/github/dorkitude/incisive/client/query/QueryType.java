package io.github.dorkitude.incisive.client.query;

import java.util.Locale;

/** Simple analysis types; {@link #toString()} is the URL path segment / analysis_type value. */
public enum QueryType {
    COUNT("count"),
    COUNT_UNIQUE("count_unique"),
    MINIMUM("minimum"),
    MAXIMUM("maximum"),
    AVERAGE("average"),
    MEDIAN("median"),
    PERCENTILE("percentile"),
    SUM("sum"),
    SELECT_UNIQUE("select_unique"),
    STANDARD_DEVIATION("standard_deviation");

    private final String text;

    QueryType(String text) {
        this.text = text;
    }

    @Override
    public String toString() {
        return text;
    }

    /** Parses either the enum name or the wire name, case-insensitively. */
    static QueryType valueOfIgnoreCase(String name) {
        return valueOf(name.trim().toUpperCase(Locale.US));
    }
}
