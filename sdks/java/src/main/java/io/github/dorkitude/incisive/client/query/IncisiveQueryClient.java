package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.Environment;
import io.github.dorkitude.incisive.client.IncisiveConstants;
import io.github.dorkitude.incisive.client.IncisiveJsonHandler;
import io.github.dorkitude.incisive.client.IncisiveLogging;
import io.github.dorkitude.incisive.client.IncisiveProject;
import io.github.dorkitude.incisive.client.JacksonJsonHandler;
import io.github.dorkitude.incisive.client.exceptions.IncisiveQueryClientException;
import io.github.dorkitude.incisive.client.exceptions.ServerException;
import io.github.dorkitude.incisive.client.http.HttpHandler;
import io.github.dorkitude.incisive.client.http.HttpMethods;
import io.github.dorkitude.incisive.client.http.OutputSource;
import io.github.dorkitude.incisive.client.http.Request;
import io.github.dorkitude.incisive.client.http.Response;
import io.github.dorkitude.incisive.client.http.UrlConnectionHttpHandler;
import io.github.dorkitude.incisive.client.query.result.DoubleResult;
import io.github.dorkitude.incisive.client.query.result.FunnelResult;
import io.github.dorkitude.incisive.client.query.result.Group;
import io.github.dorkitude.incisive.client.query.result.GroupByResult;
import io.github.dorkitude.incisive.client.query.result.IntervalResult;
import io.github.dorkitude.incisive.client.query.result.IntervalResultValue;
import io.github.dorkitude.incisive.client.query.result.ListResult;
import io.github.dorkitude.incisive.client.query.result.LongResult;
import io.github.dorkitude.incisive.client.query.result.MultiAnalysisResult;
import io.github.dorkitude.incisive.client.query.result.QueryResult;
import io.github.dorkitude.incisive.client.query.result.StringResult;

import java.io.IOException;
import java.io.OutputStreamWriter;
import java.io.StringReader;
import java.io.Writer;
import java.math.BigInteger;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Query client. Analyses are sent as {@code POST {base}/{version}/projects/{id}/queries/{type}}
 * with a JSON body and the read key in the {@code Authorization} header; the {@code result} field
 * of the response is parsed into a {@link QueryResult}.
 *
 * <p>Non-2xx responses raise {@link ServerException} (message = raw body; status, error_code and
 * message available via getters). A 2xx body with {@code error_code} and no {@code result}
 * raises {@link IncisiveQueryClientException}.
 */
public class IncisiveQueryClient {
    private final IncisiveJsonHandler jsonHandler;
    private final RequestUrlBuilder requestUrlBuilder;
    private final IncisiveProject project;
    private final HttpHandler httpHandler;
    private final int connectTimeout;
    private final int readTimeout;

    protected IncisiveQueryClient(Builder builder) {
        this.httpHandler = builder.httpHandler;
        this.jsonHandler = builder.jsonHandler;
        this.requestUrlBuilder = new RequestUrlBuilder(builder.apiVersion, builder.baseUrl);
        this.project = builder.project;
        this.connectTimeout = builder.connectTimeout;
        this.readTimeout = builder.readTimeout;
    }

    public IncisiveProject getProject() {
        return project;
    }

    // ----- simple analyses ------------------------------------------------------------------

    public long count(String eventCollection, Timeframe timeframe) throws IOException {
        return queryResultToLong(execute(new Query.Builder(QueryType.COUNT)
                .withEventCollection(eventCollection).withTimeframe(timeframe).build()));
    }

    public long countUnique(String eventCollection, String targetProperty, Timeframe timeframe) throws IOException {
        return queryResultToLong(execute(simple(QueryType.COUNT_UNIQUE, eventCollection, targetProperty, timeframe)));
    }

    public double minimum(String eventCollection, String targetProperty, Timeframe timeframe) throws IOException {
        return queryResultToDouble(execute(simple(QueryType.MINIMUM, eventCollection, targetProperty, timeframe)));
    }

    public double maximum(String eventCollection, String targetProperty, Timeframe timeframe) throws IOException {
        return queryResultToDouble(execute(simple(QueryType.MAXIMUM, eventCollection, targetProperty, timeframe)));
    }

    public double average(String eventCollection, String targetProperty, Timeframe timeframe) throws IOException {
        return queryResultToDouble(execute(simple(QueryType.AVERAGE, eventCollection, targetProperty, timeframe)));
    }

    public double median(String eventCollection, String targetProperty, Timeframe timeframe) throws IOException {
        return queryResultToDouble(execute(simple(QueryType.MEDIAN, eventCollection, targetProperty, timeframe)));
    }

