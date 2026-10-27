package io.github.dorkitude.incisive.client;

import java.io.IOException;

/**
 * An event store that also persists the remaining-attempts map (JSON) per project and collection,
 * so events that keep failing are eventually dropped.
 */
public interface IncisiveAttemptCountingEventStore extends IncisiveEventStore {
    String getAttempts(String projectId, String eventCollection) throws IOException;

    void setAttempts(String projectId, String eventCollection, String attemptsString) throws IOException;
}
