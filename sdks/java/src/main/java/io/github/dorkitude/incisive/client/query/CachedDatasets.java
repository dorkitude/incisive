package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.query.result.IntervalResultValue;

import java.io.IOException;
import java.util.Collection;
import java.util.List;
import java.util.Map;

/**
 * Cached dataset management. Obtain via {@link IncisiveQueryClient#getCachedDatasetsClient()}.
 * Create/delete use the master key; reads use the read key.
 */
public interface CachedDatasets {
    DatasetDefinition create(String datasetName, String displayName, DatasetQuery query, Collection<String> indexBy)
            throws IOException;

    DatasetDefinition getDefinition(String datasetName) throws IOException;

    List<IntervalResultValue> getResults(DatasetDefinition datasetDefinition, Map<String, ?> indexByValues,
                                         Timeframe timeframe) throws IOException;

    List<DatasetDefinition> getDefinitions() throws IOException;

    List<DatasetDefinition> getDefinitions(Integer limit, String afterName) throws IOException;

    /** True when the server answered with an empty body (204). */
    boolean delete(String datasetName) throws IOException;
}
