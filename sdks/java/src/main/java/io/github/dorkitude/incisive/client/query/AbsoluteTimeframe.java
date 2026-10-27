package io.github.dorkitude.incisive.client.query;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;

/** {@code "timeframe": {"start": "...", "end": "..."}} with ISO-8601 strings. */
public class AbsoluteTimeframe implements Timeframe {
    private final String start;
    private final String end;

    public AbsoluteTimeframe(String start, String end) {
        this.start = start;
        this.end = end;
    }

    public String getStart() {
        return start;
    }

    public String getEnd() {
        return end;
    }

    @Override
    public Map<String, Object> constructTimeframeArgs() {
        if (start == null || end == null) {
            return Collections.emptyMap();
        }
        Map<String, Object> range = new LinkedHashMap<>();
        range.put(QueryConstants.START, start);
        range.put(QueryConstants.END, end);
        Map<String, Object> args = new LinkedHashMap<>();
        args.put(QueryConstants.TIMEFRAME, range);
        return args;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof AbsoluteTimeframe)) return false;
        AbsoluteTimeframe t = (AbsoluteTimeframe) o;
        return Objects.equals(start, t.start) && Objects.equals(end, t.end);
    }

    @Override
    public int hashCode() {
        return Objects.hash(start, end);
    }

    @Override
    public String toString() {
        return "AbsoluteTimeframe[" + start + " .. " + end + "]";
    }
}
