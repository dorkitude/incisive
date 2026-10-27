package io.github.dorkitude.incisive.client;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * In-memory event store (the default). Keeps at most {@code maxEventsPerCollection} events per
 * project/collection pair, evicting the oldest. Handles are {@link Long}s.
 */
public class RamEventStore implements IncisiveAttemptCountingEventStore {
    private long nextId = 0;
    private final Map<String, Map<String, List<Long>>> idsByProject = new HashMap<>();
    private final Map<Long, String> events = new HashMap<>();
    private final Map<String, Map<String, String>> attempts = new HashMap<>();
    private int maxEventsPerCollection = 10000;

    @Override
    public synchronized Object store(String projectId, String eventCollection, String event) {
        List<Long> ids = idsByProject
                .computeIfAbsent(projectId, k -> new LinkedHashMap<>())
                .computeIfAbsent(eventCollection, k -> new ArrayList<>());
        while (ids.size() >= maxEventsPerCollection && !ids.isEmpty()) {
            events.remove(ids.remove(0));
        }
        long id = nextId++;
        events.put(id, event);
        ids.add(id);
        return id;
    }

    @Override
    public synchronized String get(Object handle) {
        return events.get(toId(handle));
    }

    @Override
    public synchronized void remove(Object handle) {
        events.remove(toId(handle));
    }

    @Override
    public synchronized Map<String, List<Object>> getHandles(String projectId) {
        Map<String, List<Object>> result = new LinkedHashMap<>();
        Map<String, List<Long>> collections = idsByProject.get(projectId);
        if (collections == null) {
            return result;
        }
        for (Map.Entry<String, List<Long>> entry : collections.entrySet()) {
            List<Object> handles = new ArrayList<>();
            Iterator<Long> it = entry.getValue().iterator();
            while (it.hasNext()) {
                Long id = it.next();
                if (events.containsKey(id)) {
                    handles.add(id);
                } else {
                    it.remove();
                }
            }
            if (!handles.isEmpty()) {
                result.put(entry.getKey(), handles);
            }
        }
        return result;
    }

    @Override
    public synchronized String getAttempts(String projectId, String eventCollection) {
        Map<String, String> byCollection = attempts.get(projectId);
        return byCollection == null ? null : byCollection.get(eventCollection);
    }

    @Override
    public synchronized void setAttempts(String projectId, String eventCollection, String attemptsString) {
        attempts.computeIfAbsent(projectId, k -> new HashMap<>()).put(eventCollection, attemptsString);
    }

    public synchronized void setMaxEventsPerCollection(int maxEventsPerCollection) {
        this.maxEventsPerCollection = maxEventsPerCollection;
    }

    /** Drops all stored events and attempt counts. */
    public synchronized void clear() {
        idsByProject.clear();
        events.clear();
        attempts.clear();
    }

    private static Long toId(Object handle) {
        if (handle instanceof Long) {
            return (Long) handle;
        }
        throw new IllegalArgumentException("Expected handle to be a Long, but was: "
                + (handle == null ? "null" : handle.getClass().getName()));
    }
}
