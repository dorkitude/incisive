package io.github.dorkitude.incisive.client.query;

import java.util.ArrayList;
import java.util.Collection;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;

/** An ordered, non-empty list of parameters serialized as a JSON array. */
class RequestParameterCollection<T extends RequestParameter<?>> extends RequestParameter<List<Object>>
        implements Iterable<T> {
    private final List<T> parameters;

    RequestParameterCollection(Collection<? extends T> parameters) {
        if (parameters == null || parameters.isEmpty()) {
            throw new IllegalArgumentException("'parameters' is a required parameter and must not be empty.");
        }
        this.parameters = new ArrayList<>(parameters);
    }

    @Override
    List<Object> constructParameterRequestArgs() {
        List<Object> out = new ArrayList<>(parameters.size());
        for (T p : parameters) {
            out.add(p.constructParameterRequestArgs());
        }
        return out;
    }

    @Override
    public Iterator<T> iterator() {
        return Collections.unmodifiableList(parameters).iterator();
    }
}
