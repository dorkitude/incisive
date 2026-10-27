package io.github.dorkitude.incisive.client;

import java.util.Map;

/**
 * Callback that also receives the event details. For single-event operations both the plain and
 * detailed variants are invoked.
 */
public interface IncisiveDetailedCallback extends IncisiveCallback {
    void onSuccess(IncisiveProject project, String eventCollection, Map<String, Object> event,
                   Map<String, Object> reservedProperties);

    void onFailure(IncisiveProject project, String eventCollection, Map<String, Object> event,
                   Map<String, Object> reservedProperties, Exception e);
}
