package io.github.dorkitude.incisive.client;

import io.github.dorkitude.incisive.client.exceptions.InvalidEventCollectionException;
import io.github.dorkitude.incisive.client.exceptions.InvalidEventException;
import io.github.dorkitude.incisive.client.exceptions.NoWriteKeyException;
import io.github.dorkitude.incisive.client.exceptions.ServerException;
import io.github.dorkitude.incisive.client.http.HttpHandler;
import io.github.dorkitude.incisive.client.http.HttpMethods;
import io.github.dorkitude.incisive.client.http.OutputSource;
import io.github.dorkitude.incisive.client.http.Request;
import io.github.dorkitude.incisive.client.http.Response;
import io.github.dorkitude.incisive.client.http.UrlConnectionHttpHandler;

import java.io.IOException;
import java.io.OutputStreamWriter;
import java.io.StringReader;
import java.io.StringWriter;
import java.io.Writer;
import java.net.InetSocketAddress;
import java.net.Proxy;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.time.ZonedDateTime;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.Executor;
import java.util.concurrent.Executors;

/**
 * Event tracking client.
 *
 * <p>Single events go to {@code POST {base}/{version}/projects/{id}/events/{collection}}; queued
 * events are uploaded in one {@code POST {base}/{version}/projects/{id}/events} with a body of
 * {@code {"collection": [event, ...]}}. Both use the project's write key in the
 * {@code Authorization} header.
 *
 * <p>Errors are delivered to the optional {@link IncisiveCallback}; in debug mode they are thrown.
 */
public class IncisiveClient {

    private static final DateTimeFormatter ISO_8601_FORMAT =
            DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ss.SSSZ", Locale.US);

    // ----- singleton ------------------------------------------------------------------------

    private static volatile IncisiveClient instance;

    /** Returns the shared client registered with {@link #initialize}. */
    public static IncisiveClient client() {
        IncisiveClient c = instance;
        if (c == null) {
            throw new IllegalStateException("Please call IncisiveClient.initialize() before requesting the client.");
        }
        return c;
    }

    /** Registers the shared client. Subsequent calls are ignored. */
    public static synchronized void initialize(IncisiveClient client) {
        if (client == null) {
            throw new IllegalArgumentException("Client must not be null");
        }
        if (instance == null) {
            instance = client;
        }
    }

    public static boolean isInitialized() {
        return instance != null;
    }

    /** Clears the shared client (mostly for tests). */
    static synchronized void resetForTests() {
        instance = null;
    }

    // ----- state ----------------------------------------------------------------------------

    private final HttpHandler httpHandler;
    private final IncisiveJsonHandler jsonHandler;
    private final IncisiveEventStore eventStore;
    private final Executor publishExecutor;
    private final IncisiveNetworkStatusHandler networkStatusHandler;
    private final Object attemptsLock = new Object();
    private final int connectTimeout;
    private final int readTimeout;

    private volatile boolean isActive = true;
    private boolean isDebugMode;
    private int maxAttempts = IncisiveConstants.DEFAULT_MAX_ATTEMPTS;
    private IncisiveProject defaultProject;
    private String baseUrl;
    private String apiVersion;
    private GlobalPropertiesEvaluator globalPropertiesEvaluator;
    private Map<String, Object> globalProperties;
    private Proxy proxy;

    protected IncisiveClient(Builder builder) {
        Environment env = builder.environment != null ? builder.environment : new Environment();
        this.httpHandler = builder.httpHandler;
        this.jsonHandler = builder.jsonHandler;
        this.eventStore = builder.eventStore;
        this.publishExecutor = builder.publishExecutor;
        this.networkStatusHandler = builder.networkStatusHandler;
        this.connectTimeout = builder.connectTimeout;
        this.readTimeout = builder.readTimeout;
        if (httpHandler == null || jsonHandler == null || eventStore == null || publishExecutor == null
                || networkStatusHandler == null) {
            setActive(false);
        }
        String url = builder.baseUrl != null ? builder.baseUrl : env.getBaseUrl();
        setBaseUrl(url);
        this.apiVersion = builder.apiVersion != null ? builder.apiVersion : IncisiveConstants.DEFAULT_API_VERSION;
        if (builder.defaultProject != null) {
            this.defaultProject = builder.defaultProject;
        } else if (env.getProjectId() != null) {
            try {
                this.defaultProject = new IncisiveProject(env);
            } catch (IllegalArgumentException e) {
                IncisiveLogging.log("Ignoring incomplete project configuration from environment: " + e.getMessage());
            }
        }
    }

