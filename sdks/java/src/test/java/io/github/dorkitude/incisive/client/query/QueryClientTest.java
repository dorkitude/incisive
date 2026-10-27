package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.Environment;
import io.github.dorkitude.incisive.client.IncisiveProject;
import io.github.dorkitude.incisive.client.MockServer;
import io.github.dorkitude.incisive.client.exceptions.IncisiveQueryClientException;
import io.github.dorkitude.incisive.client.exceptions.ServerException;
import io.github.dorkitude.incisive.client.query.result.FunnelResult;
import io.github.dorkitude.incisive.client.query.result.Group;
import io.github.dorkitude.incisive.client.query.result.IntervalResultValue;
import io.github.dorkitude.incisive.client.query.result.MultiAnalysisResult;
import io.github.dorkitude.incisive.client.query.result.QueryResult;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.util.Collections;
import java.util.List;
import java.util.Map;

import static io.github.dorkitude.incisive.client.MockServer.json;
import static org.junit.jupiter.api.Assertions.*;

class QueryClientTest {
    private MockServer server;
    private IncisiveQueryClient client;

    @BeforeEach
    void setUp() throws Exception {
        server = new MockServer();
        client = new IncisiveQueryClient.Builder(new IncisiveProject("proj1", "WRITE", "READ", "MASTER"))
                .withEnvironment(new Environment(Collections.emptyMap()))
                .withBaseUrl(server.baseUrl())
                .build();
    }

    @AfterEach
    void tearDown() {
        server.close();
    }

    private void assertQueryRequest(String analysis, String expectedBody) throws Exception {
        MockServer.Recorded r = server.last();
        assertEquals("POST", r.method);
        assertEquals("/1.0/projects/proj1/queries/" + analysis, r.rawPath);
        assertNull(r.rawQuery);
        assertEquals("READ", r.header("Authorization"));
        assertEquals("application/json", r.header("Content-Type"));
        assertEquals(json(expectedBody), r.json());
    }

    @Test
    void count() throws Exception {
        server.respond(200, "{\"result\": 42}");
        assertEquals(42L, client.count("purchases", new RelativeTimeframe("this_7_days")));
        assertQueryRequest("count", "{\"event_collection\":\"purchases\",\"timeframe\":\"this_7_days\"}");
    }

    @Test
    void simpleAnalysesHitTheirRoutes() throws Exception {
        RelativeTimeframe tf = new RelativeTimeframe("this_1_days", "UTC");
        String body = "{\"event_collection\":\"c\",\"target_property\":\"p\",\"timeframe\":\"this_1_days\",\"timezone\":\"UTC\"}";

        server.respond(200, "{\"result\": 7}");
        assertEquals(7L, client.countUnique("c", "p", tf));
        assertQueryRequest("count_unique", body);

        server.respond(200, "{\"result\": 1.5}");
        assertEquals(1.5, client.minimum("c", "p", tf));
        assertQueryRequest("minimum", body);

        server.respond(200, "{\"result\": 9}");
        assertEquals(9.0, client.maximum("c", "p", tf));
        assertQueryRequest("maximum", body);

        server.respond(200, "{\"result\": 2.25}");
        assertEquals(2.25, client.average("c", "p", tf));
        assertQueryRequest("average", body);

        server.respond(200, "{\"result\": 3}");
        assertEquals(3.0, client.median("c", "p", tf));
        assertQueryRequest("median", body);

        server.respond(200, "{\"result\": 100.5}");
        assertEquals(100.5, client.sum("c", "p", tf));
        assertQueryRequest("sum", body);

        server.respond(200, "{\"result\": 0.5}");
        assertEquals(0.5, client.standardDeviation("c", "p", tf));
        assertQueryRequest("standard_deviation", body);

        server.respond(200, "{\"result\": 95}");
        assertEquals(95.0, client.percentile("c", "p", 99.0, tf));
        assertQueryRequest("percentile",
                "{\"event_collection\":\"c\",\"target_property\":\"p\",\"percentile\":99.0,\"timeframe\":\"this_1_days\",\"timezone\":\"UTC\"}");

        server.respond(200, "{\"result\": [\"a\", \"b\", 3]}");
        QueryResult unique = client.selectUnique("c", "p", tf);
        assertTrue(unique.isListResult());
        assertEquals("a", unique.getListResults().get(0).stringValue());
        assertEquals(3L, unique.getListResults().get(2).longValue());
        assertQueryRequest("select_unique", body);
    }

