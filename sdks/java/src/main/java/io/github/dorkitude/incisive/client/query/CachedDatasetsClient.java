package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.query.result.IntervalResultValue;

import java.io.IOException;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;

class CachedDatasetsClient implements CachedDatasets {
    private final IncisiveQueryClient queryClient;

    CachedDatasetsClient(IncisiveQueryClient queryClient) {
        this.queryClient = queryClient;
    }

    private static void notBlank(String s, String message) {
        if (s == null) {
            throw new NullPointerException(message);
        }
        if (s.trim().isEmpty()) {
            throw new IllegalArgumentException(message);
        }
    }

    @Override
    public DatasetDefinition create(String datasetName, String displayName, DatasetQuery query,
                                    Collection<String> indexBy) throws IOException {
        notBlank(datasetName, "Dataset name cannot be blank");
        notBlank(displayName, "Display name cannot be blank");
        if (query == null) {
            throw new NullPointerException("Dataset query is required");
        }
        if (indexBy == null || indexBy.isEmpty()) {
            throw new IllegalArgumentException("At least one index property is required");
        }
        return DatasetDefinition.fromMap(queryClient.getMapResponse(
                CachedDatasetRequest.creationRequest(datasetName, displayName, query, indexBy)));
    }

    @Override
    public DatasetDefinition getDefinition(String datasetName) throws IOException {
        notBlank(datasetName, "Dataset name cannot be blank");
        return DatasetDefinition.fromMap(queryClient.getMapResponse(CachedDatasetRequest.definitionRequest(datasetName)));
    }

    @Override
    public List<IntervalResultValue> getResults(DatasetDefinition datasetDefinition, Map<String, ?> indexByValues,
                                                Timeframe timeframe) throws IOException {
        if (datasetDefinition == null) {
            throw new NullPointerException("Dataset definition is required");
        }
        if (timeframe == null) {
            throw new NullPointerException("Timeframe is required");
        }
        Set<String> expected = new TreeSet<>(datasetDefinition.getIndexBy());
        if (indexByValues == null) {
            throw new NullPointerException("Values for all index_by properties are required: " + expected);
        }
        if (indexByValues.isEmpty()) {
            throw new IllegalArgumentException("Values for all index_by properties are required: " + expected);
        }
        Set<String> given = new TreeSet<>(indexByValues.keySet());
        if (!expected.equals(given)) {
            throw new IllegalArgumentException("Values for the following index_by properties must be present: "
                    + expected + ". Found for: " + given);
        }
        return queryClient.execute(CachedDatasetRequest.resultsRequest(datasetDefinition, indexByValues, timeframe))
                .getIntervalResults();
    }

    @Override
    public List<DatasetDefinition> getDefinitions() throws IOException {
        return getDefinitions(null, null);
    }

    @Override
    public List<DatasetDefinition> getDefinitions(Integer limit, String afterName) throws IOException {
        return DatasetDefinition.definitionsFromMap(queryClient.getMapResponse(
                CachedDatasetRequest.definitionsByProjectRequest(limit, afterName)));
    }

    @Override
    public boolean delete(String datasetName) throws IOException {
        notBlank(datasetName, "Dataset name cannot be blank");
        return queryClient.getMapResponse(CachedDatasetRequest.deletionRequest(datasetName)).isEmpty();
    }
}
