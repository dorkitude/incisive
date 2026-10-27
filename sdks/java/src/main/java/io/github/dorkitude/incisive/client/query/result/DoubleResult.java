package io.github.dorkitude.incisive.client.query.result;

/** A floating point result. */
public class DoubleResult extends QueryResult {
    private final double result;

    public DoubleResult(double result) {
        this.result = result;
    }

    @Override
    public boolean isDouble() {
        return true;
    }

    @Override
    public double doubleValue() {
        return result;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof DoubleResult && Double.compare(((DoubleResult) o).result, result) == 0;
    }

    @Override
    public int hashCode() {
        return Double.hashCode(result);
    }

    @Override
    public String toString() {
        return "DoubleResult[" + result + "]";
    }
}
