package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.exceptions.IncisiveQueryClientException;

import java.net.URL;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * A simple analysis (count, sum, percentile, ...) sent as
 * {@code POST .../queries/{analysis_type}} with a JSON body. Build with {@link Builder}.
 */
public class Query extends IncisiveQueryRequest {
    private final QueryType queryType;
    private final String eventCollection;
    private final Timeframe timeframe;
    private final String targetProperty;
    private final RequestParameterCollection<Filter> filters;
    private final String interval;
    private final String timezone;
    private final List<String> groupBy;
    private final Integer maxAge;
    private final Double percentile;

    protected Query(Builder builder) {
        this.queryType = builder.queryType;
        this.eventCollection = builder.eventCollection;
        this.timeframe = builder.timeframe;
        this.targetProperty = builder.targetProperty;
        this.interval = builder.interval;
        this.timezone = builder.timezone;
        this.groupBy = builder.groupBy == null ? null : new ArrayList<>(builder.groupBy);
        this.maxAge = builder.maxAge;
        this.percentile = builder.percentile;
        this.filters = (builder.filters == null || builder.filters.isEmpty())
                ? null : new RequestParameterCollection<>(builder.filters);
    }

    @Override
    Map<String, Object> constructRequestArgs() {
        Map<String, Object> args = new LinkedHashMap<>();
        if (eventCollection != null) args.put(QueryConstants.EVENT_COLLECTION, eventCollection);
        if (targetProperty != null) args.put(QueryConstants.TARGET_PROPERTY, targetProperty);
        if (filters != null) args.put(QueryConstants.FILTERS, filters.constructParameterRequestArgs());
        if (interval != null) args.put(QueryConstants.INTERVAL, interval);
        if (timezone != null) args.put(QueryConstants.TIMEZONE, timezone);
        if (groupBy != null) args.put(QueryConstants.GROUP_BY, groupBy);
        if (maxAge != null) args.put(QueryConstants.MAX_AGE, maxAge);
        if (percentile != null) args.put(QueryConstants.PERCENTILE, percentile);
        if (timeframe != null) args.putAll(timeframe.constructTimeframeArgs());
        return args;
    }

    public QueryType getQueryType() {
        return queryType;
    }

    public boolean hasGroupBy() {
        return groupBy != null;
    }

    public boolean hasInterval() {
        return interval != null;
    }

    /** Checks the arguments required by the analysis type. */
    public boolean areParamsValid() {
        if (queryType == null || eventCollection == null || eventCollection.isEmpty()) {
            return false;
        }
        if (queryType != QueryType.COUNT && (targetProperty == null || targetProperty.isEmpty())) {
            return false;
        }
        return queryType != QueryType.PERCENTILE || percentile != null;
    }

