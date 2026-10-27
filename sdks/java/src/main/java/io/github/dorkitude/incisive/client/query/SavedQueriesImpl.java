package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.exceptions.ServerException;
import io.github.dorkitude.incisive.client.http.HttpMethods;
import io.github.dorkitude.incisive.client.query.result.QueryResult;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

final class SavedQueriesImpl implements SavedQueries {
    private final IncisiveQueryClient queryClient;

    SavedQueriesImpl(IncisiveQueryClient queryClient) {
        this.queryClient = queryClient;
    }

    @Override
    public Map<String, Object> createSavedQuery(String queryName, IncisiveQueryRequest query) throws IOException {
        return queryClient.getMapResponse(new SavedQueryPut(queryName, null, query, RefreshRate.NO_CACHING, null));
    }

    @Override
    public Map<String, Object> createSavedQuery(String queryName, IncisiveQueryRequest query, String displayName)
            throws IOException {
        return queryClient.getMapResponse(new SavedQueryPut(queryName, displayName, query, RefreshRate.NO_CACHING, null));
    }

    @Override
    public Map<String, Object> createCachedQuery(String queryName, IncisiveQueryRequest query, int refreshRate)
            throws IOException {
        return queryClient.getMapResponse(new SavedQueryPut(queryName, null, query, refreshRate, null));
    }

    @Override
    public Map<String, Object> createCachedQuery(String queryName, IncisiveQueryRequest query, String displayName,
                                                 int refreshRate) throws IOException {
        return queryClient.getMapResponse(new SavedQueryPut(queryName, displayName, query, refreshRate, null));
    }

    @Override
    public Map<String, Object> getDefinition(String queryName) throws IOException {
        if (queryName == null) {
            throw new IllegalArgumentException("A query name is required.");
        }
        return queryClient.getMapResponse(new SavedQueryRequest(HttpMethods.GET, true, queryName));
    }

    @Override
    @SuppressWarnings("unchecked")
    public List<Map<String, Object>> getAllDefinitions() throws IOException {
        List<Object> response = queryClient.getListResponse(new SavedQueryRequest(HttpMethods.GET, true, null));
        List<Map<String, Object>> out = new ArrayList<>(response.size());
        for (Object def : response) {
            if (!(def instanceof Map)) {
                throw new ServerException("Expected list of definitions to be a JSON array of JSON objects, "
                        + "but encountered this: " + def);
            }
            out.add((Map<String, Object>) def);
        }
        return out;
    }

    @Override
    @SuppressWarnings("unchecked")
    public QueryResult getResult(String queryName) throws IOException {
        if (queryName == null) {
            throw new IllegalArgumentException("A query name is required.");
        }
        SavedQueryRequest request = new SavedQueryRequest(HttpMethods.GET, false, queryName) {
            @Override
            boolean retrievingResults() {
                return true;
            }
        };
        Map<String, Object> response = queryClient.getMapResponse(request);

        Object queryObj = response.get(QueryConstants.QUERY);
        if (!(queryObj instanceof Map)) {
            throw new ServerException("Null, missing or malformed '" + QueryConstants.QUERY + "' property.");
        }
        Map<String, Object> query = (Map<String, Object>) queryObj;

        Collection<String> groupByParams = Collections.emptyList();
        Object groupByObj = query.get(QueryConstants.GROUP_BY);
        if (groupByObj instanceof List) {
            List<String> params = new ArrayList<>();
            for (Object p : (List<Object>) groupByObj) {
                params.add(String.valueOf(p));
            }
            groupByParams = params;
        } else if (groupByObj instanceof String) {
            groupByParams = Collections.singletonList((String) groupByObj);
        } else if (groupByObj != null) {
            throw new ServerException("Property '" + QueryConstants.GROUP_BY + "' was not a List type.");
        }
        boolean isGroupBy = !groupByParams.isEmpty();
        boolean isInterval = query.get(QueryConstants.INTERVAL) instanceof String;

        Object analysisTypeObj = query.get(QueryConstants.ANALYSIS_TYPE);
        if (analysisTypeObj == null) {
            throw new ServerException("The '" + QueryConstants.ANALYSIS_TYPE + "' property is missing.");
        }
        String analysisType = analysisTypeObj.toString().trim();
        boolean isMultiAnalysis = QueryConstants.MULTI_ANALYSIS.equals(analysisType);
        boolean isFunnel = QueryConstants.FUNNEL.equals(analysisType);

        if (!response.containsKey(QueryConstants.RESULT)) {
            throw new ServerException("The '" + QueryConstants.RESULT + "' property is missing.");
        }
        Object resultObj = response.get(QueryConstants.RESULT);
        Map<String, Object> wrapped;
        if (!isMultiAnalysis && resultObj instanceof Map) {
            // e.g. a funnel result object {"result": [...], "actors": [...]}
            wrapped = (Map<String, Object>) resultObj;
        } else {
            wrapped = new LinkedHashMap<>();
            wrapped.put(QueryConstants.RESULT, resultObj);
        }
        return queryClient.rawMapResponseToQueryResult(wrapped, isGroupBy, isInterval, isMultiAnalysis, isFunnel,
                groupByParams);
    }