    @Test
    void fullQueryBuilderBody() throws Exception {
        server.respond(200, "{\"result\": 1}");
        Query q = new Query.Builder(QueryType.SUM)
                .withEventCollection("purchases")
                .withTargetProperty("price")
                .withFilter("price", FilterOperator.GREATER_THAN, 10)
                .withFilter("tags", FilterOperator.IN, List.of("a", "b"))
                .withTimeframe(new AbsoluteTimeframe("2026-01-01T00:00:00.000Z", "2026-02-01T00:00:00.000Z"))
                .withTimezone("US/Pacific")
                .withMaxAge(300)
                .build();
        client.execute(q);
        assertQueryRequest("sum", "{\"event_collection\":\"purchases\",\"target_property\":\"price\","
                + "\"filters\":[{\"property_name\":\"price\",\"operator\":\"gt\",\"property_value\":10},"
                + "{\"property_name\":\"tags\",\"operator\":\"in\",\"property_value\":[\"a\",\"b\"]}],"
                + "\"timezone\":\"US/Pacific\",\"max_age\":300,"
                + "\"timeframe\":{\"start\":\"2026-01-01T00:00:00.000Z\",\"end\":\"2026-02-01T00:00:00.000Z\"}}");
    }

    @Test
    void groupByResult() throws Exception {
        server.respond(200, "{\"result\": [{\"country\": \"US\", \"result\": 3}, {\"country\": \"FR\", \"result\": 1}]}");
        QueryResult r = client.execute(new Query.Builder(QueryType.COUNT).withEventCollection("c")
                .withGroupBy("country").withTimeframe(new RelativeTimeframe("this_1_days")).build());
        assertQueryRequest("count", "{\"event_collection\":\"c\",\"group_by\":[\"country\"],\"timeframe\":\"this_1_days\"}");
        assertTrue(r.isGroupResult());
        Map<Group, QueryResult> groups = r.getGroupResults();
        assertEquals(3L, groups.get(new Group(Map.of("country", "US"))).longValue());
        assertEquals(1L, groups.get(new Group(Map.of("country", "FR"))).longValue());
    }

    @Test
    void intervalWithGroupByResult() throws Exception {
        server.respond(200, "{\"result\": [{\"timeframe\": {\"start\": \"s1\", \"end\": \"e1\"}, \"value\": [{\"g\": \"x\", \"result\": 2}]},"
                + "{\"timeframe\": {\"start\": \"s2\", \"end\": \"e2\"}, \"value\": [{\"g\": \"x\", \"result\": 5}]}]}");
        QueryResult r = client.execute(new SingleAnalysis.Builder(QueryType.COUNT).withEventCollection("c")
                .withTimeframe(new RelativeTimeframe("this_2_days")).withInterval("daily").withGroupBy("g").build());
        assertQueryRequest("count", "{\"event_collection\":\"c\",\"timeframe\":\"this_2_days\",\"interval\":\"daily\",\"group_by\":[\"g\"]}");
        assertTrue(r.isIntervalResult());
        List<IntervalResultValue> values = r.getIntervalResults();
        assertEquals(new AbsoluteTimeframe("s2", "e2"), values.get(1).getTimeframe());
        assertEquals(5L, values.get(1).getResult().getGroupResults().get(new Group(Map.of("g", "x"))).longValue());
    }

    @Test
    void multiAnalysis() throws Exception {
        server.respond(200, "{\"result\": {\"total\": 10, \"avg\": 2.5, \"p90\": 9}}");
        MultiAnalysis m = new MultiAnalysis.Builder()
                .withEventCollection("purchases")
                .withTimeframe(new RelativeTimeframe("this_7_days"))
                .withSubAnalysis(new SubAnalysis("total", QueryType.COUNT))
                .withSubAnalysis(new SubAnalysis("avg", QueryType.AVERAGE, "price"))
                .withSubAnalysis(new SubAnalysis("p90", QueryType.PERCENTILE, "price", Percentile.createStrict(90)))
                .build();
        MultiAnalysisResult r = (MultiAnalysisResult) client.execute(m);
        assertQueryRequest("multi_analysis", "{\"event_collection\":\"purchases\",\"timeframe\":\"this_7_days\","
                + "\"analyses\":{\"total\":{\"analysis_type\":\"count\"},"
                + "\"avg\":{\"analysis_type\":\"average\",\"target_property\":\"price\"},"
                + "\"p90\":{\"analysis_type\":\"percentile\",\"target_property\":\"price\",\"percentile\":90.0}}}");
        assertEquals(10L, r.getResultFor("total").longValue());
        assertEquals(2.5, r.getResultFor("avg").doubleValue());
    }

