package io.github.dorkitude.incisive.client;

import com.fasterxml.jackson.databind.JsonNode;
import io.github.dorkitude.incisive.client.exceptions.InvalidEventException;
import io.github.dorkitude.incisive.client.exceptions.NoWriteKeyException;
import io.github.dorkitude.incisive.client.exceptions.ServerException;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicReference;

import static org.junit.jupiter.api.Assertions.*;

class IncisiveClientTest {
    private MockServer server;
    private IncisiveClient client;
    private IncisiveProject project;

    @BeforeEach
    void setUp() throws Exception {
        server = new MockServer();
        project = new IncisiveProject("proj1", "WRITE", "READ", "MASTER");
        client = new IncisiveClient.Builder()
                .withEnvironment(new Environment(Collections.emptyMap()))
                .withBaseUrl(server.baseUrl())
                .withDefaultProject(project)
                .withPublishExecutor(Runnable::run)
                .build();
    }

    @AfterEach
    void tearDown() {
        server.close();
    }

    private static Map<String, Object> event(Object... kv) {
        Map<String, Object> m = new LinkedHashMap<>();
        for (int i = 0; i < kv.length; i += 2) {
            m.put((String) kv[i], kv[i + 1]);
        }
        return m;
    }

    static final class RecordingCallback implements IncisiveDetailedCallback {
        int successes;
        int detailedSuccesses;
        final List<Exception> failures = new ArrayList<>();
        final List<Exception> detailedFailures = new ArrayList<>();

        @Override public void onSuccess() { successes++; }
        @Override public void onFailure(Exception e) { failures.add(e); }
        @Override public void onSuccess(IncisiveProject p, String c, Map<String, Object> e, Map<String, Object> k) { detailedSuccesses++; }
        @Override public void onFailure(IncisiveProject p, String c, Map<String, Object> e, Map<String, Object> k, Exception ex) { detailedFailures.add(ex); }
    }

    @Test
    void addEventWireFormat() throws Exception {
        server.respond(201, "{\"created\": true}");
        RecordingCallback cb = new RecordingCallback();
        client.addEvent(null, "purchases", event("item", "golden widget", "price", 25), null, cb);

        assertEquals(1, cb.successes);
        assertEquals(1, cb.detailedSuccesses);
        assertTrue(cb.failures.isEmpty());

        MockServer.Recorded r = server.last();
        assertEquals("POST", r.method);
        assertEquals("/1.0/projects/proj1/events/purchases", r.rawPath);
        assertNull(r.rawQuery);
        assertEquals("WRITE", r.header("Authorization"));
        assertEquals("application/json", r.header("Content-Type"));
        assertEquals("application/json", r.header("Accept"));
        assertEquals("incisive-java/" + IncisiveVersion.getSdkVersion(), r.header("User-Agent"));
        assertTrue(r.headers.keySet().stream().noneMatch(h -> h.toLowerCase().endsWith("-sdk")), "no SDK header");

        JsonNode body = r.json();
        assertEquals("golden widget", body.get("item").asText());
        assertEquals(25, body.get("price").asInt());
        String ts = body.get("keen").get("timestamp").asText();
        assertTrue(ts.matches("\\d{4}-\\d{2}-\\d{2}T\\d{2}:\\d{2}:\\d{2}\\.\\d{3}[+-]\\d{4}"), ts);
        assertEquals(3, body.size());
    }

    @Test
    void reservedPropertiesAndGlobalsMerge() throws Exception {
        Map<String, Object> globals = new HashMap<>();
        globals.put("app", "demo");
        globals.put("keen", event("addons", List.of()));
        client.setGlobalProperties(globals);
        client.setGlobalPropertiesEvaluator(collection -> event("collection_seen", collection, "app", "evaluated"));

        client.addEvent("signups", event("user", "u1"), event("timestamp", "2026-01-02T03:04:05.000Z"));

        JsonNode body = server.last().json();
        assertEquals("u1", body.get("user").asText());
        assertEquals("evaluated", body.get("app").asText(), "evaluator overrides static globals");
        assertEquals("signups", body.get("collection_seen").asText());
        assertEquals("2026-01-02T03:04:05.000Z", body.get("keen").get("timestamp").asText());
        assertTrue(body.get("keen").get("addons").isArray());
    }

    @Test
    void collectionNameIsEscapedAsOnePathSegment() {
        client.addEvent("my events/ünï?#%", event("a", 1));
        assertEquals("/1.0/projects/proj1/events/my%20events%2F%C3%BCn%C3%AF%3F%23%25", server.last().rawPath);
        assertNull(server.last().rawQuery);
    }

