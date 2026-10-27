package io.github.dorkitude.incisive.client.query;

import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;

/**
 * Common base of {@link SingleAnalysis} and {@link MultiAnalysis}: an analysis over one event
 * collection with a required timeframe, optional filters, interval and group_by.
 */
public abstract class CollectionAnalysis extends IncisiveQueryRequest {
    private final String collectionName;
    private final Timeframe timeframe;
    private final RequestParameterCollection<Filter> filters;
    private final String interval;
    private final List<String> groupBy;

    protected CollectionAnalysis(Builder<?> builder) {
        this.collectionName = builder.collectionName;
        this.timeframe = builder.timeframe;
        this.interval = builder.interval;
        this.groupBy = (builder.groupBy == null || builder.groupBy.isEmpty()) ? null : new ArrayList<>(builder.groupBy);
        this.filters = (builder.filters == null || builder.filters.isEmpty())
                ? null : new RequestParameterCollection<>(builder.filters);
    }

    public boolean hasGroupBy() {
        return groupBy != null;
    }

    public boolean hasInterval() {
        return interval != null;
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

    @Override
    Map<String, Object> constructRequestArgs() {
        Map<String, Object> args = new LinkedHashMap<>();
        args.put(QueryConstants.EVENT_COLLECTION, collectionName);
        args.putAll(timeframe.constructTimeframeArgs());
        if (filters != null) args.put(QueryConstants.FILTERS, filters.constructParameterRequestArgs());
        if (interval != null) args.put(QueryConstants.INTERVAL, interval);
        if (groupBy != null) args.put(QueryConstants.GROUP_BY, groupBy);
        return args;
    }

    protected void validateParams() {
        if (collectionName == null || collectionName.trim().isEmpty()) {
            throw new IllegalArgumentException("There must be an event collection name set to perform a CollectionAnalysis.");
        }
        if (timeframe == null) {
            throw new IllegalArgumentException("A 'timeframe' parameter is required for a CollectionAnalysis");
        }
        if (groupBy != null && filters != null) {
            for (Filter f : filters) {
                if (f.isGeoFilter()) {
                    throw new IllegalArgumentException("The 'group_by' parameter cannot be used in conjunction with geo-filtering.");
                }
            }
        }
    }

    /** Self-typed builder base. */
    protected abstract static class Builder<ChainT extends Builder<ChainT>> {
        private String collectionName;
        private Timeframe timeframe;
        private List<Filter> filters;
        private String interval;
        private LinkedHashSet<String> groupBy;

        public String getCollectionName() {
            return collectionName;
        }

        public void setCollectionName(String collectionName) {
            if (collectionName == null || collectionName.trim().isEmpty()) {
                throw new IllegalArgumentException("The 'collectionName' parameter is null or empty.");
            }
            this.collectionName = collectionName;
        }

        public ChainT withEventCollection(String collectionName) {
            if (this.collectionName != null) {
                throw new IllegalArgumentException("Attempting to set the 'collectionName' parameter multiple times.");
            }
            setCollectionName(collectionName);
            return getThis();
        }

        public Timeframe getTimeframe() {
            return timeframe;
        }

        public void setTimeframe(Timeframe timeframe) {
            this.timeframe = timeframe;
        }

        public ChainT withTimeframe(Timeframe timeframe) {
            if (this.timeframe != null) {
                throw new IllegalArgumentException("Attempting to set the 'timeframe' parameter multiple times.");
            }
            setTimeframe(timeframe);
            return getThis();
        }

        public Collection<Filter> getFilters() {
            return filters;
        }

        public void setFilters(Collection<? extends Filter> filters) {
            this.filters = null;
            if (filters != null) {
                withFilters(filters);
            }
        }

        public ChainT withFilters(Collection<? extends Filter> filters) {
            if (filters == null) {
                throw new IllegalArgumentException("The 'filters' parameter cannot be null.");
            }
            for (Filter f : filters) {
                addFilter(f);
            }
            return getThis();
        }

        public ChainT withFilter(String propertyName, FilterOperator operator, Object propertyValue) {
            addFilter(propertyName, operator, propertyValue);
            return getThis();
        }

        public ChainT withFilter(Filter filter) {
            addFilter(filter);
            return getThis();
        }

        public void addFilter(String propertyName, FilterOperator operator, Object propertyValue) {
            addFilter(new Filter(propertyName, operator, propertyValue));
        }

        public void addFilter(Filter filter) {
            if (filter == null) {
                throw new IllegalArgumentException("The 'filter' parameter cannot be null.");
            }
            if (filters == null) {
                filters = new ArrayList<>();
            }
            filters.add(filter);
        }

        public String getInterval() {
            return interval;
        }

        public void setInterval(String interval) {
            if (interval == null || interval.trim().isEmpty()) {
                throw new IllegalArgumentException("The 'interval' parameter is null or empty.");
            }
            this.interval = interval;
        }

        public ChainT withInterval(String interval) {
            if (this.interval != null) {
                throw new IllegalArgumentException("Attempting to set the 'interval' parameter multiple times.");
            }
            setInterval(interval);
            return getThis();
        }

        public Collection<String> getGroupBy() {
            return groupBy;
        }

        public void setGroupBy(Collection<String> groupBy) {
            this.groupBy = null;
            if (groupBy != null) {
                withGroupBy(groupBy);
            }
        }

        public ChainT withGroupBy(Collection<String> groupBy) {
            if (groupBy == null) {
                throw new IllegalArgumentException("The 'groupBy' parameter cannot be null.");
            }
            for (String g : groupBy) {
                addGroupBy(g);
            }
            return getThis();
        }

        public ChainT withGroupBy(String groupBy) {
            addGroupBy(groupBy);
            return getThis();
        }

        public void addGroupBy(String groupBy) {
            if (groupBy == null || groupBy.trim().isEmpty()) {
                throw new IllegalArgumentException("The 'groupBy' parameter is null or empty.");
            }
            if (this.groupBy == null) {
                this.groupBy = new LinkedHashSet<>();
            }
            this.groupBy.add(groupBy);
        }

        protected abstract ChainT getThis();
    }
}