    // ----- addEvent -------------------------------------------------------------------------

    /** Synchronously sends a single event to the default project. */
    public void addEvent(String eventCollection, Map<String, Object> event) {
        addEvent(eventCollection, event, null);
    }

    public void addEvent(String eventCollection, Map<String, Object> event, Map<String, Object> reservedProperties) {
        addEvent(null, eventCollection, event, reservedProperties, null);
    }

    /**
     * Synchronously sends a single event.
     *
     * @param project            project to use, or null for the default project
     * @param reservedProperties values merged into the reserved metadata object (e.g. "timestamp")
     * @param callback           optional completion callback
     */
    public void addEvent(IncisiveProject project, String eventCollection, Map<String, Object> event,
                         Map<String, Object> reservedProperties, IncisiveCallback callback) {
        if (!isActive) {
            handleLibraryInactive(callback);
            return;
        }
        IncisiveProject useProject = project == null ? defaultProject : project;
        if (useProject == null) {
            handleFailure(null, project, eventCollection, event, reservedProperties,
                    new IllegalStateException("No project specified, but no default project found"));
            return;
        }
        try {
            Map<String, Object> newEvent = validateAndBuildEvent(useProject, eventCollection, event, reservedProperties);
            publish(useProject, eventCollection, newEvent);
            handleSuccess(callback, project, eventCollection, event, reservedProperties);
        } catch (Exception e) {
            handleFailure(callback, project, eventCollection, event, reservedProperties, e);
        }
    }

    public void addEventAsync(String eventCollection, Map<String, Object> event) {
        addEventAsync(eventCollection, event, null);
    }

    public void addEventAsync(String eventCollection, Map<String, Object> event, Map<String, Object> reservedProperties) {
        addEventAsync(null, eventCollection, event, reservedProperties, null);
    }

    /** Sends a single event on the publish executor. */
    public void addEventAsync(final IncisiveProject project, final String eventCollection,
                              final Map<String, Object> event, final Map<String, Object> reservedProperties,
                              final IncisiveCallback callback) {
        if (!isActive) {
            handleLibraryInactive(callback);
            return;
        }
        final IncisiveProject useProject = project == null ? defaultProject : project;
        if (useProject == null) {
            handleFailure(null, project, eventCollection, event, reservedProperties,
                    new IllegalStateException("No project specified, but no default project found"));
            return;
        }
        try {
            publishExecutor.execute(() -> addEvent(useProject, eventCollection, event, reservedProperties, callback));
        } catch (Exception e) {
            handleFailure(callback, project, eventCollection, event, reservedProperties, e);
        }
    }

    // ----- queue ----------------------------------------------------------------------------

    public void queueEvent(String eventCollection, Map<String, Object> event) {
        queueEvent(eventCollection, event, null);
    }

    public void queueEvent(String eventCollection, Map<String, Object> event, Map<String, Object> reservedProperties) {
        queueEvent(null, eventCollection, event, reservedProperties, null);
    }

