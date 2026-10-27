package io.github.dorkitude.incisive.client.query.result;

import io.github.dorkitude.incisive.client.query.AbsoluteTimeframe;

import java.util.Objects;

/** One {@code {"timeframe": {start, end}, "value": ...}} entry of an interval result. */
public class IntervalResultValue {
    private final AbsoluteTimeframe timeframe;
    private final QueryResult result;

    public IntervalResultValue(AbsoluteTimeframe timeframe, QueryResult result) {
        this.timeframe = timeframe;
        this.result = result;
    }

    public AbsoluteTimeframe getTimeframe() {
        return timeframe;
    }

    public QueryResult getResult() {
        return result;
    }

    @Override
    public boolean equals(Object o) {
        if (!(o instanceof IntervalResultValue)) return false;
        IntervalResultValue v = (IntervalResultValue) o;
        return Objects.equals(timeframe, v.timeframe) && Objects.equals(result, v.result);
    }

    @Override
    public int hashCode() {
        return Objects.hash(timeframe, result);
    }

    @Override
    public String toString() {
        return "IntervalResultValue[" + timeframe + " = " + result + "]";
    }
}
