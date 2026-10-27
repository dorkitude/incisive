package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.Environment;
import io.github.dorkitude.incisive.client.IncisiveProject;
import io.github.dorkitude.incisive.client.MockServer;
import io.github.dorkitude.incisive.client.query.result.Group;
import io.github.dorkitude.incisive.client.query.result.IntervalResultValue;
import io.github.dorkitude.incisive.client.query.result.QueryResult;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import static io.github.dorkitude.incisive.client.MockServer.json;
import static org.junit.jupiter.api.Assertions.*;

class SavedQueriesAndDatasetsTest {
    private MockServer server;
    private IncisiveQueryClient client;
    private SavedQueries saved;
    private CachedDatasets datasets;

    @BeforeEach
    void setUp() throws Exception {
        server = new MockServer();
        client = new IncisiveQueryClient.Builder(new IncisiveProject("proj1", "WRITE", "READ", "MASTER"))
                .withEnvironment(new Environment(Collections.emptyMap()))
                .withBaseUrl(server.baseUrl())
                .build();
        saved = client.getSavedQueriesInterface();
        datasets = client.getCachedDatasetsClient();
    }

    @AfterEach
    void tearDown() {
        server.close();
    }

    private static Query countQuery() {
        return new Query.Builder(QueryType.COUNT).withEventCollection("purchases")
                .withTimeframe(new RelativeTimeframe("this_7_days")).build();
    }

    // ----- saved queries ------------------------------------------------------------------

    @Test
    void createSavedQuery() throws Exception {
        server.respond(201, "{\"query_name\": \"my-count\", \"refresh_rate\": 0}");
        Map<String, Object> resp = saved.createSavedQuery("my-count", countQuery(), "My Count");
        assertEquals("my-count", resp.get("query_name"));
        MockServer.Recorded r = server.last();
        assertEquals("PUT", r.method);
        assertEquals("/1.0/projects/proj1/queries/saved/my-count", r.rawPath);
        assertEquals("MASTER", r.header("Authorization"));
        assertEquals("application/json", r.header("Content-Type"));
        assertEquals(json("{\"query\":{\"event_collection\":\"purchases\",\"timeframe\":\"this_7_days\",\"analysis_type\":\"count\"},"
                + "\"refresh_rate\":0,\"metadata\":{\"display_name\":\"My Count\"}}"), r.json());
    }

    @Test
    void createCachedQuery() throws Exception {
        server.respond(201, "{}");
        saved.createCachedQuery("cached_1", countQuery(), RefreshRate.fromHours(6));
        assertEquals(json("{\"query\":{\"event_collection\":\"purchases\",\"timeframe\":\"this_7_days\",\"analysis_type\":\"count\"},"
                + "\"refresh_rate\":21600}"), server.last().json());
        assertThrows(IllegalArgumentException.class, () -> saved.createCachedQuery("x", countQuery(), 60));
        assertThrows(IllegalArgumentException.class, () -> saved.createSavedQuery("bad name!", countQuery()));
    }

    @Test
    void getDefinitionsAndDelete() throws Exception {
        server.respond(200, "{\"query_name\": \"q\", \"query\": {\"analysis_type\": \"count\"}}");
        assertEquals("q", saved.getDefinition("q").get("query_name"));
        assertEquals("GET", server.last().method);
        assertEquals("/1.0/projects/proj1/queries/saved/q", server.last().rawPath);
        assertEquals("MASTER", server.last().header("Authorization"));
        assertNull(server.last().header("Content-Type"));
        assertEquals("", server.last().body);

        server.respond(200, "[{\"query_name\": \"a\"}, {\"query_name\": \"b\"}]");
        List<Map<String, Object>> all = saved.getAllDefinitions();
        assertEquals(2, all.size());
        assertEquals("b", all.get(1).get("query_name"));
        assertEquals("/1.0/projects/proj1/queries/saved", server.last().rawPath);

        server.respond(204, null);
        saved.deleteQuery("q");
        assertEquals("DELETE", server.last().method);
        assertEquals("/1.0/projects/proj1/queries/saved/q", server.last().rawPath);
        assertEquals("MASTER", server.last().header("Authorization"));
    }