    public double percentile(String eventCollection, String targetProperty, Double percentile, Timeframe timeframe)
            throws IOException {
        return queryResultToDouble(execute(new Query.Builder(QueryType.PERCENTILE)
                .withEventCollection(eventCollection).withTargetProperty(targetProperty)
                .withPercentile(percentile).withTimeframe(timeframe).build()));
    }

    public double sum(String eventCollection, String targetProperty, Timeframe timeframe) throws IOException {
        return queryResultToDouble(execute(simple(QueryType.SUM, eventCollection, targetProperty, timeframe)));
    }

    public QueryResult selectUnique(String eventCollection, String targetProperty, Timeframe timeframe)
            throws IOException {
        return execute(simple(QueryType.SELECT_UNIQUE, eventCollection, targetProperty, timeframe));
    }

    public double standardDeviation(String eventCollection, String targetProperty, Timeframe timeframe)
            throws IOException {
        return queryResultToDouble(execute(simple(QueryType.STANDARD_DEVIATION, eventCollection, targetProperty, timeframe)));
    }

    private static Query simple(QueryType type, String collection, String target, Timeframe timeframe) {
        return new Query.Builder(type).withEventCollection(collection).withTargetProperty(target)
                .withTimeframe(timeframe).build();
    }

    /** Executes any request and parses its result according to the request's shape. */
    public QueryResult execute(IncisiveQueryRequest request) throws IOException {
        Map<String, Object> response = getMapResponse(request);
        boolean isGroupBy = request.groupedResponseExpected();
        Collection<String> groupByParams = isGroupBy ? request.getGroupByParams() : Collections.<String>emptyList();
        return rawMapResponseToQueryResult(response, isGroupBy, request.intervalResponseExpected(),
                QueryConstants.MULTI_ANALYSIS.equals(request.getAnalysisType()), request instanceof Funnel,
                groupByParams);
    }

    public SavedQueries getSavedQueriesInterface() {
        return new SavedQueriesImpl(this);
    }

    public CachedDatasets getCachedDatasetsClient() {
        return new CachedDatasetsClient(this);
    }

    // ----- transport ------------------------------------------------------------------------

    @SuppressWarnings("unchecked")
    Map<String, Object> getMapResponse(IncisiveQueryRequest request) throws IOException {
        Object response = getResponse(request);
        if (!(response instanceof Map)) {
            throw new IncisiveQueryClientException("Root of response was not a 'Map' as expected.");
        }
        return (Map<String, Object>) response;
    }

    @SuppressWarnings("unchecked")
    List<Object> getListResponse(IncisiveQueryRequest request) throws IOException {
        Object response = getResponse(request);
        if (!(response instanceof List)) {
            throw new IncisiveQueryClientException("Root of response was not a 'List' as expected.");
        }
        return (List<Object>) response;
    }

    private Object getResponse(IncisiveQueryRequest request) throws IOException {
        URL url = request.getRequestURL(requestUrlBuilder, project.getProjectId());
        Map<String, Object> wrapped = sendRequest(url, request.getHttpMethod(), request.getAuthKey(project),
                request.constructRequestArgs());
        if (wrapped.containsKey(IncisiveConstants.FAKE_JSON_ROOT)) {
            return wrapped.get(IncisiveConstants.FAKE_JSON_ROOT);
        }
        return wrapped;
    }

    private Map<String, Object> sendRequest(URL url, String method, String authKey, final Map<String, ?> requestData)
            throws IOException {
        boolean bodyless = HttpMethods.GET.equals(method) || HttpMethods.DELETE.equals(method);
        if (bodyless && requestData != null && !requestData.isEmpty()) {
            throw new IllegalStateException("Trying to send a " + method + " request with a request body.");
        }
        OutputSource source = bodyless ? null : out -> {
            Writer writer = new OutputStreamWriter(out, StandardCharsets.UTF_8);
            jsonHandler.writeJson(writer, requestData == null ? Collections.emptyMap() : requestData);
            writer.flush();
        };
        Request request = new Request(url, method, authKey, source, null, connectTimeout, readTimeout);
        Response response = httpHandler.execute(request);
        IncisiveLogging.log(String.format("%s %s -> %d '%s'", method, url, response.statusCode, response.body));

        if (!response.isSuccess()) {
            throw new ServerException(response.statusCode, response.body);
        }
        boolean empty = response.body == null || response.body.trim().isEmpty();
        if (empty && response.statusCode != HttpURLConnection.HTTP_NO_CONTENT) {
            throw new ServerException(response.statusCode, "Empty response when response was expected.");
        }
        Map<String, Object> responseMap = response.statusCode == HttpURLConnection.HTTP_NO_CONTENT || empty
                ? Collections.<String, Object>emptyMap()
                : jsonHandler.readJson(new StringReader(response.body));

        if (responseMap.get(QueryConstants.RESULT) == null && responseMap.containsKey(QueryConstants.ERROR_CODE)) {
            Object errorCode = responseMap.get(QueryConstants.ERROR_CODE);
            Object message = responseMap.get(QueryConstants.MESSAGE);
            String text = "Error response received from server";
            if (errorCode != null) text += " " + errorCode;
            if (message != null) text += ": " + message;
            throw new IncisiveQueryClientException(text, errorCode == null ? null : errorCode.toString());
        }
        return responseMap;
    }

