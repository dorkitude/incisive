package io.github.dorkitude.incisive.client.query;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/** The query inside a cached dataset definition. The timeframe is a relative timeframe string. */
public class DatasetQuery {
    private String projectId;
    private String analysisType;
    private List<SubAnalysis> analyses;
    private String targetProperty;
    private String eventCollection;
    private String timezone;
    private String interval;
    private String timeframe;
    private List<String> groupBy;
    private List<Filter> filters;

    @SuppressWarnings("unchecked")
    static DatasetQuery fromMap(Map<String, ?> p) {
        DatasetQuery q = new DatasetQuery();
        q.projectId = str(p.get("project_id"));
        q.analysisType = str(p.get("analysis_type"));
        q.targetProperty = str(p.get("target_property"));
        q.eventCollection = str(p.get("event_collection"));
        q.timezone = str(p.get("timezone"));
        q.interval = str(p.get("interval"));
        q.timeframe = str(p.get("timeframe"));
        Object g = p.get("group_by");
        if (g instanceof List) {
            q.groupBy = new ArrayList<>();
            for (Object o : (List<Object>) g) {
                q.groupBy.add(String.valueOf(o));
            }
        } else if (g instanceof String) {
            q.groupBy = new ArrayList<>(List.of((String) g));
        }
        Object a = p.get("analyses");
        if (a instanceof Map) {
            q.analyses = new ArrayList<>();
            for (Map.Entry<String, Object> e : ((Map<String, Object>) a).entrySet()) {
                Map<String, Object> def = (Map<String, Object>) e.getValue();
                QueryType type = QueryType.valueOfIgnoreCase(String.valueOf(def.get("analysis_type")));
                String target = str(def.get("target_property"));
                Object pct = def.get("percentile");
                if (pct != null) {
                    double v = pct instanceof Number ? ((Number) pct).doubleValue() : Double.parseDouble(pct.toString());
                    q.analyses.add(new SubAnalysis(e.getKey(), type, target, Percentile.createCoerced(v)));
                } else {
                    q.analyses.add(new SubAnalysis(e.getKey(), type, type == QueryType.COUNT ? null : target));
                }
            }
        }
        Object f = p.get("filters");
        if (f instanceof List) {
            q.filters = new ArrayList<>();
            for (Object o : (List<Object>) f) {
                Map<String, Object> m = (Map<String, Object>) o;
                q.filters.add(new Filter(str(m.get("property_name")),
                        FilterOperator.fromString(str(m.get("operator"))), m.get("property_value")));
            }
        }
        return q;
    }

    private static String str(Object o) {
        return o == null ? null : o.toString();
    }

    Map<String, Object> asMap() {
        Map<String, Object> m = new LinkedHashMap<>();
        if (analysisType != null) m.put(QueryConstants.ANALYSIS_TYPE, analysisType);
        if (targetProperty != null) m.put(QueryConstants.TARGET_PROPERTY, targetProperty);
        if (eventCollection != null) m.put(QueryConstants.EVENT_COLLECTION, eventCollection);
        if (timeframe != null) m.put(QueryConstants.TIMEFRAME, timeframe);
        if (timezone != null) m.put(QueryConstants.TIMEZONE, timezone);
        if (interval != null) m.put(QueryConstants.INTERVAL, interval);
        if (groupBy != null) m.put(QueryConstants.GROUP_BY, groupBy);
        if (analyses != null) {
            Map<String, Object> am = new LinkedHashMap<>();
            for (SubAnalysis s : analyses) {
                am.put(s.getLabel(), s.constructParameterRequestArgs());
            }
            m.put(QueryConstants.ANALYSES, am);
        }
        if (filters != null) {
            List<Object> fl = new ArrayList<>();
            for (Filter f : filters) {
                fl.add(f.constructParameterRequestArgs());
            }
            m.put(QueryConstants.FILTERS, fl);
        }
        return m;
    }

    public String getProjectId() {
        return projectId;
    }

    public String getAnalysisType() {
        return analysisType;
    }

    public List<SubAnalysis> getAnalyses() {
        return analyses;
    }

    public String getTargetProperty() {
        return targetProperty;
    }

    public String getEventCollection() {
        return eventCollection;
    }

    public String getTimezone() {
        return timezone;
    }

    public String getInterval() {
        return interval;
    }

    public String getTimeframe() {
        return timeframe;
    }

    public List<String> getGroupBy() {
        return groupBy;
    }

    public List<Filter> getFilters() {
        return filters;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof DatasetQuery)) return false;
        DatasetQuery q = (DatasetQuery) o;
        return Objects.equals(asMap(), q.asMap()) && Objects.equals(projectId, q.projectId);
    }

    @Override
    public int hashCode() {
        return Objects.hash(asMap(), projectId);
    }

    @Override
    public String toString() {
        return "DatasetQuery" + asMap();
    }

    public static final class DatasetQueryBuilder {
        private final DatasetQuery q = new DatasetQuery();

        private DatasetQueryBuilder() {}

        public static DatasetQueryBuilder aDatasetQuery() {
            return new DatasetQueryBuilder();
        }

        public DatasetQueryBuilder withProjectId(String v) {
            q.projectId = v;
            return this;
        }

        public DatasetQueryBuilder withAnalysisType(String v) {
            q.analysisType = v;
            return this;
        }

        public DatasetQueryBuilder withAnalyses(List<SubAnalysis> v) {
            q.analyses = v;
            return this;
        }

        public DatasetQueryBuilder withTargetProperty(String v) {
            q.targetProperty = v;
            return this;
        }

        public DatasetQueryBuilder withEventCollection(String v) {
            q.eventCollection = v;
            return this;
        }

        public DatasetQueryBuilder withTimezone(String v) {
            q.timezone = v;
            return this;
        }

        public DatasetQueryBuilder withInterval(String v) {
            q.interval = v;
            return this;
        }

        public DatasetQueryBuilder withTimeframe(String v) {
            q.timeframe = v;
            return this;
        }

        public DatasetQueryBuilder withGroupBy(List<String> v) {
            q.groupBy = v;
            return this;
        }

        public DatasetQueryBuilder withFilters(List<Filter> v) {
            q.filters = v;
            return this;
        }

        public DatasetQuery build() {
            DatasetQuery out = new DatasetQuery();
            out.projectId = q.projectId;
            out.analysisType = q.analysisType;
            out.analyses = q.analyses;
            out.targetProperty = q.targetProperty;
            out.eventCollection = q.eventCollection;
            out.timezone = q.timezone;
            out.interval = q.interval;
            out.timeframe = q.timeframe;
            out.groupBy = q.groupBy;
            out.filters = q.filters;
            return out;
        }
    }
}
