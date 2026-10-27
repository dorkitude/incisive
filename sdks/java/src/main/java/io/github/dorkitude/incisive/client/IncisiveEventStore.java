package io.github.dorkitude.incisive.client;

import java.io.IOException;
import java.util.List;
import java.util.Map;

/** Storage for queued events awaiting a batch upload. Events are stored as JSON strings. */
public interface IncisiveEventStore {
    /** Stores an event and returns an opaque handle for it. */
    Object store(String projectId, String eventCollection, String event) throws IOException;

    String get(Object handle) throws IOException;

    void remove(Object handle) throws IOException;

    /** Handles of all stored events for a project, grouped by collection. */
    Map<String, List<Object>> getHandles(String projectId) throws IOException;
}
