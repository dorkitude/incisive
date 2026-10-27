package io.github.dorkitude.incisive.client.query;

import java.util.Locale;

/** Cached query refresh rates in seconds: 0 (no caching) or between 4 and 24 hours. */
public final class RefreshRate {
    private RefreshRate() {}

    public static final int NO_CACHING = 0;
    public static final int MIN = 14400;
    public static final int MAX = 86400;

    public static int fromHours(int hours) {
        int rate = hours * 3600;
        validateRefreshRate(rate);
        return rate;
    }

    static void validateRefreshRate(int refreshRate) {
        if (refreshRate != NO_CACHING && (refreshRate < MIN || refreshRate > MAX)) {
            throw new IllegalArgumentException(String.format(Locale.US, "refreshRate '%d' is out of range.", refreshRate));
        }
    }
}