    /** Validates and stores an event in the event store for a later {@link #sendQueuedEvents}. */
    public void queueEvent(IncisiveProject project, String eventCollection, Map<String, Object> event,
                           Map<String, Object> reservedProperties, IncisiveCallback callback) {
        if (!isActive) {
            handleLibraryInactive(callback);
            return;
        }
        IncisiveProject useProject = project == null ? defaultProject : project;
        if (useProject == null) {
            handleFailure(null, project, eventCollection, event, reservedProperties,
                    new IllegalStateException("No project specified, but no default project found"));
            return;
        }
        try {
            Map<String, Object> newEvent = validateAndBuildEvent(useProject, eventCollection, event, reservedProperties);
            StringWriter writer = new StringWriter();
            jsonHandler.writeJson(writer, newEvent);
            Object handle = eventStore.store(useProject.getProjectId(), eventCollection, writer.toString());
            if (eventStore instanceof IncisiveAttemptCountingEventStore) {
                try {
                    synchronized (attemptsLock) {
                        Map<String, Integer> attempts = getAttemptsMap(useProject.getProjectId(), eventCollection);
                        attempts.put(String.valueOf(handle.hashCode()), maxAttempts);
                        setAttemptsMap(useProject.getProjectId(), eventCollection, attempts);
                    }
                } catch (IOException ex) {
                    IncisiveLogging.log("Failed to set the event POST attempt count; the event is still queued.");
                }
            }
            handleSuccess(callback, project, eventCollection, event, reservedProperties);
        } catch (Exception e) {
            handleFailure(callback, project, eventCollection, event, reservedProperties, e);
        }
    }

    public void sendQueuedEvents() {
        sendQueuedEvents(null);
    }

    public void sendQueuedEvents(IncisiveProject project) {
        sendQueuedEvents(project, null);
    }

    /** Uploads all queued events of a project in a single batch request. */
    public synchronized void sendQueuedEvents(IncisiveProject project, IncisiveCallback callback) {
        if (!isActive) {
            handleLibraryInactive(callback);
            return;
        }
        IncisiveProject useProject = project == null ? defaultProject : project;
        if (useProject == null) {
            handleFailure(null, new IllegalStateException("No project specified, but no default project found"));
            return;
        }
        if (!networkStatusHandler.isNetworkConnected()) {
            IncisiveLogging.log("Not sending events because there is no network connection.");
            handleFailure(callback, new Exception("Network not connected."));
            return;
        }
        try {
            String projectId = useProject.getProjectId();
            Map<String, List<Object>> handles = eventStore.getHandles(projectId);
            Map<String, List<Object>> sentHandles = new LinkedHashMap<>();
            Map<String, List<Map<String, Object>>> events = buildEventMap(projectId, handles, sentHandles);
            String response = publishAll(useProject, events);
            if (response != null) {
                try {
                    handleAddEventsResponse(sentHandles, response);
                } catch (Exception e) {
                    IncisiveLogging.log("Error handling response to batch publish: " + e.getMessage());
                }
            }
            handleSuccess(callback);
        } catch (Exception e) {
            handleFailure(callback, e);
        }
    }

    public void sendQueuedEventsAsync() {
        sendQueuedEventsAsync(null);
    }

    public void sendQueuedEventsAsync(IncisiveProject project) {
        sendQueuedEventsAsync(project, null);
    }

    public void sendQueuedEventsAsync(final IncisiveProject project, final IncisiveCallback callback) {
        if (!isActive) {
            handleLibraryInactive(callback);
            return;
        }
        final IncisiveProject useProject = project == null ? defaultProject : project;
        if (useProject == null) {
            handleFailure(null, new IllegalStateException("No project specified, but no default project found"));
            return;
        }
        try {
            publishExecutor.execute(() -> sendQueuedEvents(useProject, callback));
        } catch (Exception e) {
            handleFailure(callback, e);
        }
    }

    // ----- accessors ------------------------------------------------------------------------

    public IncisiveJsonHandler getJsonHandler() {
        return jsonHandler;
    }

    public IncisiveEventStore getEventStore() {
        return eventStore;
    }

    public Executor getPublishExecutor() {
        return publishExecutor;
    }

    public HttpHandler getHttpHandler() {
        return httpHandler;
    }

    public IncisiveProject getDefaultProject() {
        return defaultProject;
    }

    public void setDefaultProject(IncisiveProject defaultProject) {
        this.defaultProject = defaultProject;
    }

    public String getBaseUrl() {
        return baseUrl;
    }

    /** Sets the server base URL (scheme, host, port, optional path prefix). Null restores the default. */
    public void setBaseUrl(String baseUrl) {
        String url = baseUrl == null ? IncisiveConstants.DEFAULT_BASE_URL : baseUrl;
        while (url.endsWith("/")) {
            url = url.substring(0, url.length() - 1);
        }
        this.baseUrl = url;
    }