    @Test
    void customApiVersionAndBaseUrlPrefix() {
        client.setBaseUrl(server.baseUrl() + "/prefix/");
        client.setApiVersion("9.9");
        client.addEvent("c", event("a", 1));
        assertEquals("/prefix/9.9/projects/proj1/events/c", server.last().rawPath);
    }

    @Test
    void serverErrorGoesToCallbackWithParsedFields() {
        server.respond(401, "{\"message\": \"bad key\", \"error_code\": \"InvalidApiKeyError\"}");
        RecordingCallback cb = new RecordingCallback();
        client.addEvent(null, "c", event("a", 1), null, cb);

        assertEquals(0, cb.successes);
        assertEquals(1, cb.failures.size());
        assertEquals(1, cb.detailedFailures.size());
        ServerException e = assertInstanceOf(ServerException.class, cb.failures.get(0));
        assertEquals(401, e.getStatusCode());
        assertEquals("InvalidApiKeyError", e.getErrorCode());
        assertEquals("bad key", e.getServerMessage());
        assertEquals("{\"message\": \"bad key\", \"error_code\": \"InvalidApiKeyError\"}", e.getMessage());
    }

    @Test
    void debugModeThrows() {
        server.respond(500, "oops");
        client.setDebugMode(true);
        ServerException e = assertThrows(ServerException.class, () -> client.addEvent("c", event("a", 1)));
        assertEquals(500, e.getStatusCode());
        assertNull(e.getErrorCode());
        assertEquals("oops", e.getMessage());
    }

    @Test
    void validationErrors() {
        client.setDebugMode(true);
        assertThrows(InvalidEventException.class, () -> client.addEvent("c", event("keen", event("x", 1))));
        assertThrows(InvalidEventException.class, () -> client.addEvent("c", event("a.b", 1)));
        assertThrows(InvalidEventException.class, () -> client.addEvent("c", new HashMap<>()));
        assertThrows(RuntimeException.class, () -> client.addEvent("", event("a", 1)));
        IncisiveProject readOnly = new IncisiveProject("p", null, "READ");
        assertThrows(NoWriteKeyException.class, () -> client.addEvent(readOnly, "c", event("a", 1), null, null));
        assertTrue(server.requests().isEmpty());
    }

    @Test
    void addEventAsyncUsesExecutor() {
        RecordingCallback cb = new RecordingCallback();
        client.addEventAsync(null, "async", event("a", 1), null, cb);
        assertEquals(1, cb.successes);
        assertEquals("/1.0/projects/proj1/events/async", server.last().rawPath);
    }

    @Test
    void queueAndSendBatch() throws Exception {
        client.queueEvent("clicks", event("n", 1));
        client.queueEvent("clicks", event("n", 2));
        client.queueEvent("views", event("n", 3));
        assertTrue(server.requests().isEmpty());

        server.respond(200, "{\"clicks\": [{\"success\": true}, {\"success\": false, \"error\": {\"name\": \"SomeTransientError\", \"description\": \"try again\"}}],"
                + " \"views\": [{\"success\": false, \"error\": {\"name\": \"InvalidPropertyNameError\", \"description\": \"bad\"}}]}");
        RecordingCallback cb = new RecordingCallback();
        client.sendQueuedEvents(null, cb);
        assertEquals(1, cb.successes);

        MockServer.Recorded r = server.last();
        assertEquals("POST", r.method);
        assertEquals("/1.0/projects/proj1/events", r.rawPath);
        assertEquals("WRITE", r.header("Authorization"));
        JsonNode body = r.json();
        assertEquals(2, body.get("clicks").size());
        assertEquals(1, body.get("clicks").get(0).get("n").asInt());
        assertEquals(2, body.get("clicks").get(1).get("n").asInt());
        assertTrue(body.get("clicks").get(0).get("keen").has("timestamp"));
        assertEquals(1, body.get("views").size());

        // Only the transient failure remains queued.
        Map<String, List<Object>> remaining = client.getEventStore().getHandles("proj1");
        assertEquals(1, remaining.size());
        assertEquals(1, remaining.get("clicks").size());

        server.respond(200, "{\"clicks\": [{\"success\": true}]}");
        client.sendQueuedEvents();
        JsonNode second = server.last().json();
        assertEquals(1, second.get("clicks").size());
        assertEquals(2, second.get("clicks").get(0).get("n").asInt());
        assertTrue(client.getEventStore().getHandles("proj1").isEmpty());

        int before = server.requests().size();
        client.sendQueuedEvents();
        assertEquals(before, server.requests().size(), "no request when queue is empty");
    }

