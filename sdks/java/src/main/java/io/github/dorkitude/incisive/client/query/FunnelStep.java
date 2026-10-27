package io.github.dorkitude.incisive.client.query;

import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.Map;

/** One funnel step: {@code {event_collection, actor_property, timeframe?, filters?, inverted?, optional?, with_actors?}}. */
public class FunnelStep extends RequestParameter<Map<String, Object>> {
    private final String collectionName;
    private final String actorPropertyName;
    private final Timeframe timeframe;
    private final RequestParameterCollection<Filter> filters;
    private final Boolean inverted;
    private final Boolean optional;
    private final Boolean withActors;

    public FunnelStep(String collectionName, String actorPropertyName) {
        this(collectionName, actorPropertyName, null);
    }

    public FunnelStep(String collectionName, String actorPropertyName, Timeframe timeframe) {
        this(collectionName, actorPropertyName, timeframe, null, null, null, null);
    }

    public FunnelStep(String collectionName, String actorPropertyName, Timeframe timeframe,
                      Collection<Filter> filters, Boolean inverted, Boolean optional, Boolean withActors) {
        if (collectionName == null || collectionName.trim().isEmpty()) {
            throw new IllegalArgumentException("FunnelStep parameter collectionName must be provided.");
        }
        if (actorPropertyName == null || actorPropertyName.trim().isEmpty()) {
            throw new IllegalArgumentException("FunnelStep parameter actorPropertyName must be provided.");
        }
        this.collectionName = collectionName;
        this.actorPropertyName = actorPropertyName;
        this.timeframe = timeframe;
        this.filters = (filters == null || filters.isEmpty()) ? null : new RequestParameterCollection<>(filters);
        this.inverted = inverted;
        this.optional = optional;
        this.withActors = withActors;
    }

    @Override
    Map<String, Object> constructParameterRequestArgs() {
        Map<String, Object> args = new LinkedHashMap<>();
        args.put(QueryConstants.EVENT_COLLECTION, collectionName);
        args.put(QueryConstants.ACTOR_PROPERTY, actorPropertyName);
        if (timeframe != null) args.putAll(timeframe.constructTimeframeArgs());
        if (filters != null) args.put(QueryConstants.FILTERS, filters.constructParameterRequestArgs());
        if (inverted != null) args.put(QueryConstants.INVERTED, inverted);
        if (optional != null) args.put(QueryConstants.OPTIONAL, optional);
        if (withActors != null) args.put(QueryConstants.WITH_ACTORS, withActors);
        return args;
    }

    public Timeframe getTimeframe() {
        return timeframe;
    }

    public Boolean getInverted() {
        return inverted;
    }

    public Boolean getOptional() {
        return optional;
    }

    public Boolean getWithActors() {
        return withActors;
    }
}