    public String getApiVersion() {
        return apiVersion;
    }

    public void setApiVersion(String apiVersion) {
        this.apiVersion = apiVersion == null ? IncisiveConstants.DEFAULT_API_VERSION : apiVersion;
    }

    public int getMaxAttempts() {
        return maxAttempts;
    }

    public void setMaxAttempts(int maxAttempts) {
        this.maxAttempts = maxAttempts;
    }

    public GlobalPropertiesEvaluator getGlobalPropertiesEvaluator() {
        return globalPropertiesEvaluator;
    }

    public void setGlobalPropertiesEvaluator(GlobalPropertiesEvaluator globalPropertiesEvaluator) {
        this.globalPropertiesEvaluator = globalPropertiesEvaluator;
    }

    public Map<String, Object> getGlobalProperties() {
        return globalProperties;
    }

    public void setGlobalProperties(Map<String, Object> globalProperties) {
        this.globalProperties = globalProperties;
    }

    public boolean isDebugMode() {
        return isDebugMode;
    }

    /** In debug mode failures are thrown instead of delivered to callbacks. */
    public void setDebugMode(boolean isDebugMode) {
        this.isDebugMode = isDebugMode;
    }

    public boolean isActive() {
        return isActive;
    }

    protected void setActive(boolean isActive) {
        this.isActive = isActive;
        IncisiveLogging.log("Incisive client set to " + (isActive ? "active" : "inactive"));
    }

    public void setProxy(String proxyHost, int proxyPort) {
        this.proxy = new Proxy(Proxy.Type.HTTP, new InetSocketAddress(proxyHost, proxyPort));
    }

    public void setProxy(Proxy proxy) {
        this.proxy = proxy;
    }

    public Proxy getProxy() {
        return proxy;
    }

    // ----- builder --------------------------------------------------------------------------

    /**
     * Builder for {@link IncisiveClient}. Unset components fall back to the {@code getDefault*}
     * methods, which subclasses may override (e.g. an Android builder with a file-backed store).
     */
    public static class Builder {
        private HttpHandler httpHandler;
        private IncisiveJsonHandler jsonHandler;
        private IncisiveEventStore eventStore;
        private Executor publishExecutor;
        private IncisiveNetworkStatusHandler networkStatusHandler;
        private int connectTimeout = IncisiveConstants.DEFAULT_CONNECT_TIMEOUT;
        private int readTimeout = IncisiveConstants.DEFAULT_READ_TIMEOUT;
        private String baseUrl;
        private String apiVersion;
        private IncisiveProject defaultProject;
        private Environment environment;

        protected HttpHandler getDefaultHttpHandler() throws Exception {
            return new UrlConnectionHttpHandler();
        }

        protected IncisiveJsonHandler getDefaultJsonHandler() throws Exception {
            return new JacksonJsonHandler();
        }

        protected IncisiveEventStore getDefaultEventStore() throws Exception {
            return new RamEventStore();
        }

        protected Executor getDefaultPublishExecutor() throws Exception {
            return Executors.newFixedThreadPool(Runtime.getRuntime().availableProcessors(), r -> {
                Thread t = new Thread(r, "incisive-publish");
                t.setDaemon(true);
                return t;
            });
        }

        protected IncisiveNetworkStatusHandler getDefaultNetworkStatusHandler() {
            return new AlwaysConnectedNetworkStatusHandler();
        }

        public HttpHandler getHttpHandler() {
            return httpHandler;
        }

        public void setHttpHandler(HttpHandler httpHandler) {
            this.httpHandler = httpHandler;
        }

        public Builder withHttpHandler(HttpHandler httpHandler) {
            setHttpHandler(httpHandler);
            return this;
        }

        public IncisiveJsonHandler getJsonHandler() {
            return jsonHandler;
        }

        public void setJsonHandler(IncisiveJsonHandler jsonHandler) {
            this.jsonHandler = jsonHandler;
        }

        public Builder withJsonHandler(IncisiveJsonHandler jsonHandler) {
            setJsonHandler(jsonHandler);
            return this;
        }

        public IncisiveEventStore getEventStore() {
            return eventStore;
        }

