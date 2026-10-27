package io.github.dorkitude.incisive.client.query.result;

import java.util.Collections;
import java.util.List;

/** A list result (select_unique, funnel steps, ...). */
public class ListResult extends QueryResult {
    private final List<QueryResult> results;

    public ListResult(List<? extends QueryResult> results) {
        this.results = Collections.unmodifiableList(results);
    }

    @Override
    public boolean isListResult() {
        return true;
    }

    @Override
    public List<QueryResult> getListResults() {
        return results;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof ListResult && ((ListResult) o).results.equals(results);
    }

    @Override
    public int hashCode() {
        return results.hashCode();
    }

    @Override
    public String toString() {
        return "ListResult" + results;
    }
}
