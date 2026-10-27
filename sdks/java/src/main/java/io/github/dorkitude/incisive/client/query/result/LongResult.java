package io.github.dorkitude.incisive.client.query.result;

/** An integral result. */
public class LongResult extends QueryResult {
    private final long result;

    public LongResult(long result) {
        this.result = result;
    }

    @Override
    public boolean isLong() {
        return true;
    }

    @Override
    public long longValue() {
        return result;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof LongResult && ((LongResult) o).result == result;
    }

    @Override
    public int hashCode() {
        return Long.hashCode(result);
    }

    @Override
    public String toString() {
        return "LongResult[" + result + "]";
    }
}