    @Test
    void getResultUsesReadKeyAndQueryShape() throws Exception {
        server.respond(200, "{\"query_name\": \"q\", \"query\": {\"analysis_type\": \"count\", \"group_by\": [\"country\"], \"interval\": null},"
                + " \"result\": [{\"country\": \"US\", \"result\": 4}]}");
        QueryResult r = saved.getResult("q");
        assertEquals("GET", server.last().method);
        assertEquals("/1.0/projects/proj1/queries/saved/q/result", server.last().rawPath);
        assertEquals("READ", server.last().header("Authorization"));
        assertEquals(4L, r.getGroupResults().get(new Group(Map.of("country", "US"))).longValue());

        server.respond(200, "{\"query\": {\"analysis_type\": \"sum\"}, \"result\": 12.5}");
        assertEquals(12.5, saved.getResult("q").doubleValue());
    }

    @Test
    void updateQueryMergesAndPuts() throws Exception {
        server.respond(200, "{\"query_name\": \"q\", \"refresh_rate\": 0, \"metadata\": {\"display_name\": \"Old\", \"x\": 1},"
                + " \"query\": {\"analysis_type\": \"count\", \"event_collection\": \"c\", \"timeframe\": \"this_1_days\","
                + " \"filters\": [], \"timezone\": null, \"max_age\": 300}, \"run_information\": {}}");
        server.respond(200, "{\"query_name\": \"q\"}");
        saved.setDisplayName("q", "New");
        List<MockServer.Recorded> reqs = server.requests();
        assertEquals("GET", reqs.get(0).method);
        MockServer.Recorded put = reqs.get(1);
        assertEquals("PUT", put.method);
        assertEquals("/1.0/projects/proj1/queries/saved/q", put.rawPath);
        assertEquals("MASTER", put.header("Authorization"));
        assertEquals(json("{\"query_name\":\"q\",\"refresh_rate\":0,\"metadata\":{\"display_name\":\"New\",\"x\":1},"
                + "\"query\":{\"analysis_type\":\"count\",\"event_collection\":\"c\",\"timeframe\":\"this_1_days\",\"max_age\":300}}"),
                put.json());
    }

    // ----- cached datasets ----------------------------------------------------------------

    private static final String DEFINITION = "{\"dataset_name\": \"ds1\", \"display_name\": \"DS\", \"project_id\": \"proj1\","
            + " \"organization_id\": \"org\", \"last_scheduled_date\": \"2026-01-01\", \"latest_subtimeframe_available\": \"2026-01-02\","
            + " \"milliseconds_behind\": 5000, \"index_by\": [\"product.id\"],"
            + " \"query\": {\"project_id\": \"proj1\", \"analysis_type\": \"count\", \"event_collection\": \"purchases\","
            + " \"timeframe\": \"this_500_days\", \"interval\": \"daily\", \"filters\": [{\"property_name\": \"p\", \"operator\": \"eq\", \"property_value\": 1}]}}";

