package io.github.dorkitude.incisive.client.query;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/** A cached dataset definition as returned by the server. */
public class DatasetDefinition {
    private String datasetName;
    private String displayName;
    private String projectId;
    private String organizationId;
    private String lastScheduledDate;
    private String latestSubtimeframeAvailable;
    private String millisecondsBehind;
    private List<String> indexBy;
    private DatasetQuery query;

    @SuppressWarnings("unchecked")
    static DatasetDefinition fromMap(Map<String, ?> p) {
        DatasetDefinition d = new DatasetDefinition();
        d.datasetName = str(p.get("dataset_name"));
        d.displayName = str(p.get("display_name"));
        d.projectId = str(p.get("project_id"));
        d.organizationId = str(p.get("organization_id"));
        d.lastScheduledDate = str(p.get("last_scheduled_date"));
        d.latestSubtimeframeAvailable = str(p.get("latest_subtimeframe_available"));
        d.millisecondsBehind = String.valueOf(p.get("milliseconds_behind"));
        Object idx = p.get("index_by");
        if (idx instanceof List) {
            d.indexBy = new ArrayList<>();
            for (Object o : (List<Object>) idx) {
                d.indexBy.add(String.valueOf(o));
            }
        } else if (idx instanceof String) {
            d.indexBy = Collections.singletonList((String) idx);
        }
        Object q = p.get("query");
        d.query = q instanceof Map ? DatasetQuery.fromMap((Map<String, ?>) q) : null;
        return d;
    }

    @SuppressWarnings("unchecked")
    static List<DatasetDefinition> definitionsFromMap(Map<String, ?> p) {
        Object datasets = p.get("datasets");
        if (!(datasets instanceof List)) {
            return Collections.emptyList();
        }
        List<DatasetDefinition> out = new ArrayList<>();
        for (Object d : (List<Object>) datasets) {
            out.add(fromMap((Map<String, ?>) d));
        }
        return out;
    }

    private static String str(Object o) {
        return o == null ? null : o.toString();
    }

    public String getDatasetName() {
        return datasetName;
    }

    public String getDisplayName() {
        return displayName;
    }

    public String getProjectId() {
        return projectId;
    }

    public String getOrganizationId() {
        return organizationId;
    }

    public String getLastScheduledDate() {
        return lastScheduledDate;
    }

    public String getLatestSubtimeframeAvailable() {
        return latestSubtimeframeAvailable;
    }

    public String getMillisecondsBehind() {
        return millisecondsBehind;
    }

    public List<String> getIndexBy() {
        return indexBy;
    }

    public DatasetQuery getQuery() {
        return query;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof DatasetDefinition)) return false;
        DatasetDefinition d = (DatasetDefinition) o;
        return Objects.equals(datasetName, d.datasetName) && Objects.equals(displayName, d.displayName)
                && Objects.equals(projectId, d.projectId) && Objects.equals(organizationId, d.organizationId)
                && Objects.equals(lastScheduledDate, d.lastScheduledDate)
                && Objects.equals(latestSubtimeframeAvailable, d.latestSubtimeframeAvailable)
                && Objects.equals(millisecondsBehind, d.millisecondsBehind) && Objects.equals(indexBy, d.indexBy)
                && Objects.equals(query, d.query);
    }

    @Override
    public int hashCode() {
        return Objects.hash(datasetName, displayName, projectId, organizationId, lastScheduledDate,
                latestSubtimeframeAvailable, millisecondsBehind, indexBy, query);
    }

    @Override
    public String toString() {
        return "DatasetDefinition[" + datasetName + ", index_by=" + indexBy + ", query=" + query + "]";
    }

    /** Builder, mainly for constructing definitions to pass to {@link CachedDatasets#getResults}. */
    public static final class DatasetDefinitionBuilder {
        private final DatasetDefinition d = new DatasetDefinition();

        private DatasetDefinitionBuilder() {}

        public static DatasetDefinitionBuilder aDatasetDefinition() {
            return new DatasetDefinitionBuilder();
        }

        public DatasetDefinitionBuilder withDatasetName(String v) {
            d.datasetName = v;
            return this;
        }

        public DatasetDefinitionBuilder withDisplayName(String v) {
            d.displayName = v;
            return this;
        }

        public DatasetDefinitionBuilder withProjectId(String v) {
            d.projectId = v;
            return this;
        }

        public DatasetDefinitionBuilder withOrganizationId(String v) {
            d.organizationId = v;
            return this;
        }

        public DatasetDefinitionBuilder withLastScheduledDate(String v) {
            d.lastScheduledDate = v;
            return this;
        }

        public DatasetDefinitionBuilder withLatestSubtimeframeAvailable(String v) {
            d.latestSubtimeframeAvailable = v;
            return this;
        }

        public DatasetDefinitionBuilder withMillisecondsBehind(String v) {
            d.millisecondsBehind = v;
            return this;
        }

        public DatasetDefinitionBuilder withIndexBy(List<String> v) {
            d.indexBy = v;
            return this;
        }

        public DatasetDefinitionBuilder withQuery(DatasetQuery v) {
            d.query = v;
            return this;
        }

        public DatasetDefinition build() {
            DatasetDefinition out = new DatasetDefinition();
            out.datasetName = d.datasetName;
            out.displayName = d.displayName;
            out.projectId = d.projectId;
            out.organizationId = d.organizationId;
            out.lastScheduledDate = d.lastScheduledDate;
            out.latestSubtimeframeAvailable = d.latestSubtimeframeAvailable;
            out.millisecondsBehind = d.millisecondsBehind;
            out.indexBy = d.indexBy;
            out.query = d.query;
            return out;
        }
    }
}