    // ----- result parsing -------------------------------------------------------------------

    QueryResult rawMapResponseToQueryResult(Map<String, Object> response, boolean isGroupBy, boolean isInterval,
                                            boolean isMultiAnalysis, boolean isFunnel,
                                            Collection<String> groupByParams) {
        if (isFunnel) {
            return constructFunnelResult(response);
        }
        return constructQueryResult(response.get(QueryConstants.RESULT), isGroupBy, isInterval, isMultiAnalysis,
                groupByParams);
    }

    private static QueryResult constructQueryResult(Object input, boolean isGroupBy, boolean isInterval) {
        return constructQueryResult(input, isGroupBy, isInterval, false, Collections.<String>emptyList());
    }

    @SuppressWarnings("unchecked")
    private static QueryResult constructQueryResult(Object input, boolean isGroupBy, boolean isInterval,
                                                    boolean isMultiAnalysis, Collection<String> groupByParams) {
        if (isGroupBy && (groupByParams == null || groupByParams.isEmpty())) {
            throw new IllegalArgumentException("If we expect a GroupByResult, then 'groupByParams' are required.");
        }
        if (input instanceof Integer || input instanceof Long || input instanceof Short || input instanceof BigInteger) {
            return new LongResult(((Number) input).longValue());
        }
        if (input instanceof Number) {
            return new DoubleResult(((Number) input).doubleValue());
        }
        if (input instanceof String) {
            return new StringResult((String) input);
        }
        if (input instanceof Map) {
            if (!isMultiAnalysis) {
                throw new IllegalStateException("Received a JSON dictionary result when not expecting a MultiAnalysisResult.");
            }
            Map<String, QueryResult> sub = new LinkedHashMap<>();
            for (Map.Entry<String, Object> e : ((Map<String, Object>) input).entrySet()) {
                if (e.getValue() instanceof Map) {
                    throw new IllegalStateException("A MultiAnalysis sub-result was a nested JSON dictionary.");
                }
                sub.put(e.getKey(), constructQueryResult(e.getValue(), false, false));
            }
            return new MultiAnalysisResult(sub);
        }
        if (input instanceof List) {
            List<Object> list = (List<Object>) input;
            if (isInterval) {
                return constructIntervalResult(list, isGroupBy, isMultiAnalysis, groupByParams);
            }
            if (isGroupBy) {
                return constructGroupByResult(list, groupByParams, isMultiAnalysis);
            }
            List<QueryResult> out = new ArrayList<>();
            for (Object child : list) {
                out.add(constructQueryResult(child, false, false));
            }
            return new ListResult(out);
        }
        return null;
    }

    @SuppressWarnings("unchecked")
    private static IntervalResult constructIntervalResult(List<Object> intervals, boolean isGroupBy,
                                                          boolean isMultiAnalysis, Collection<String> groupByParams) {
        List<IntervalResultValue> out = new ArrayList<>();
        for (Object child : intervals) {
            if (!(child instanceof Map)) {
                throw new IllegalStateException("IntervalResult entries should be JSON objects.");
            }
            Map<String, Object> m = (Map<String, Object>) child;
            if (!m.containsKey(QueryConstants.TIMEFRAME) || !m.containsKey(QueryConstants.VALUE)) {
                throw new IllegalStateException("IntervalResult is missing \"timeframe\" and \"value\" keys.");
            }
            Object tf = m.get(QueryConstants.TIMEFRAME);
            if (!(tf instanceof Map)) {
                throw new IllegalStateException("IntervalResult timeframe should be a JSON object.");
            }
            Map<String, Object> tfm = (Map<String, Object>) tf;
            AbsoluteTimeframe timeframe = new AbsoluteTimeframe(
                    tfm.get(QueryConstants.START) == null ? null : tfm.get(QueryConstants.START).toString(),
                    tfm.get(QueryConstants.END) == null ? null : tfm.get(QueryConstants.END).toString());
            QueryResult value = constructQueryResult(m.get(QueryConstants.VALUE), isGroupBy, false, isMultiAnalysis,
                    groupByParams);
            out.add(new IntervalResultValue(timeframe, value));
        }
        return new IntervalResult(out);
    }