    @Test
    void createDataset() throws Exception {
        server.respond(201, DEFINITION);
        DatasetQuery q = DatasetQuery.DatasetQueryBuilder.aDatasetQuery()
                .withAnalysisType("count").withEventCollection("purchases").withTimeframe("this_500_days")
                .withInterval("daily").withFilters(List.of(new Filter("p", FilterOperator.EQUAL_TO, 1))).build();
        DatasetDefinition d = datasets.create("ds1", "DS", q, List.of("product.id"));
        MockServer.Recorded r = server.last();
        assertEquals("PUT", r.method);
        assertEquals("/1.0/projects/proj1/datasets/ds1", r.rawPath);
        assertEquals("MASTER", r.header("Authorization"));
        assertEquals(json("{\"display_name\":\"DS\",\"index_by\":[\"product.id\"],\"query\":{\"analysis_type\":\"count\","
                + "\"event_collection\":\"purchases\",\"timeframe\":\"this_500_days\",\"interval\":\"daily\","
                + "\"filters\":[{\"property_name\":\"p\",\"operator\":\"eq\",\"property_value\":1}]}}"), r.json());
        assertEquals("ds1", d.getDatasetName());
        assertEquals("5000", d.getMillisecondsBehind());
        assertEquals(List.of("product.id"), d.getIndexBy());
        assertEquals("purchases", d.getQuery().getEventCollection());
        assertEquals(new Filter("p", FilterOperator.EQUAL_TO, 1), d.getQuery().getFilters().get(0));
    }

    @Test
    void getAndListDatasets() throws Exception {
        server.respond(200, DEFINITION);
        DatasetDefinition d = datasets.getDefinition("ds1");
        assertEquals("GET", server.last().method);
        assertEquals("/1.0/projects/proj1/datasets/ds1", server.last().rawPath);
        assertEquals("READ", server.last().header("Authorization"));
        assertEquals("DS", d.getDisplayName());

        server.respond(200, "{\"datasets\": [" + DEFINITION + "], \"next_page_url\": null}");
        List<DatasetDefinition> list = datasets.getDefinitions(10, "ds0");
        assertEquals(1, list.size());
        assertEquals(d, list.get(0));
        assertEquals("/1.0/projects/proj1/datasets", server.last().rawPath);
        assertEquals(Map.of("limit", "10", "after_name", "ds0"), server.last().query());

        server.respond(200, "{}");
        assertTrue(datasets.getDefinitions().isEmpty());
        assertNull(server.last().rawQuery);
    }

    @Test
    void datasetResults() throws Exception {
        DatasetDefinition d = DatasetDefinition.DatasetDefinitionBuilder.aDatasetDefinition()
                .withDatasetName("ds1").withIndexBy(List.of("product.id"))
                .withQuery(DatasetQuery.DatasetQueryBuilder.aDatasetQuery().withAnalysisType("count").build())
                .build();
        server.respond(200, "{\"result\": [{\"timeframe\": {\"start\": \"s\", \"end\": \"e\"}, \"value\": 11}]}");
        Map<String, Object> index = new LinkedHashMap<>();
        index.put("product.id", "abc def");
        List<IntervalResultValue> values = datasets.getResults(d, index, new RelativeTimeframe("this_3_days"));
        MockServer.Recorded r = server.last();
        assertEquals("GET", r.method);
        assertEquals("/1.0/projects/proj1/datasets/ds1/results", r.rawPath);
        assertEquals("READ", r.header("Authorization"));
        Map<String, String> q = r.query();
        assertEquals(json("{\"product.id\":\"abc def\"}"), json(q.get("index_by")));
        assertEquals("\"this_3_days\"", q.get("timeframe"), "relative timeframe is a JSON string");
        assertEquals(11L, values.get(0).getResult().longValue());

        server.respond(200, "{\"result\": []}");
        datasets.getResults(d, index, new AbsoluteTimeframe("2026-01-01", "2026-01-02"));
        assertEquals(json("{\"start\":\"2026-01-01\",\"end\":\"2026-01-02\"}"), json(server.last().query().get("timeframe")));

        assertThrows(IllegalArgumentException.class, () -> datasets.getResults(d, Map.of("other", 1), new RelativeTimeframe("this_1_days")));
    }

    @Test
    void deleteDataset() throws Exception {
        server.respond(204, null);
        assertTrue(datasets.delete("ds1"));
        assertEquals("DELETE", server.last().method);
        assertEquals("/1.0/projects/proj1/datasets/ds1", server.last().rawPath);
        assertEquals("MASTER", server.last().header("Authorization"));
    }
}
