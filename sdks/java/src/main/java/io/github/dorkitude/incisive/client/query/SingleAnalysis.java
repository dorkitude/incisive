package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.exceptions.IncisiveQueryClientException;

import java.net.URL;
import java.util.Map;

/** A strictly validated single analysis (alternative to {@link Query}). */
public class SingleAnalysis extends CollectionAnalysis {
    private final QueryType queryType;
    private final String targetPropertyName;
    private final Percentile percentile;

    protected SingleAnalysis(Builder builder) {
        super(builder);
        this.queryType = builder.queryType;
        this.targetPropertyName = builder.targetPropertyName;
        this.percentile = builder.percentile;
    }

    @Override
    URL getRequestURL(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException {
        return urlBuilder.getAnalysisUrl(projectId, getAnalysisType());
    }

    @Override
    String getAnalysisType() {
        return queryType.toString();
    }

    @Override
    Map<String, Object> constructRequestArgs() {
        Map<String, Object> args = super.constructRequestArgs();
        if (targetPropertyName != null) args.put(QueryConstants.TARGET_PROPERTY, targetPropertyName);
        if (percentile != null) args.put(QueryConstants.PERCENTILE, percentile.asDouble());
        return args;
    }

    @Override
    protected void validateParams() {
        super.validateParams();
        if (queryType == null) {
            throw new IllegalArgumentException("A 'queryType' is required.");
        }
        if (queryType == QueryType.COUNT && targetPropertyName != null) {
            throw new IllegalArgumentException("Analysis type 'count' should not specify a 'targetPropertyName' parameter.");
        }
        if (queryType != QueryType.COUNT && (targetPropertyName == null || targetPropertyName.trim().isEmpty())) {
            throw new IllegalArgumentException("All but the 'count' analysis type require a target property name.");
        }
        if ((queryType == QueryType.PERCENTILE) ^ (percentile != null)) {
            throw new IllegalArgumentException("Analysis parameter 'percentile' is both required and only allowed for the 'percentile' analysis type.");
        }
    }

    public static class Builder extends CollectionAnalysis.Builder<Builder> {
        private final QueryType queryType;
        private String targetPropertyName;
        private Percentile percentile;

        public Builder(QueryType queryType) {
            this.queryType = queryType;
        }

        public String getTargetPropertyName() {
            return targetPropertyName;
        }

        public void setTargetPropertyName(String targetPropertyName) {
            if (targetPropertyName == null || targetPropertyName.trim().isEmpty()) {
                throw new IllegalArgumentException("The 'targetPropertyName' parameter is null or empty.");
            }
            this.targetPropertyName = targetPropertyName;
        }

        public Builder withTargetPropertyName(String targetPropertyName) {
            if (this.targetPropertyName != null) {
                throw new IllegalArgumentException("Attempting to set the 'targetPropertyName' parameter multiple times.");
            }
            setTargetPropertyName(targetPropertyName);
            return this;
        }

        public Percentile getPercentile() {
            return percentile;
        }

        public void setPercentile(Percentile percentile) {
            if (percentile == null) {
                throw new IllegalArgumentException("The 'percentile' parameter cannot be null.");
            }
            this.percentile = percentile;
        }

        public Builder withPercentile(Percentile percentile) {
            if (this.percentile != null) {
                throw new IllegalArgumentException("Attempting to set the 'percentile' parameter multiple times.");
            }
            setPercentile(percentile);
            return this;
        }

        @Override
        protected Builder getThis() {
            return this;
        }

        public SingleAnalysis build() {
            SingleAnalysis analysis = new SingleAnalysis(this);
            analysis.validateParams();
            return analysis;
        }
    }
}
