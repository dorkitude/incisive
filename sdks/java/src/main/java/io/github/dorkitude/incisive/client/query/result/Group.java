package io.github.dorkitude.incisive.client.query.result;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Set;

/** The group_by property values identifying one group of a {@link GroupByResult}. */
public class Group {
    private final Map<String, Object> properties;

    public Group(Map<String, ?> properties) {
        this.properties = Collections.unmodifiableMap(new LinkedHashMap<>(properties));
    }

    public Map<String, Object> getProperties() {
        return properties;
    }

    public Set<String> getPropertyNames() {
        return properties.keySet();
    }

    public Object getGroupValue(String propertyName) {
        if (!properties.containsKey(propertyName)) {
            throw new IllegalStateException("GroupBy does not contain expected property '" + propertyName + "'.");
        }
        return properties.get(propertyName);
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof Group && ((Group) o).properties.equals(properties);
    }

    @Override
    public int hashCode() {
        return properties.hashCode();
    }

    @Override
    public String toString() {
        return "Group" + properties;
    }
}
