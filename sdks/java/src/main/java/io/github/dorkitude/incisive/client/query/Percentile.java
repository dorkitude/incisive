package io.github.dorkitude.incisive.client.query;

import java.math.BigDecimal;

/** A percentile in (0, 100] with at most two decimal places. */
public final class Percentile {
    private static final double MIN = 0.01;
    private static final double MAX = 100.00;

    private final double percentile;

    private Percentile(double percentile) {
        this.percentile = percentile;
    }

    public Double asDouble() {
        return percentile;
    }

    /** Validates range and precision, throwing IllegalArgumentException when out of bounds. */
    public static Percentile createStrict(double percentile) {
        if (percentile < MIN || percentile > MAX) {
            throw new IllegalArgumentException("The 'percentile' must be in the range (0, 100].");
        }
        if (BigDecimal.valueOf(percentile).stripTrailingZeros().scale() > 2) {
            throw new IllegalArgumentException("The 'percentile' has over two decimal places.");
        }
        return new Percentile(percentile);
    }

    public static Percentile createStrict(Number percentile) {
        return createStrict(percentile.doubleValue());
    }

    /** Clamps into range and rounds to two decimal places. */
    public static Percentile createCoerced(double percentile) {
        double clamped = Math.max(MIN, Math.min(MAX, percentile));
        return new Percentile(Math.round(clamped * 100) / 100.0);
    }

    public static Percentile createCoerced(Number percentile) {
        return createCoerced(percentile.doubleValue());
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof Percentile && Double.compare(((Percentile) o).percentile, percentile) == 0;
    }

    @Override
    public int hashCode() {
        return Double.hashCode(percentile);
    }

    @Override
    public String toString() {
        return "Percentile[" + percentile + "]";
    }
}