        public void setEventStore(IncisiveEventStore eventStore) {
            this.eventStore = eventStore;
        }

        public Builder withEventStore(IncisiveEventStore eventStore) {
            setEventStore(eventStore);
            return this;
        }

        public Executor getPublishExecutor() {
            return publishExecutor;
        }

        public void setPublishExecutor(Executor publishExecutor) {
            this.publishExecutor = publishExecutor;
        }

        public Builder withPublishExecutor(Executor publishExecutor) {
            setPublishExecutor(publishExecutor);
            return this;
        }

        public IncisiveNetworkStatusHandler getNetworkStatusHandler() {
            return networkStatusHandler;
        }

        public void setNetworkStatusHandler(IncisiveNetworkStatusHandler networkStatusHandler) {
            this.networkStatusHandler = networkStatusHandler;
        }

        public Builder withNetworkStatusHandler(IncisiveNetworkStatusHandler networkStatusHandler) {
            setNetworkStatusHandler(networkStatusHandler);
            return this;
        }

        public Builder withConnectTimeout(int connectTimeout) {
            this.connectTimeout = connectTimeout;
            return this;
        }

        public Builder withReadTimeout(int readTimeout) {
            this.readTimeout = readTimeout;
            return this;
        }

        /** Server base URL; defaults to INCISIVE_BASE_URL or http://localhost:8080. */
        public Builder withBaseUrl(String baseUrl) {
            this.baseUrl = baseUrl;
            return this;
        }

        /** API version path segment; defaults to "3.0". */
        public Builder withApiVersion(String apiVersion) {
            this.apiVersion = apiVersion;
            return this;
        }

        /** Default project; defaults to one built from INCISIVE_* environment variables if set. */
        public Builder withDefaultProject(IncisiveProject project) {
            this.defaultProject = project;
            return this;
        }

        /** Overrides where environment configuration is read from. */
        public Builder withEnvironment(Environment environment) {
            this.environment = environment;
            return this;
        }

        public IncisiveClient build() {
            try {
                if (httpHandler == null) {
                    httpHandler = getDefaultHttpHandler();
                }
            } catch (Exception e) {
                IncisiveLogging.log("Exception building HTTP handler: " + e.getMessage());
            }
            try {
                if (jsonHandler == null) {
                    jsonHandler = getDefaultJsonHandler();
                }
            } catch (Exception e) {
                IncisiveLogging.log("Exception building JSON handler: " + e.getMessage());
            }
            try {
                if (eventStore == null) {
                    eventStore = getDefaultEventStore();
                }
            } catch (Exception e) {
                IncisiveLogging.log("Exception building event store: " + e.getMessage());
            }
            try {
                if (publishExecutor == null) {
                    publishExecutor = getDefaultPublishExecutor();
                }
            } catch (Exception e) {
                IncisiveLogging.log("Exception building publish executor: " + e.getMessage());
            }
            if (networkStatusHandler == null) {
                networkStatusHandler = getDefaultNetworkStatusHandler();
            }
            return buildInstance();
        }

        protected IncisiveClient buildInstance() {
            return new IncisiveClient(this);
        }
    }

    // ----- event building -------------------------------------------------------------------

    protected Map<String, Object> validateAndBuildEvent(IncisiveProject project, String eventCollection,
                                                        Map<String, Object> event,
                                                        Map<String, Object> reservedProperties) {
        if (project.getWriteKey() == null) {
            throw new NoWriteKeyException("You can't send events if you haven't set a write key.");
        }
        validateEventCollection(eventCollection);
        validateEvent(event, 0);

        Map<String, Object> newEvent = new LinkedHashMap<>();
        Map<String, Object> mergedReserved = new LinkedHashMap<>();
        if (globalProperties != null) {
            mergeGlobalProperties(globalProperties, mergedReserved, newEvent);
        }
        GlobalPropertiesEvaluator evaluator = globalPropertiesEvaluator;
        if (evaluator != null) {
            mergeGlobalProperties(evaluator.getGlobalProperties(eventCollection), mergedReserved, newEvent);
        }
        if (reservedProperties != null) {
            mergedReserved.putAll(reservedProperties);
        }
        if (!mergedReserved.containsKey("timestamp")) {
            mergedReserved.put("timestamp", ISO_8601_FORMAT.format(ZonedDateTime.now()));
        }
        newEvent.put(IncisiveConstants.RESERVED_PROPERTY, mergedReserved);
        newEvent.putAll(event);
        return newEvent;
    }

