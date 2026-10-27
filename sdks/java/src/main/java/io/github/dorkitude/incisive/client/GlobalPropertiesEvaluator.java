package io.github.dorkitude.incisive.client;

import java.util.Map;

/** Supplies per-collection properties merged into every event at send/queue time. */
public interface GlobalPropertiesEvaluator {
    Map<String, Object> getGlobalProperties(String eventCollection);
}