    @Test
    void eventsDroppedAfterMaxAttempts() throws Exception {
        client.setMaxAttempts(2);
        client.queueEvent("c", event("n", 1));
        for (int i = 0; i < 2; i++) {
            server.respond(500, "{\"message\":\"down\",\"error_code\":\"ServiceUnavailable\"}");
            RecordingCallback cb = new RecordingCallback();
            client.sendQueuedEvents(null, cb);
            ServerException e = assertInstanceOf(ServerException.class, cb.failures.get(0));
            assertEquals(500, e.getStatusCode());
        }
        int before = server.requests().size();
        client.sendQueuedEvents();
        assertEquals(before, server.requests().size());
        assertTrue(client.getEventStore().getHandles("proj1").isEmpty());
    }

    @Test
    void offlineDoesNotSend() {
        IncisiveClient offline = new IncisiveClient.Builder()
                .withEnvironment(new Environment(Collections.emptyMap()))
                .withBaseUrl(server.baseUrl()).withDefaultProject(project)
                .withNetworkStatusHandler(() -> false).build();
        offline.queueEvent("c", event("a", 1));
        RecordingCallback cb = new RecordingCallback();
        offline.sendQueuedEvents(null, cb);
        assertEquals(1, cb.failures.size());
        assertTrue(server.requests().isEmpty());
    }

    @Test
    void environmentConfiguration() {
        Map<String, String> env = new HashMap<>();
        env.put("INCISIVE_PROJECT_ID", "envproj");
        env.put("INCISIVE_WRITE_KEY", "ENVWRITE");
        env.put("INCISIVE_BASE_URL", server.baseUrl());
        IncisiveClient c = new IncisiveClient.Builder().withEnvironment(new Environment(env)).build();
        assertEquals("envproj", c.getDefaultProject().getProjectId());
        assertEquals(server.baseUrl(), c.getBaseUrl());
        c.addEvent("c", event("a", 1));
        assertEquals("/1.0/projects/envproj/events/c", server.last().rawPath);
        assertEquals("ENVWRITE", server.last().header("Authorization"));
    }

    @Test
    void defaults() {
        IncisiveClient c = new JavaIncisiveClientBuilder().withEnvironment(new Environment(Collections.emptyMap())).build();
        assertEquals("http://localhost:8080", c.getBaseUrl());
        assertEquals("1.0", c.getApiVersion());
        assertNull(c.getDefaultProject());
        assertTrue(c.isActive());
        assertInstanceOf(RamEventStore.class, c.getEventStore());
    }

    @Test
    void noProjectFailsViaCallback() {
        IncisiveClient c = new IncisiveClient.Builder().withEnvironment(new Environment(Collections.emptyMap())).build();
        c.setDebugMode(true);
        assertThrows(IllegalStateException.class, () -> c.addEvent("c", event("a", 1)));
    }

    @Test
    void projectValidation() {
        assertThrows(IllegalArgumentException.class, () -> new IncisiveProject(null, "w", "r"));
        assertThrows(IllegalArgumentException.class, () -> new IncisiveProject("p", null, " ", null));
        assertEquals("M", new IncisiveProject("p", null, null, "M").getMasterKey());
    }

    @Test
    void singleton() {
        IncisiveClient.resetForTests();
        assertFalse(IncisiveClient.isInitialized());
        assertThrows(IllegalStateException.class, IncisiveClient::client);
        IncisiveClient.initialize(client);
        assertSame(client, IncisiveClient.client());
        IncisiveClient.resetForTests();
    }

    @Test
    void ramEventStoreEvictsOldest() throws Exception {
        RamEventStore store = new RamEventStore();
        store.setMaxEventsPerCollection(2);
        Object h1 = store.store("p", "c", "{\"n\":1}");
        store.store("p", "c", "{\"n\":2}");
        store.store("p", "c", "{\"n\":3}");
        assertNull(store.get(h1));
        assertEquals(2, store.getHandles("p").get("c").size());
        assertTrue(store.getHandles("other").isEmpty());
    }

    @Test
    void callbackExceptionsAreSwallowed() {
        AtomicReference<Boolean> called = new AtomicReference<>(false);
        client.addEvent(null, "c", event("a", 1), null, new IncisiveCallback() {
            @Override public void onSuccess() { called.set(true); throw new RuntimeException("user bug"); }
            @Override public void onFailure(Exception e) { fail("should not fail"); }
        });
        assertTrue(called.get());
    }
}
