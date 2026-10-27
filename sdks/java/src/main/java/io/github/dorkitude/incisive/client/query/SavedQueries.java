package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.query.result.QueryResult;

import java.io.IOException;
import java.util.List;
import java.util.Map;

/**
 * Saved and cached query management. Obtain via {@link IncisiveQueryClient#getSavedQueriesInterface()}.
 * Create/update/delete/definition calls use the master key; {@link #getResult} uses the read key.
 */
public interface SavedQueries {
    Map<String, Object> createSavedQuery(String queryName, IncisiveQueryRequest query) throws IOException;

    Map<String, Object> createSavedQuery(String queryName, IncisiveQueryRequest query, String displayName) throws IOException;

    Map<String, Object> createCachedQuery(String queryName, IncisiveQueryRequest query, int refreshRate) throws IOException;

    Map<String, Object> createCachedQuery(String queryName, IncisiveQueryRequest query, String displayName,
                                          int refreshRate) throws IOException;

    Map<String, Object> getDefinition(String queryName) throws IOException;

    List<Map<String, Object>> getAllDefinitions() throws IOException;

    QueryResult getResult(String queryName) throws IOException;

    /** Fetches the definition, deep-merges {@code updates}, and PUTs the result. */
    Map<String, Object> updateQuery(String queryName, Map<String, ?> updates) throws IOException;

    /** PUTs {@code fullDefinition} as-is. */
    Map<String, Object> updateQueryFull(String queryName, Map<String, ?> fullDefinition) throws IOException;

    void deleteQuery(String queryName) throws IOException;

    Map<String, Object> setQueryName(String queryName, String newQueryName) throws IOException;

    Map<String, Object> setRefreshRate(String queryName, int refreshRate) throws IOException;

    Map<String, Object> setDisplayName(String queryName, String displayName) throws IOException;
}
