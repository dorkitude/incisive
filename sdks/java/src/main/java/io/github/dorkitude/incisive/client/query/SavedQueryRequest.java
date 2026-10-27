package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.exceptions.IncisiveQueryClientException;

import java.net.URL;
import java.util.LinkedHashMap;
import java.util.Map;

/** {@code .../queries/saved[/{name}[/result]]} with an empty body. */
class SavedQueryRequest extends PersistentAnalysis {
    SavedQueryRequest(String httpMethod, boolean needsMasterKey, String queryName) {
        this(httpMethod, needsMasterKey, queryName, null);
    }

    SavedQueryRequest(String httpMethod, boolean needsMasterKey, String queryName, String displayName) {
        super(httpMethod, needsMasterKey, queryName, displayName);
    }

    @Override
    URL getRequestURL(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException {
        String name = getResourceName();
        String subPath = getAnalysisType() + (name != null ? "/" + name : "");
        if (retrievingResults()) {
            subPath += "/result";
        }
        return urlBuilder.getAnalysisUrl(projectId, subPath);
    }

    @Override
    String getAnalysisType() {
        return QueryConstants.SAVED;
    }

    @Override
    Map<String, Object> constructRequestArgs() {
        return new LinkedHashMap<>();
    }
}