    @Test
    void funnelWithActors() throws Exception {
        server.respond(200, "{\"result\": [3, 1], \"actors\": [[\"a\", \"b\", \"c\"], [\"a\"]], \"steps\": []}");
        Funnel f = new Funnel.Builder()
                .withTimeframe(new RelativeTimeframe("this_14_days"))
                .withStep(new FunnelStep("signups", "user.id"))
                .withStep(new FunnelStep("purchases", "user.id", null,
                        List.of(new Filter("price", FilterOperator.GREATER_THAN_EQUAL, 5)), false, true, true))
                .build();
        FunnelResult r = (FunnelResult) client.execute(f);
        assertQueryRequest("funnel", "{\"steps\":[{\"event_collection\":\"signups\",\"actor_property\":\"user.id\"},"
                + "{\"event_collection\":\"purchases\",\"actor_property\":\"user.id\","
                + "\"filters\":[{\"property_name\":\"price\",\"operator\":\"gte\",\"property_value\":5}],"
                + "\"inverted\":false,\"optional\":true,\"with_actors\":true}],\"timeframe\":\"this_14_days\"}");
        assertEquals(3L, r.getFunnelResult().getListResults().get(0).longValue());
        assertEquals("a", r.getActorsResult().getListResults().get(1).getListResults().get(0).stringValue());
    }

    @Test
    void validation() {
        assertThrows(IllegalArgumentException.class, () -> new Query.Builder(QueryType.SUM).withEventCollection("c").build());
        assertThrows(IllegalArgumentException.class, () -> new Query.Builder(QueryType.PERCENTILE)
                .withEventCollection("c").withTargetProperty("p").build());
        assertThrows(IllegalArgumentException.class, () -> new SingleAnalysis.Builder(QueryType.COUNT).withEventCollection("c").build());
        assertThrows(IllegalArgumentException.class, () -> new Funnel.Builder().withStep(new FunnelStep("c", "a")).build());
        assertThrows(IllegalArgumentException.class, () -> Percentile.createStrict(0.001));
        assertThrows(IllegalArgumentException.class, () -> Percentile.createStrict(12.345));
        assertEquals(12.35, Percentile.createCoerced(12.345).asDouble());
        assertThrows(IllegalArgumentException.class, () -> RefreshRate.fromHours(1));
        assertEquals(14400, RefreshRate.fromHours(4));
    }

    @Test
    void serverErrorStatus() {
        server.respond(400, "{\"message\": \"Missing event_collection\", \"error_code\": \"MissingRequiredPropertyError\"}");
        ServerException e = assertThrows(ServerException.class, () -> client.count("c", new RelativeTimeframe("this_1_days")));
        assertEquals(400, e.getStatusCode());
        assertEquals("MissingRequiredPropertyError", e.getErrorCode());
        assertEquals("Missing event_collection", e.getServerMessage());
    }

    @Test
    void errorCodeInSuccessBody() {
        server.respond(200, "{\"message\": \"nope\", \"error_code\": \"QueryError\"}");
        IncisiveQueryClientException e = assertThrows(IncisiveQueryClientException.class,
                () -> client.count("c", new RelativeTimeframe("this_1_days")));
        assertEquals("QueryError", e.getErrorCode());
        assertEquals("Error response received from server QueryError: nope", e.getMessage());
    }

    @Test
    void emptySuccessBodyIsAnError() {
        server.respond(200, "");
        assertThrows(ServerException.class, () -> client.count("c", new RelativeTimeframe("this_1_days")));
    }

    @Test
    void baseUrlPrefixAndApiVersion() throws Exception {
        IncisiveQueryClient c = new IncisiveQueryClient.Builder(new IncisiveProject("p", null, "R"))
                .withEnvironment(new Environment(Collections.emptyMap()))
                .withBaseUrl(server.baseUrl() + "/api/").withApiVersion("4.0").build();
        server.respond(200, "{\"result\": 1}");
        c.count("c", new RelativeTimeframe("this_1_days"));
        assertEquals("/api/4.0/projects/p/queries/count", server.last().rawPath);
        assertEquals("R", server.last().header("Authorization"));
    }

    @Test
    void builderFromEnvironment() {
        Environment env = new Environment(Map.of("INCISIVE_PROJECT_ID", "envp", "INCISIVE_READ_KEY", "ER",
                "INCISIVE_BASE_URL", server.baseUrl()));
        IncisiveQueryClient c = new IncisiveQueryClient.Builder().withEnvironment(env).build();
        assertEquals("envp", c.getProject().getProjectId());
        assertThrows(IllegalStateException.class,
                () -> new IncisiveQueryClient.Builder().withEnvironment(new Environment(Collections.emptyMap())).build());
    }
}
