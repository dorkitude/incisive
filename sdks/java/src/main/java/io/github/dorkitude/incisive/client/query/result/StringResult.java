package io.github.dorkitude.incisive.client.query.result;

import java.util.Objects;

/** A string result (e.g. an element of select_unique). */
public class StringResult extends QueryResult {
    private final String result;

    public StringResult(String result) {
        this.result = result;
    }

    @Override
    public boolean isString() {
        return true;
    }

    @Override
    public String stringValue() {
        return result;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof StringResult && Objects.equals(((StringResult) o).result, result);
    }

    @Override
    public int hashCode() {
        return Objects.hashCode(result);
    }

    @Override
    public String toString() {
        return "StringResult[" + result + "]";
    }
}
