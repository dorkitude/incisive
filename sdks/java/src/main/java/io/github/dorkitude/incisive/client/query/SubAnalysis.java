package io.github.dorkitude.incisive.client.query;

import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;

/** One labeled analysis inside a {@link MultiAnalysis}. */
public class SubAnalysis extends RequestParameter<Map<String, Object>> {
    private final String label;
    private final QueryType analysisType;
    private final String targetPropertyName;
    private final Percentile percentile;

    public SubAnalysis(String label, QueryType analysisType) {
        this(label, analysisType, null);
    }

    public SubAnalysis(String label, QueryType analysisType, String targetPropertyName) {
        this(label, analysisType, targetPropertyName, null);
    }

    public SubAnalysis(String label, QueryType analysisType, String targetPropertyName, Percentile percentile) {
        if (label == null || label.trim().isEmpty()) {
            throw new IllegalArgumentException("SubAnalysis parameter 'label' is required.");
        }
        if (analysisType == null) {
            throw new IllegalArgumentException("SubAnalysis parameter 'analysisType' is required.");
        }
        if (analysisType != QueryType.COUNT && (targetPropertyName == null || targetPropertyName.trim().isEmpty())) {
            throw new IllegalArgumentException("All SubAnalysis types but 'count' require a target property name.");
        }
        if (analysisType == QueryType.COUNT && targetPropertyName != null) {
            throw new IllegalArgumentException("SubAnalysis of analysis type 'count' should not specify a 'targetPropertyName' parameter.");
        }
        if ((analysisType == QueryType.PERCENTILE) ^ (percentile != null)) {
            throw new IllegalArgumentException("SubAnalysis parameter 'percentile' is both required and only allowed for the 'percentile' analysis type.");
        }
        this.label = label;
        this.analysisType = analysisType;
        this.targetPropertyName = targetPropertyName;
        this.percentile = percentile;
    }

    public String getLabel() {
        return label;
    }

    public QueryType getAnalysisType() {
        return analysisType;
    }

    public String getTargetPropertyName() {
        return targetPropertyName;
    }

    public Percentile getPercentile() {
        return percentile;
    }

    @Override
    Map<String, Object> constructParameterRequestArgs() {
        Map<String, Object> args = new LinkedHashMap<>();
        args.put(QueryConstants.ANALYSIS_TYPE, analysisType.toString());
        if (analysisType != QueryType.COUNT) args.put(QueryConstants.TARGET_PROPERTY, targetPropertyName);
        if (analysisType == QueryType.PERCENTILE) args.put(QueryConstants.PERCENTILE, percentile.asDouble());
        return args;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof SubAnalysis)) return false;
        SubAnalysis s = (SubAnalysis) o;
        return label.equals(s.label) && analysisType == s.analysisType
                && Objects.equals(targetPropertyName, s.targetPropertyName) && Objects.equals(percentile, s.percentile);
    }

    @Override
    public int hashCode() {
        return Objects.hash(label, analysisType, targetPropertyName, percentile);
    }

    @Override
    public String toString() {
        return "SubAnalysis[" + label + ": " + analysisType + (targetPropertyName == null ? "" : " " + targetPropertyName) + "]";
    }
}
