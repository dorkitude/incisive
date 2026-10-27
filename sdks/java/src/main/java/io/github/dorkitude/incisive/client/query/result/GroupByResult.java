package io.github.dorkitude.incisive.client.query.result;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

/** A result for a query with group_by, keyed by group. */
public class GroupByResult extends QueryResult {
    private final Map<Group, QueryResult> results;

    public GroupByResult(Map<Group, QueryResult> results) {
        this.results = Collections.unmodifiableMap(new LinkedHashMap<>(results));
    }

    @Override
    public boolean isGroupResult() {
        return true;
    }

    @Override
    public Map<Group, QueryResult> getGroupResults() {
        return results;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof GroupByResult && ((GroupByResult) o).results.equals(results);
    }

    @Override
    public int hashCode() {
        return results.hashCode();
    }

    @Override
    public String toString() {
        return "GroupByResult" + results;
    }
}
