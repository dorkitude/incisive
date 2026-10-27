package io.github.dorkitude.incisive.client.query;

import java.util.Map;

/** A query timeframe; contributes {@code timeframe} (and possibly {@code timezone}) arguments. */
public interface Timeframe {
    Map<String, Object> constructTimeframeArgs();
}
