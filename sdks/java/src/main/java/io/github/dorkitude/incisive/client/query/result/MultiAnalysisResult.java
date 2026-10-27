package io.github.dorkitude.incisive.client.query.result;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;

/** A multi_analysis result: one result per sub-analysis label. */
public class MultiAnalysisResult extends QueryResult {
    private final Map<String, QueryResult> analysesResults;

    public MultiAnalysisResult(Map<String, QueryResult> analysesResults) {
        this.analysesResults = Collections.unmodifiableMap(new LinkedHashMap<>(analysesResults));
    }

    public QueryResult getResultFor(String subAnalysisLabel) {
        if (!analysesResults.containsKey(subAnalysisLabel)) {
            throw new IllegalArgumentException("No results for a sub-analysis with that label.");
        }
        return analysesResults.get(subAnalysisLabel);
    }

    public Map<String, QueryResult> getAllResults() {
        return analysesResults;
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof MultiAnalysisResult && ((MultiAnalysisResult) o).analysesResults.equals(analysesResults);
    }

    @Override
    public int hashCode() {
        return analysesResults.hashCode();
    }

    @Override
    public String toString() {
        return "MultiAnalysisResult" + analysesResults;
    }
}
