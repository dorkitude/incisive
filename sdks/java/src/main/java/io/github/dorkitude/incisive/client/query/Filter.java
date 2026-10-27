package io.github.dorkitude.incisive.client.query;

import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;

/** A query filter: {@code {"property_name", "operator", "property_value"}}. */
public class Filter extends RequestParameter<Map<String, Object>> {
    private final String propertyName;
    private final FilterOperator operator;
    private final Object propertyValue;

    public Filter(String propertyName, FilterOperator operator, Object propertyValue) {
        if (propertyName == null || propertyName.trim().isEmpty()) {
            throw new IllegalArgumentException("Filter parameter 'propertyName' must be provided.");
        }
        if (operator == null) {
            throw new IllegalArgumentException("Filter parameter 'operator' must be provided.");
        }
        if (propertyValue == null) {
            throw new IllegalArgumentException("Filter parameter 'propertyValue' must be provided.");
        }
        this.propertyName = propertyName;
        this.operator = operator;
        this.propertyValue = propertyValue;
    }

    public String getPropertyName() {
        return propertyName;
    }

    public FilterOperator getOperator() {
        return operator;
    }

    public Object getPropertyValue() {
        return propertyValue;
    }

    public boolean isGeoFilter() {
        return operator == FilterOperator.WITHIN;
    }

    @Override
    Map<String, Object> constructParameterRequestArgs() {
        Map<String, Object> args = new LinkedHashMap<>();
        args.put(QueryConstants.PROPERTY_NAME, propertyName);
        args.put(QueryConstants.OPERATOR, operator.toString());
        args.put(QueryConstants.PROPERTY_VALUE, propertyValue);
        return args;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Filter)) return false;
        Filter f = (Filter) o;
        return propertyName.equals(f.propertyName) && operator == f.operator && propertyValue.equals(f.propertyValue);
    }

    @Override
    public int hashCode() {
        return Objects.hash(propertyName, operator, propertyValue);
    }

    @Override
    public String toString() {
        return "Filter[" + propertyName + " " + operator + " " + propertyValue + "]";
    }
}
