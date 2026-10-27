package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.JacksonJsonHandler;
import io.github.dorkitude.incisive.client.exceptions.IncisiveQueryClientException;
import io.github.dorkitude.incisive.client.http.HttpMethods;

import java.io.IOException;
import java.net.URL;
import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

/** Requests against {@code .../datasets[/{name}[/results]]}. */
abstract class CachedDatasetRequest extends PersistentAnalysis {
    private static final JacksonJsonHandler JSON = new JacksonJsonHandler();

    private CachedDatasetRequest(String httpMethod, boolean needsMasterKey, String datasetName) {
        super(httpMethod, needsMasterKey, datasetName, null);
    }

    URL plainUrl(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException {
        return urlBuilder.getDatasetsUrl(projectId, getResourceName(), false, Collections.emptyMap());
    }

    static IncisiveQueryRequest definitionRequest(String datasetName) {
        return new CachedDatasetRequest(HttpMethods.GET, false, datasetName) {
            @Override
            URL getRequestURL(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException {
                return plainUrl(urlBuilder, projectId);
            }
        };
    }

    /**
     * {@code GET .../datasets/{name}/results?index_by=<json object>&timeframe=<json value>} (read key).
     * A relative timeframe is sent as a JSON string, i.e. with surrounding quotes.
     */
    static IncisiveQueryRequest resultsRequest(final DatasetDefinition definition, final Map<String, ?> indexByValues,
                                               final Timeframe timeframe) {
        return new CachedDatasetRequest(HttpMethods.GET, false, definition.getDatasetName()) {
            @Override
            URL getRequestURL(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException {
                Map<String, Object> params = new LinkedHashMap<>();
                try {
                    params.put(QueryConstants.INDEX_BY, JSON.writeValueAsString(indexByValues));
                    params.put(QueryConstants.TIMEFRAME,
                            JSON.writeValueAsString(timeframe.constructTimeframeArgs().get(QueryConstants.TIMEFRAME)));
                } catch (IOException e) {
                    throw new IncisiveQueryClientException(e);
                }
                return urlBuilder.getDatasetsUrl(projectId, definition.getDatasetName(), true, params);
            }

            @Override
            boolean retrievingResults() {
                return true;
            }

            @Override
            boolean groupedResponseExpected() {
                Collection<String> g = getGroupByParams();
                return g != null && !g.isEmpty();
            }

            @Override
            String getAnalysisType() {
                return definition.getQuery() == null ? null : definition.getQuery().getAnalysisType();
            }

            @Override
            Collection<String> getGroupByParams() {
                return definition.getQuery() == null || definition.getQuery().getGroupBy() == null
                        ? new ArrayList<>() : definition.getQuery().getGroupBy();
            }
        };
    }

    /** {@code GET .../datasets[?limit=N&after_name=X]} (read key). */
    static IncisiveQueryRequest definitionsByProjectRequest(final Integer limit, final String afterName) {
        return new CachedDatasetRequest(HttpMethods.GET, false, null) {
            @Override
            URL getRequestURL(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException {
                Map<String, Object> params = new LinkedHashMap<>();
                if (limit != null) params.put(QueryConstants.LIMIT, limit);
                if (afterName != null) params.put(QueryConstants.AFTER_NAME, afterName);
                return urlBuilder.getDatasetsUrl(projectId, null, false, params);
            }
        };
    }

    /** {@code DELETE .../datasets/{name}} (master key). */
    static IncisiveQueryRequest deletionRequest(String datasetName) {
        return new CachedDatasetRequest(HttpMethods.DELETE, true, datasetName) {
            @Override
            URL getRequestURL(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException {
                return plainUrl(urlBuilder, projectId);
            }
        };
    }

    /** {@code PUT .../datasets/{name}} with {@code {display_name, index_by, query}} (master key). */
    static IncisiveQueryRequest creationRequest(final String datasetName, final String displayName,
                                                final DatasetQuery query, final Collection<String> indexBy) {
        return new CachedDatasetRequest(HttpMethods.PUT, true, datasetName) {
            @Override
            URL getRequestURL(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException {
                return plainUrl(urlBuilder, projectId);
            }

            @Override
            Map<String, Object> constructRequestArgs() {
                Map<String, Object> args = new LinkedHashMap<>();
                args.put(QueryConstants.DISPLAY_NAME, displayName);
                args.put(QueryConstants.INDEX_BY, new ArrayList<>(indexBy));
                args.put(QueryConstants.QUERY, query.asMap());
                return args;
            }
        };
    }

    @Override
    String getAnalysisType() {
        return QueryConstants.DATASETS;
    }

    @Override
    Map<String, Object> constructRequestArgs() {
        return Collections.emptyMap();
    }

    @Override
    boolean intervalResponseExpected() {
        return true;
    }
}