    @Override
    @SuppressWarnings("unchecked")
    public Map<String, Object> updateQuery(String queryName, Map<String, ?> updates) throws IOException {
        Map<String, Object> oldDef = getDefinition(queryName);
        if (oldDef == null || oldDef.isEmpty()) {
            throw new ServerException("The existing saved/cached query definition found was null or empty.");
        }
        Map<String, Object> newDef = new LinkedHashMap<>();
        newDef.put(QueryConstants.QUERY_NAME, oldDef.get(QueryConstants.QUERY_NAME));
        newDef.put(QueryConstants.REFRESH_RATE, oldDef.get(QueryConstants.REFRESH_RATE));
        if (oldDef.containsKey(QueryConstants.METADATA)) {
            newDef.put(QueryConstants.METADATA, oldDef.get(QueryConstants.METADATA));
        }
        Object oldQueryObj = oldDef.get(QueryConstants.QUERY);
        if (!(oldQueryObj instanceof Map)) {
            throw new ServerException("Null, missing or malformed '" + QueryConstants.QUERY + "' property.");
        }
        Map<String, Object> newQuery = new LinkedHashMap<>();
        for (Map.Entry<String, Object> e : ((Map<String, Object>) oldQueryObj).entrySet()) {
            if (isMeaningful(e.getValue())) {
                newQuery.put(e.getKey(), e.getValue());
            }
        }
        newDef.put(QueryConstants.QUERY, newQuery);
        deepUpdate(newDef, updates);
        return queryClient.getMapResponse(new SavedQueryPut(queryName, newDef));
    }

    /** Drops nulls and empty strings/maps/lists from a fetched definition before re-PUTting it. */
    private static boolean isMeaningful(Object v) {
        if (v == null) return false;
        if (v instanceof String) return !((String) v).trim().isEmpty();
        if (v instanceof Map) return !((Map<?, ?>) v).isEmpty();
        if (v instanceof List) return !((List<?>) v).isEmpty();
        return true;
    }

    @SuppressWarnings("unchecked")
    private static void deepUpdate(Map<String, Object> target, Map<String, ?> updates) {
        for (Map.Entry<String, ?> e : updates.entrySet()) {
            Object existing = target.get(e.getKey());
            if (e.getValue() instanceof Map && existing instanceof Map) {
                Map<String, Object> merged = new LinkedHashMap<>((Map<String, Object>) existing);
                deepUpdate(merged, (Map<String, ?>) e.getValue());
                target.put(e.getKey(), merged);
            } else {
                target.put(e.getKey(), e.getValue());
            }
        }
    }

    @Override
    public Map<String, Object> updateQueryFull(String queryName, Map<String, ?> fullDefinition) throws IOException {
        return queryClient.getMapResponse(new SavedQueryPut(queryName, fullDefinition));
    }

    @Override
    public void deleteQuery(String queryName) throws IOException {
        queryClient.getMapResponse(new SavedQueryRequest(HttpMethods.DELETE, true, queryName));
    }

    @Override
    public Map<String, Object> setQueryName(String queryName, String newQueryName) throws IOException {
        PersistentAnalysis.validateResourceName(newQueryName);
        Map<String, Object> updates = new LinkedHashMap<>();
        updates.put(QueryConstants.QUERY_NAME, newQueryName);
        return updateQuery(queryName, updates);
    }

    @Override
    public Map<String, Object> setRefreshRate(String queryName, int refreshRate) throws IOException {
        RefreshRate.validateRefreshRate(refreshRate);
        Map<String, Object> updates = new LinkedHashMap<>();
        updates.put(QueryConstants.REFRESH_RATE, refreshRate);
        return updateQuery(queryName, updates);
    }

    @Override
    public Map<String, Object> setDisplayName(String queryName, String displayName) throws IOException {
        PersistentAnalysis.validateDisplayName(displayName);
        Map<String, Object> metadata = new LinkedHashMap<>();
        metadata.put(QueryConstants.DISPLAY_NAME, displayName);
        Map<String, Object> updates = new LinkedHashMap<>();
        updates.put(QueryConstants.METADATA, metadata);
        return updateQuery(queryName, updates);
    }
}