    @SuppressWarnings("unchecked")
    private static void mergeGlobalProperties(Map<String, Object> globals, Map<String, Object> reserved,
                                              Map<String, Object> newEvent) {
        if (globals == null) {
            return;
        }
        Map<String, Object> copy = new LinkedHashMap<>(globals);
        Object reservedGlobals = copy.remove(IncisiveConstants.RESERVED_PROPERTY);
        if (reservedGlobals instanceof Map) {
            reserved.putAll((Map<String, Object>) reservedGlobals);
        }
        newEvent.putAll(copy);
    }

    private static void validateEventCollection(String eventCollection) {
        if (eventCollection == null || eventCollection.isEmpty()) {
            throw new InvalidEventCollectionException(
                    "You must specify a non-null, non-empty event collection: " + eventCollection);
        }
        if (eventCollection.length() > 256) {
            throw new InvalidEventCollectionException("An event collection name cannot be longer than 256 characters.");
        }
    }

    @SuppressWarnings("unchecked")
    private static void validateEvent(Map<String, Object> event, int depth) {
        if (depth == 0) {
            if (event == null || event.isEmpty()) {
                throw new InvalidEventException("You must specify a non-null, non-empty event.");
            }
            if (event.containsKey(IncisiveConstants.RESERVED_PROPERTY)) {
                throw new InvalidEventException("An event cannot contain a root-level property named '"
                        + IncisiveConstants.RESERVED_PROPERTY + "'.");
            }
        } else if (depth > IncisiveConstants.MAX_EVENT_DEPTH) {
            throw new InvalidEventException("An event's depth (i.e. layers of nesting) cannot exceed "
                    + IncisiveConstants.MAX_EVENT_DEPTH);
        }
        for (Map.Entry<String, Object> entry : event.entrySet()) {
            String key = entry.getKey();
            if (key.contains(".")) {
                throw new InvalidEventException("An event cannot contain a property with the period (.) character in it.");
            }
            if (key.length() > 256) {
                throw new InvalidEventException("An event cannot contain a property name longer than 256 characters.");
            }
            validateEventValue(entry.getValue(), depth);
        }
    }

    @SuppressWarnings("unchecked")
    private static void validateEventValue(Object value, int depth) {
        if (value instanceof String) {
            if (((String) value).length() >= 10000) {
                throw new InvalidEventException("An event cannot contain a string property value longer than 10,000 characters.");
            }
        } else if (value instanceof Map) {
            validateEvent((Map<String, Object>) value, depth + 1);
        } else if (value instanceof Iterable) {
            for (Object element : (Iterable<Object>) value) {
                validateEventValue(element, depth);
            }
        }
    }

    // ----- HTTP -----------------------------------------------------------------------------

    /**
     * Percent-encodes a name for use as one URL path segment (RFC 3986 pchar). Unreserved
     * characters and {@code !$&'()*+,;=:@} stay literal; everything else, including "/", "%",
     * space and non-ASCII (as UTF-8 bytes), becomes {@code %XX} with uppercase hex.
     */
    public static String encodePathSegment(String segment) {
        StringBuilder sb = new StringBuilder();
        for (byte b : segment.getBytes(StandardCharsets.UTF_8)) {
            int c = b & 0xFF;
            boolean literal = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9')
                    || "-._~!$&'()*+,;=:@".indexOf(c) >= 0;
            if (literal) {
                sb.append((char) c);
            } else {
                sb.append('%').append(Character.toUpperCase(Character.forDigit(c >> 4, 16)))
                        .append(Character.toUpperCase(Character.forDigit(c & 0xF, 16)));
            }
        }
        return sb.toString();
    }

    static String encodeCollectionName(String eventCollection) {
        return encodePathSegment(eventCollection);
    }

