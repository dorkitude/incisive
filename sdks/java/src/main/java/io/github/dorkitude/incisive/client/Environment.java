package io.github.dorkitude.incisive.client;

import java.util.Map;

/**
 * Reads configuration from environment variables: INCISIVE_PROJECT_ID, INCISIVE_WRITE_KEY,
 * INCISIVE_READ_KEY, INCISIVE_MASTER_KEY, INCISIVE_BASE_URL.
 */
public class Environment {
    private final Map<String, String> values;

    public Environment() {
        this(System.getenv());
    }

    /** Uses an explicit map instead of the process environment (handy for tests). */
    public Environment(Map<String, String> values) {
        this.values = values;
    }

    public String getProjectId() {
        return get("INCISIVE_PROJECT_ID");
    }

    public String getWriteKey() {
        return get("INCISIVE_WRITE_KEY");
    }

    public String getReadKey() {
        return get("INCISIVE_READ_KEY");
    }

    public String getMasterKey() {
        return get("INCISIVE_MASTER_KEY");
    }

    public String getBaseUrl() {
        return get("INCISIVE_BASE_URL");
    }

    private String get(String name) {
        String v = values.get(name);
        return (v == null || v.trim().isEmpty()) ? null : v;
    }
}
