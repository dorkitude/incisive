package io.github.dorkitude.incisive.client;

/** A project id plus its API keys. At least one key must be set. */
public class IncisiveProject {
    private final String projectId;
    private final String writeKey;
    private final String readKey;
    private final String masterKey;

    /** Builds a project from INCISIVE_PROJECT_ID / _WRITE_KEY / _READ_KEY / _MASTER_KEY. */
    public IncisiveProject() {
        this(new Environment());
    }

    public IncisiveProject(Environment env) {
        this(env.getProjectId(), env.getWriteKey(), env.getReadKey(), env.getMasterKey());
    }

    public IncisiveProject(String projectId, String writeKey, String readKey) {
        this(projectId, writeKey, readKey, null);
    }

    public IncisiveProject(String projectId, String writeKey, String readKey, String masterKey) {
        if (projectId == null || projectId.trim().isEmpty()) {
            throw new IllegalArgumentException("Invalid project id specified: " + projectId);
        }
        if (isBlank(writeKey) && isBlank(readKey) && isBlank(masterKey)) {
            throw new IllegalArgumentException("At least one of the keys given must be non-null and non-empty.");
        }
        this.projectId = projectId;
        this.writeKey = writeKey;
        this.readKey = readKey;
        this.masterKey = masterKey;
    }

    private static boolean isBlank(String s) {
        return s == null || s.trim().isEmpty();
    }

    public String getProjectId() {
        return projectId;
    }

    public String getWriteKey() {
        return writeKey;
    }

    public String getReadKey() {
        return readKey;
    }

    public String getMasterKey() {
        return masterKey;
    }
}