    private String projectUrl(IncisiveProject project) {
        return String.format(Locale.US, "%s/%s/projects/%s", baseUrl, apiVersion, project.getProjectId());
    }

    private void publish(IncisiveProject project, String eventCollection, Map<String, Object> event)
            throws IOException {
        URL url = new URL(projectUrl(project) + "/events/" + encodeCollectionName(eventCollection));
        publishObject(project, url, event);
    }

    private String publishAll(IncisiveProject project, Map<String, List<Map<String, Object>>> events)
            throws IOException {
        URL url = new URL(projectUrl(project) + "/events");
        return publishObject(project, url, events);
    }

    private String publishObject(IncisiveProject project, URL url, final Map<String, ?> requestData)
            throws IOException {
        if (requestData == null || requestData.isEmpty()) {
            IncisiveLogging.log("No API calls were made because there were no events to upload");
            return null;
        }
        OutputSource source = out -> {
            Writer writer = new OutputStreamWriter(out, StandardCharsets.UTF_8);
            jsonHandler.writeJson(writer, requestData);
            writer.flush();
        };
        Request request = new Request(url, HttpMethods.POST, project.getWriteKey(), source, proxy,
                connectTimeout, readTimeout);
        Response response = httpHandler.execute(request);
        IncisiveLogging.log(String.format(Locale.US, "Received response: '%s' (%d)", response.body, response.statusCode));
        if (response.isSuccess()) {
            return response.body;
        }
        throw new ServerException(response.statusCode, response.body);
    }

    private Map<String, List<Map<String, Object>>> buildEventMap(String projectId,
                                                                 Map<String, List<Object>> eventHandles,
                                                                 Map<String, List<Object>> sentHandles)
            throws IOException {
        Map<String, List<Map<String, Object>>> result = new LinkedHashMap<>();
        for (Map.Entry<String, List<Object>> entry : eventHandles.entrySet()) {
            String collection = entry.getKey();
            List<Object> handles = entry.getValue();
            if (handles == null || handles.isEmpty()) {
                continue;
            }
            List<Map<String, Object>> events = new ArrayList<>(handles.size());
            List<Object> sent = new ArrayList<>(handles.size());
            if (eventStore instanceof IncisiveAttemptCountingEventStore) {
                synchronized (attemptsLock) {
                    Map<String, Integer> attempts;
                    try {
                        attempts = getAttemptsMap(projectId, collection);
                    } catch (IOException ex) {
                        attempts = new HashMap<>();
                    }
                    for (Object handle : handles) {
                        String key = String.valueOf(handle.hashCode());
                        Integer remaining = attempts.get(key);
                        int left = (remaining == null ? 1 : remaining) - 1;
                        if (left >= 0) {
                            attempts.put(key, left);
                            events.add(getEvent(handle));
                            sent.add(handle);
                        } else {
                            eventStore.remove(handle);
                            attempts.remove(key);
                        }
                    }
                    try {
                        setAttemptsMap(projectId, collection, attempts);
                    } catch (IOException ex) {
                        IncisiveLogging.log("Failed to update event POST attempt counts: " + ex);
                    }
                }
            } else {
                for (Object handle : handles) {
                    events.add(getEvent(handle));
                    sent.add(handle);
                }
            }
            if (!events.isEmpty()) {
                result.put(collection, events);
                sentHandles.put(collection, sent);
            }
        }
        return result;
    }

