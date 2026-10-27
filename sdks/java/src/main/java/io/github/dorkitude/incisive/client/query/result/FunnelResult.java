package io.github.dorkitude.incisive.client.query.result;

import java.util.Objects;

/** A funnel result: per-step counts and, if requested, per-step actor lists. */
public class FunnelResult extends QueryResult {
    private final ListResult funnelResult;
    private final ListResult actorsResult;

    public FunnelResult(ListResult result, ListResult actorsResult) {
        if (result == null) {
            throw new IllegalArgumentException("No result value provided");
        }
        this.funnelResult = result;
        this.actorsResult = actorsResult;
    }

    public ListResult getFunnelResult() {
        return funnelResult;
    }

    /** Null unless a step requested {@code with_actors}. */
    public ListResult getActorsResult() {
        return actorsResult;
    }

    @Override
    public boolean equals(Object o) {
        if (!(o instanceof FunnelResult)) return false;
        FunnelResult f = (FunnelResult) o;
        return funnelResult.equals(f.funnelResult) && Objects.equals(actorsResult, f.actorsResult);
    }

    @Override
    public int hashCode() {
        return Objects.hash(funnelResult, actorsResult);
    }

    @Override
    public String toString() {
        return "FunnelResult[" + funnelResult + ", actors=" + actorsResult + "]";
    }
}
