package io.github.dorkitude.incisive.client.query;

/** Wire-format names used by the query API. */
final class QueryConstants {
    private QueryConstants() {}

    static final String FUNNEL = "funnel";
    static final String MULTI_ANALYSIS = "multi_analysis";
    static final String SAVED = "saved";
    static final String DATASETS = "datasets";

    static final String EVENT_COLLECTION = "event_collection";
    static final String TARGET_PROPERTY = "target_property";
    static final String FILTERS = "filters";
    static final String TIMEFRAME = "timeframe";
    static final String INTERVAL = "interval";
    static final String TIMEZONE = "timezone";
    static final String GROUP_BY = "group_by";
    static final String MAX_AGE = "max_age";
    static final String PERCENTILE = "percentile";
    static final String INDEX_BY = "index_by";
    static final String LIMIT = "limit";
    static final String AFTER_NAME = "after_name";

    static final String PROPERTY_NAME = "property_name";
    static final String OPERATOR = "operator";
    static final String PROPERTY_VALUE = "property_value";

    static final String ACTOR_PROPERTY = "actor_property";
    static final String STEPS = "steps";
    static final String INVERTED = "inverted";
    static final String OPTIONAL = "optional";
    static final String WITH_ACTORS = "with_actors";
    static final String ACTORS = "actors";

    static final String ANALYSES = "analyses";
    static final String ANALYSIS_TYPE = "analysis_type";
    static final String RESULT = "result";
    static final String ERROR_CODE = "error_code";
    static final String MESSAGE = "message";
    static final String VALUE = "value";
    static final String START = "start";
    static final String END = "end";

    static final String QUERY = "query";
    static final String REFRESH_RATE = "refresh_rate";
    static final String METADATA = "metadata";
    static final String DISPLAY_NAME = "display_name";
    static final String QUERY_NAME = "query_name";
}