    @Override
    URL getRequestURL(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException {
        return urlBuilder.getAnalysisUrl(projectId, getAnalysisType());
    }

    @Override
    String getAnalysisType() {
        return queryType.toString();
    }

    @Override
    boolean groupedResponseExpected() {
        return hasGroupBy();
    }

    @Override
    boolean intervalResponseExpected() {
        return hasInterval();
    }

    @Override
    Collection<String> getGroupByParams() {
        return groupBy;
    }

    /** Fluent builder; {@code with*} methods chain, {@code set*}/{@code add*} mutate. */
    public static class Builder {
        private final QueryType queryType;
        private String eventCollection;
        private Timeframe timeframe;
        private String targetProperty;
        private Double percentile;
        private List<Filter> filters;
        private String interval;
        private String timezone;
        private List<String> groupBy;
        private Integer maxAge;

        public Builder(QueryType queryType) {
            this.queryType = queryType;
        }

        /** Filters as wire-format maps. */
        public List<Map<String, Object>> getFilters() {
            List<Map<String, Object>> out = new ArrayList<>();
            if (filters != null) {
                for (Filter f : filters) {
                    out.add(f.constructParameterRequestArgs());
                }
            }
            return out;
        }

        /** Replaces filters from maps with property_name / operator (enum or wire string) / property_value. */
        public void setFilters(Collection<? extends Map<String, Object>> filterMaps) {
            this.filters = null;
            if (filterMaps == null) {
                return;
            }
            for (Map<String, Object> m : filterMaps) {
                Object rawOp = m.get(QueryConstants.OPERATOR);
                FilterOperator op;
                if (rawOp instanceof FilterOperator) {
                    op = (FilterOperator) rawOp;
                } else if (rawOp instanceof String) {
                    op = FilterOperator.fromString((String) rawOp);
                } else {
                    throw new IncisiveQueryClientException("Incorrect type for filter operator.");
                }
                addFilter((String) m.get(QueryConstants.PROPERTY_NAME), op, m.get(QueryConstants.PROPERTY_VALUE));
            }
        }

        public Builder withFilters(Collection<? extends Map<String, Object>> filterMaps) {
            setFilters(filterMaps);
            return this;
        }

        public Builder withFilter(String propertyName, FilterOperator operator, Object propertyValue) {
            addFilter(propertyName, operator, propertyValue);
            return this;
        }

        public Builder withFilter(Filter filter) {
            addFilter(filter);
            return this;
        }

        public void addFilter(String propertyName, FilterOperator operator, Object propertyValue) {
            addFilter(new Filter(propertyName, operator, propertyValue));
        }

        public void addFilter(Filter filter) {
            if (filters == null) {
                filters = new ArrayList<>();
            }
            filters.add(filter);
        }

        public String getEventCollection() {
            return eventCollection;
        }

        public void setEventCollection(String eventCollection) {
            this.eventCollection = eventCollection;
        }

        public Builder withEventCollection(String eventCollection) {
            setEventCollection(eventCollection);
            return this;
        }

        public String getTargetProperty() {
            return targetProperty;
        }

        public void setTargetProperty(String targetProperty) {
            this.targetProperty = targetProperty;
        }

        public Builder withTargetProperty(String targetProperty) {
            setTargetProperty(targetProperty);
            return this;
        }

        public String getInterval() {
            return interval;
        }

        public void setInterval(String interval) {
            this.interval = interval;
        }

        public Builder withInterval(String interval) {
            setInterval(interval);
            return this;
        }

        public String getTimezone() {
            return timezone;
        }

        public void setTimezone(String timezone) {
            this.timezone = timezone;
        }

        public Builder withTimezone(String timezone) {
            setTimezone(timezone);
            return this;
        }

        public List<String> getGroupBy() {
            return groupBy;
        }

        public void setGroupBy(List<String> groupBy) {
            this.groupBy = groupBy == null ? null : new ArrayList<>(groupBy);
        }

        /** Appends one group_by property (always sent as a JSON array). */
        public Builder withGroupBy(String groupBy) {
            if (this.groupBy == null) {
                this.groupBy = new ArrayList<>();
            }
            this.groupBy.add(groupBy);
            return this;
        }

        public Builder withGroupBy(List<String> groupBy) {
            setGroupBy(groupBy);
            return this;
        }

        public Integer getMaxAge() {
            return maxAge;
        }

        public void setMaxAge(Integer maxAge) {
            this.maxAge = maxAge;
        }

        public Builder withMaxAge(Integer maxAge) {
            setMaxAge(maxAge);
            return this;
        }

        public Double getPercentile() {
            return percentile;
        }

        public void setPercentile(Double percentile) {
            this.percentile = percentile;
        }

        public void setPercentile(Integer percentile) {
            this.percentile = percentile == null ? null : percentile.doubleValue();
        }

        public Builder withPercentile(Double percentile) {
            setPercentile(percentile);
            return this;
        }

        public Builder withPercentile(Integer percentile) {
            setPercentile(percentile);
            return this;
        }

        public Timeframe getTimeframe() {
            return timeframe;
        }

        public void setTimeframe(Timeframe timeframe) {
            this.timeframe = timeframe;
        }

        public Builder withTimeframe(Timeframe timeframe) {
            setTimeframe(timeframe);
            return this;
        }

        public Query build() {
            Query query = new Query(this);
            if (!query.areParamsValid()) {
                throw new IllegalArgumentException("Query parameters are insufficient. "
                        + "Please check the query API docs for required arguments.");
            }
            return query;
        }
    }
}
