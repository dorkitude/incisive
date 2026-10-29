package io.github.dorkitude.incisive.client;

/** Shared constants. */
public final class IncisiveConstants {
    private IncisiveConstants() {}

    /** Default server address (a locally running incisive server). */
    public static final String DEFAULT_BASE_URL = "http://localhost:8080";
    /** Default HTTP API version path segment. */
    public static final String DEFAULT_API_VERSION = "1.0";

    /** Name of the reserved metadata object on every event (wire protocol literal). */
    public static final String RESERVED_PROPERTY = "incisive";

    /**
     * Key under which {@link IncisiveJsonHandler#readJson} stores a JSON array root, since the
     * handler contract returns a map.
     */
    public static final String FAKE_JSON_ROOT = "io.github.dorkitude.incisive.client.__fake_root";

    static final int MAX_EVENT_DEPTH = 1000;
    static final int DEFAULT_MAX_ATTEMPTS = 3;
    static final int DEFAULT_CONNECT_TIMEOUT = 30000;
    static final int DEFAULT_READ_TIMEOUT = 30000;

    static final String NAME_PARAM = "name";
    static final String SUCCESS_PARAM = "success";
    static final String ERROR_PARAM = "error";
    static final String DESCRIPTION_PARAM = "description";
    static final String INVALID_COLLECTION_NAME_ERROR = "InvalidCollectionNameError";
    static final String INVALID_PROPERTY_NAME_ERROR = "InvalidPropertyNameError";
    static final String INVALID_PROPERTY_VALUE_ERROR = "InvalidPropertyValueError";
}
