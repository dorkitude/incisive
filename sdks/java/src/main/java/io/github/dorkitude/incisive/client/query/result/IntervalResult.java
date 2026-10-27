package io.github.dorkitude.incisive.client.query.result;

import java.util.Collections;
import java.util.List;

/** A result for a query with an interval: one value per sub-timeframe. */
public class IntervalResult extends QueryResult {
    private final List<IntervalResultValue> results;

    public IntervalResult(List<IntervalResultValue> results) {
        this.results = Collections.unmodifiableList(results);
    }

    @Override
    public boolean isIntervalResult() {
        return true;
    }

    @Override
    public List<IntervalResultValue> getIntervalResults() {
        return results;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof IntervalResult && ((IntervalResult) o).results.equals(results);
    }

    @Override
    public int hashCode() {
        return results.hashCode();
    }

    @Override
    public String toString() {
        return "IntervalResult" + results;
    }
}
