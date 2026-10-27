package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.IncisiveClient;
import io.github.dorkitude.incisive.client.exceptions.IncisiveQueryClientException;

import java.net.MalformedURLException;
import java.net.URL;
import java.net.URLEncoder;
import java.nio.charset.StandardCharsets;
import java.util.Locale;
import java.util.Map;

/** Builds query and dataset URLs: {@code {base}/{version}/projects/{id}/queries/...} and {@code .../datasets/...}. */
class RequestUrlBuilder {
    private final String apiVersion;
    private final String baseUrl;

    RequestUrlBuilder(String apiVersion, String baseUrl) {
        if (apiVersion == null || apiVersion.trim().isEmpty()) {
            throw new IllegalArgumentException("'apiVersion' is a required argument.");
        }
        if (baseUrl == null || baseUrl.trim().isEmpty()) {
            throw new IllegalArgumentException("'baseUrl' is a required argument.");
        }
        String b = baseUrl;
        while (b.endsWith("/")) {
            b = b.substring(0, b.length() - 1);
        }
        this.apiVersion = apiVersion;
        this.baseUrl = b;
    }

    URL getAnalysisUrl(String projectId, String analysisPath) throws IncisiveQueryClientException {
        try {
            return new URL(String.format(Locale.US, "%s/%s/projects/%s/queries/%s",
                    baseUrl, apiVersion, projectId, analysisPath));
        } catch (MalformedURLException ex) {
            throw new IncisiveQueryClientException("Failed to format query URL.", ex);
        }
    }

    URL getDatasetsUrl(String projectId, String datasetName, boolean fetchResults, Map<String, ?> queryParams)
            throws IncisiveQueryClientException {
        StringBuilder url = new StringBuilder(String.format(Locale.US, "%s/%s/projects/%s/%s",
                baseUrl, apiVersion, projectId, QueryConstants.DATASETS));
        if (datasetName != null) {
            url.append('/').append(IncisiveClient.encodePathSegment(datasetName));
            if (fetchResults) {
                url.append("/results");
            }
        }
        if (queryParams != null && !queryParams.isEmpty()) {
            StringBuilder query = new StringBuilder();
            for (Map.Entry<String, ?> entry : queryParams.entrySet()) {
                if (query.length() > 0) {
                    query.append('&');
                }
                query.append(URLEncoder.encode(entry.getKey(), StandardCharsets.UTF_8))
                        .append('=')
                        .append(URLEncoder.encode(String.valueOf(entry.getValue()), StandardCharsets.UTF_8));
            }
            url.append('?').append(query);
        }
        try {
            return new URL(url.toString());
        } catch (MalformedURLException ex) {
            throw new IncisiveQueryClientException("Failed to format dataset URL.", ex);
        }
    }
}