    @SuppressWarnings("unchecked")
    private static GroupByResult constructGroupByResult(List<Object> groups, Collection<String> groupByParams,
                                                        boolean isMultiAnalysis) {
        Map<Group, QueryResult> out = new LinkedHashMap<>();
        for (Object child : groups) {
            if (!(child instanceof Map)) {
                throw new IllegalStateException("GroupBy result entries should be JSON objects.");
            }
            Map<String, Object> properties = new LinkedHashMap<>();
            Map<String, Object> results = new LinkedHashMap<>();
            for (Map.Entry<String, Object> e : ((Map<String, Object>) child).entrySet()) {
                if (groupByParams.contains(e.getKey())) {
                    properties.put(e.getKey(), e.getValue());
                } else {
                    results.put(e.getKey(), e.getValue());
                }
            }
            QueryResult result;
            if (results.isEmpty()) {
                throw new IllegalStateException("There were no results in the GroupBy result.");
            } else if (isMultiAnalysis) {
                result = constructQueryResult(results, false, false, true, groupByParams);
            } else {
                if (!results.containsKey(QueryConstants.RESULT)) {
                    throw new IllegalStateException("Single analysis GroupBy result is missing \"result\" key.");
                }
                result = constructQueryResult(results.get(QueryConstants.RESULT), false, false);
            }
            out.put(new Group(properties), result);
        }
        return new GroupByResult(out);
    }

    private static FunnelResult constructFunnelResult(Map<String, Object> response) {
        QueryResult steps = constructQueryResult(response.get(QueryConstants.RESULT), false, false);
        if (!(steps instanceof ListResult)) {
            throw new IncisiveQueryClientException("'result' property of response contained data of an unexpected format.");
        }
        ListResult actors = null;
        if (response.containsKey(QueryConstants.ACTORS) && response.get(QueryConstants.ACTORS) != null) {
            QueryResult a = constructQueryResult(response.get(QueryConstants.ACTORS), false, false);
            if (!(a instanceof ListResult)) {
                throw new IncisiveQueryClientException("'actors' property of response contained data of an unexpected format.");
            }
            actors = (ListResult) a;
        }
        return new FunnelResult((ListResult) steps, actors);
    }

    private static long queryResultToLong(QueryResult result) {
        if (result == null) {
            throw new NullPointerException("Query Error: expected long response type but received null.");
        }
        if (result.isLong()) {
            return result.longValue();
        }
        throw new IllegalStateException("Query Error: expected long response type.");
    }

    private static double queryResultToDouble(QueryResult result) {
        if (result == null) {
            throw new NullPointerException("Query Error: expected double response type but received null.");
        }
        if (result.isDouble()) {
            return result.doubleValue();
        }
        if (result.isLong()) {
            return result.longValue();
        }
        throw new IllegalStateException("Query Error: expected double response type.");
    }

    // ----- builder --------------------------------------------------------------------------

    /** Builder. Base URL defaults to INCISIVE_BASE_URL, else http://localhost:8080; API version to "1.0". */
    public static class Builder {
        private HttpHandler httpHandler;
        private IncisiveJsonHandler jsonHandler;
        private String baseUrl;
        private String apiVersion;
        private IncisiveProject project;
        private Environment environment;
        private int connectTimeout = 30000;
        private int readTimeout = 30000;

        /** Uses a project built from INCISIVE_* environment variables. */
        public Builder() {
            this(null);
        }

        public Builder(IncisiveProject project) {
            this.project = project;
        }

        protected HttpHandler getDefaultHttpHandler() throws Exception {
            return new UrlConnectionHttpHandler();
        }

        protected IncisiveJsonHandler getDefaultJsonHandler() throws Exception {
            return new JacksonJsonHandler();
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

        public String getBaseURL() {
            return baseUrl;
        }

        public void setBaseUrl(String baseUrl) {
            this.baseUrl = baseUrl;
        }

        public Builder withBaseUrl(String baseUrl) {
            setBaseUrl(baseUrl);
            return this;
        }

        public Builder withApiVersion(String apiVersion) {
            this.apiVersion = apiVersion;
            return this;
        }

        public IncisiveProject getProject() {
            return project;
        }

        public void setProject(IncisiveProject project) {
            this.project = project;
        }

        public Builder withEnvironment(Environment environment) {
            this.environment = environment;
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

        public IncisiveQueryClient build() {
            Environment env = environment != null ? environment : new Environment();
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
            if (project == null && env.getProjectId() != null) {
                project = new IncisiveProject(env);
            }
            if (project == null) {
                throw new IllegalStateException("Cannot build IncisiveQueryClient with null project.");
            }
            if (baseUrl == null) {
                baseUrl = env.getBaseUrl() != null ? env.getBaseUrl() : IncisiveConstants.DEFAULT_BASE_URL;
            }
            if (apiVersion == null) {
                apiVersion = IncisiveConstants.DEFAULT_API_VERSION;
            }
            return buildInstance();
        }

        protected IncisiveQueryClient buildInstance() {
            return new IncisiveQueryClient(this);
        }
    }
}