    @SuppressWarnings("unchecked")
    private void handleAddEventsResponse(Map<String, List<Object>> handles, String response) throws IOException {
        Map<String, Object> responseMap = jsonHandler.readJson(new StringReader(response));
        for (Map.Entry<String, Object> entry : responseMap.entrySet()) {
            List<Object> collectionHandles = handles.get(entry.getKey());
            if (collectionHandles == null || !(entry.getValue() instanceof List)) {
                continue;
            }
            List<Object> results = (List<Object>) entry.getValue();
            for (int i = 0; i < results.size() && i < collectionHandles.size(); i++) {
                if (!(results.get(i) instanceof Map)) {
                    continue;
                }
                Map<String, Object> eventResult = (Map<String, Object>) results.get(i);
                boolean remove = true;
                if (!Boolean.TRUE.equals(eventResult.get(IncisiveConstants.SUCCESS_PARAM))) {
                    Object err = eventResult.get(IncisiveConstants.ERROR_PARAM);
                    String name = err instanceof Map ? String.valueOf(((Map<String, Object>) err).get(IncisiveConstants.NAME_PARAM)) : null;
                    remove = IncisiveConstants.INVALID_COLLECTION_NAME_ERROR.equals(name)
                            || IncisiveConstants.INVALID_PROPERTY_NAME_ERROR.equals(name)
                            || IncisiveConstants.INVALID_PROPERTY_VALUE_ERROR.equals(name);
                    IncisiveLogging.log("Event failed in batch: " + err + (remove ? " (dropping)" : " (will retry)"));
                }
                if (remove) {
                    try {
                        eventStore.remove(collectionHandles.get(i));
                    } catch (IOException e) {
                        IncisiveLogging.log("Failed to remove object '" + collectionHandles.get(i) + "' from cache");
                    }
                }
            }
        }
    }

    private Map<String, Object> getEvent(Object handle) throws IOException {
        return jsonHandler.readJson(new StringReader(eventStore.get(handle)));
    }

    private Map<String, Integer> getAttemptsMap(String projectId, String eventCollection) throws IOException {
        Map<String, Integer> attempts = new HashMap<>();
        if (eventStore instanceof IncisiveAttemptCountingEventStore) {
            String json = ((IncisiveAttemptCountingEventStore) eventStore).getAttempts(projectId, eventCollection);
            if (json != null) {
                for (Map.Entry<String, Object> e : jsonHandler.readJson(new StringReader(json)).entrySet()) {
                    if (e.getValue() instanceof Number) {
                        attempts.put(e.getKey(), ((Number) e.getValue()).intValue());
                    }
                }
            }
        }
        return attempts;
    }

    private void setAttemptsMap(String projectId, String eventCollection, Map<String, Integer> attempts)
            throws IOException {
        if (eventStore instanceof IncisiveAttemptCountingEventStore) {
            StringWriter writer = new StringWriter();
            jsonHandler.writeJson(writer, attempts);
            ((IncisiveAttemptCountingEventStore) eventStore).setAttempts(projectId, eventCollection, writer.toString());
        }
    }

    // ----- callbacks ------------------------------------------------------------------------

    private void handleSuccess(IncisiveCallback callback) {
        if (callback != null) {
            try {
                callback.onSuccess();
            } catch (Exception ignored) {
                // user callback errors are swallowed
            }
        }
    }

    private void handleSuccess(IncisiveCallback callback, IncisiveProject project, String eventCollection,
                               Map<String, Object> event, Map<String, Object> reservedProperties) {
        handleSuccess(callback);
        if (callback instanceof IncisiveDetailedCallback) {
            try {
                ((IncisiveDetailedCallback) callback).onSuccess(project, eventCollection, event, reservedProperties);
            } catch (Exception ignored) {
                // user callback errors are swallowed
            }
        }
    }

    private void handleFailure(IncisiveCallback callback, Exception e) {
        if (isDebugMode) {
            throw e instanceof RuntimeException ? (RuntimeException) e : new RuntimeException(e);
        }
        IncisiveLogging.log("Encountered error: " + e.getMessage());
        if (callback != null) {
            try {
                callback.onFailure(e);
            } catch (Exception ignored) {
                // user callback errors are swallowed
            }
        }
    }

    private void handleFailure(IncisiveCallback callback, IncisiveProject project, String eventCollection,
                               Map<String, Object> event, Map<String, Object> reservedProperties, Exception e) {
        handleFailure(callback, e);
        if (callback instanceof IncisiveDetailedCallback) {
            try {
                ((IncisiveDetailedCallback) callback).onFailure(project, eventCollection, event, reservedProperties, e);
            } catch (Exception ignored) {
                // user callback errors are swallowed
            }
        }
    }

    private void handleLibraryInactive(IncisiveCallback callback) {
        handleFailure(callback, new IllegalStateException("The Incisive library failed to initialize properly and is inactive"));
    }
}
