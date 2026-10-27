package io.github.dorkitude.incisive.client.query;

import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;

/** {@code "timeframe": "this_7_days"} plus an optional {@code "timezone"}. */
public class RelativeTimeframe implements Timeframe {
    private final String relativeTimeframe;
    private final String timezone;

    public RelativeTimeframe(String relativeTimeframe) {
        this(relativeTimeframe, null);
    }

    public RelativeTimeframe(String relativeTimeframe, String timezone) {
        if (relativeTimeframe == null || relativeTimeframe.trim().isEmpty()) {
            throw new IllegalArgumentException("'relativeTimeframe' argument must be specified and not empty.");
        }
        this.relativeTimeframe = relativeTimeframe;
        this.timezone = (timezone == null || timezone.trim().isEmpty()) ? null : timezone;
    }

    public String getTimeframe() {
        return relativeTimeframe;
    }

    public String getTimezone() {
        return timezone;
    }

    @Override
    public Map<String, Object> constructTimeframeArgs() {
        Map<String, Object> args = new LinkedHashMap<>();
        args.put(QueryConstants.TIMEFRAME, relativeTimeframe);
        if (timezone != null) {
            args.put(QueryConstants.TIMEZONE, timezone);
        }
        return args;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof RelativeTimeframe)) return false;
        RelativeTimeframe t = (RelativeTimeframe) o;
        return relativeTimeframe.equals(t.relativeTimeframe) && Objects.equals(timezone, t.timezone);
    }

    @Override
    public int hashCode() {
        return Objects.hash(relativeTimeframe, timezone);
    }

    @Override
    public String toString() {
        return "RelativeTimeframe[" + relativeTimeframe + (timezone == null ? "" : " " + timezone) + "]";
    }
}
