package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.exceptions.IncisiveQueryClientException;

import java.net.URL;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** A funnel query: {@code POST .../queries/funnel} with {@code {"steps": [...], "timeframe"?}}. */
public class Funnel extends IncisiveQueryRequest {
    private final RequestParameterCollection<FunnelStep> steps;
    private final Timeframe timeframe;

    private Funnel(Builder builder) {
        if (builder.steps == null || builder.steps.isEmpty()) {
            throw new IllegalArgumentException("Funnel parameter steps must be provided.");
        }
        this.timeframe = builder.timeframe;
        if (timeframe == null) {
            for (FunnelStep step : builder.steps) {
                if (step.getTimeframe() == null) {
                    throw new IllegalArgumentException("A funnel step is missing a timeframe but no root "
                            + "timeframe was provided for the funnel request.");
                }
            }
        }
        FunnelStep first = builder.steps.get(0);
        if (Boolean.TRUE.equals(first.getInverted())) {
            throw new IllegalArgumentException("First step in funnel cannot have special parameter 'inverted' set to true.");
        }
        if (Boolean.TRUE.equals(first.getOptional())) {
            throw new IllegalArgumentException("First step in funnel cannot have special parameter 'optional' set to true.");
        }
        this.steps = new RequestParameterCollection<>(builder.steps);
    }

    @Override
    URL getRequestURL(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException {
        return urlBuilder.getAnalysisUrl(projectId, getAnalysisType());
    }

    @Override
    String getAnalysisType() {
        return QueryConstants.FUNNEL;
    }

    @Override
    Map<String, Object> constructRequestArgs() {
        Map<String, Object> args = new LinkedHashMap<>();
        args.put(QueryConstants.STEPS, steps.constructParameterRequestArgs());
        if (timeframe != null) args.putAll(timeframe.constructTimeframeArgs());
        return args;
    }

    @Override
    boolean groupedResponseExpected() {
        return false;
    }

    @Override
    boolean intervalResponseExpected() {
        return false;
    }

    public static class Builder {
        private List<FunnelStep> steps;
        private Timeframe timeframe;

        public void setSteps(List<? extends FunnelStep> steps) {
            this.steps = null;
            if (steps != null) {
                for (FunnelStep s : steps) {
                    addStep(s);
                }
            }
        }

        public Builder withSteps(List<? extends FunnelStep> steps) {
            if (this.steps != null) {
                throw new IncisiveQueryClientException("Incorrect Usage: withSteps() called, which would clear "
                        + "existing steps. Append additional steps with withStep().");
            }
            setSteps(steps);
            return this;
        }

        public List<FunnelStep> getSteps() {
            return steps;
        }

        public Builder withStep(FunnelStep step) {
            addStep(step);
            return this;
        }

        public void addStep(FunnelStep step) {
            if (steps == null) {
                steps = new ArrayList<>();
            }
            steps.add(step);
        }

        public Timeframe getTimeframe() {
            return timeframe;
        }

        public void setTimeframe(Timeframe timeframe) {
            this.timeframe = timeframe;
        }

        public Builder withTimeframe(Timeframe timeframe) {
            if (this.timeframe != null) {
                throw new IllegalStateException("'withTimeframe' called, but a Timeframe instance has already been set.");
            }
            setTimeframe(timeframe);
            return this;
        }

        public Funnel build() {
            return new Funnel(this);
        }
    }
}
