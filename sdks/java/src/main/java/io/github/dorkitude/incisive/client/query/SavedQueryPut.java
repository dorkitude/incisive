package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.http.HttpMethods;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * {@code PUT .../queries/saved/{name}} (master key) with body
 * {@code {"query": {...args, "analysis_type"}, "refresh_rate", "metadata": {"display_name"}, ...misc}}.
 */
class SavedQueryPut extends SavedQueryRequest {
    private final IncisiveQueryRequest query;
    private final int refreshRate;
    private final Map<String, Object> miscProperties;

    SavedQueryPut(String queryName, String displayName, IncisiveQueryRequest query, int refreshRate,
                  Map<String, ?> miscProperties) {
        super(HttpMethods.PUT, true, queryName, displayName);
        if (query == null && (miscProperties == null || miscProperties.isEmpty())) {
            throw new IllegalArgumentException("If a query definition is not provided, then miscProperties "
                    + "should define the saved/cached query.");
        }
        if (refreshRate > 0) {
            RefreshRate.validateRefreshRate(refreshRate);
        }
        this.query = query;
        this.refreshRate = refreshRate;
        this.miscProperties = miscProperties == null ? new LinkedHashMap<>() : new LinkedHashMap<>(miscProperties);
    }

    /** Full-definition PUT: the body is exactly {@code miscProperties}. */
    SavedQueryPut(String queryName, Map<String, ?> miscProperties) {
        this(queryName, null, null, -1, miscProperties);
    }

    @Override
    Map<String, Object> constructRequestArgs() {
        Map<String, Object> args = super.constructRequestArgs();
        if (query != null) {
            Map<String, Object> queryArgs = query.constructRequestArgs();
            queryArgs.put(QueryConstants.ANALYSIS_TYPE, query.getAnalysisType());
            args.put(QueryConstants.QUERY, queryArgs);
        }
        if (refreshRate >= 0) {
            args.put(QueryConstants.REFRESH_RATE, refreshRate);
        }
        if (getDisplayName() != null) {
            Map<String, Object> metadata = new LinkedHashMap<>();
            metadata.put(QueryConstants.DISPLAY_NAME, getDisplayName());
            args.put(QueryConstants.METADATA, metadata);
        }
        for (String key : miscProperties.keySet()) {
            if (args.containsKey(key)) {
                throw new IllegalStateException("The key '" + key + "' already exists.");
            }
        }
        args.putAll(miscProperties);
        return args;
    }
}
