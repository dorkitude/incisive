package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.exceptions.IncisiveQueryClientException;

import java.net.URL;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.Map;

/**
 * Several labeled analyses over one collection, sent to {@code .../queries/multi_analysis} with
 * {@code "analyses": {label: {analysis_type, target_property?, percentile?}}}.
 */
public class MultiAnalysis extends CollectionAnalysis {
    private final Collection<SubAnalysis> analyses;

    protected MultiAnalysis(Builder builder) {
        super(builder);
        this.analyses = builder.subAnalyses == null ? null : new ArrayList<>(builder.subAnalyses);
    }

    @Override
    URL getRequestURL(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException {
        return urlBuilder.getAnalysisUrl(projectId, getAnalysisType());
    }

    @Override
    String getAnalysisType() {
        return QueryConstants.MULTI_ANALYSIS;
    }

    @Override
    Map<String, Object> constructRequestArgs() {
        Map<String, Object> args = super.constructRequestArgs();
        Map<String, Object> analysesArgs = new LinkedHashMap<>();
        for (SubAnalysis s : analyses) {
            analysesArgs.put(s.getLabel(), s.constructParameterRequestArgs());
        }
        args.put(QueryConstants.ANALYSES, analysesArgs);
        return args;
    }

    @Override
    protected void validateParams() {
        super.validateParams();
        if (analyses == null || analyses.isEmpty()) {
            throw new IllegalArgumentException("MultiAnalysis requires that 'analyses' be set.");
        }
    }

    public static class Builder extends CollectionAnalysis.Builder<Builder> {
        private LinkedHashSet<SubAnalysis> subAnalyses;

        public Collection<SubAnalysis> getSubAnalyses() {
            return subAnalyses;
        }

        public void setSubAnalyses(Collection<? extends SubAnalysis> subAnalyses) {
            this.subAnalyses = null;
            if (subAnalyses != null) {
                for (SubAnalysis s : subAnalyses) {
                    addSubAnalysis(s);
                }
            }
        }

        public Builder withSubAnalyses(Collection<? extends SubAnalysis> subAnalyses) {
            if (subAnalyses == null) {
                throw new IllegalArgumentException("The 'subAnalyses' parameter cannot be null.");
            }
            setSubAnalyses(subAnalyses);
            return this;
        }

        public Builder withSubAnalysis(SubAnalysis subAnalysis) {
            addSubAnalysis(subAnalysis);
            return this;
        }

        public void addSubAnalysis(SubAnalysis subAnalysis) {
            if (subAnalysis == null) {
                throw new IllegalArgumentException("The 'subAnalysis' parameter cannot be null.");
            }
            if (subAnalyses == null) {
                subAnalyses = new LinkedHashSet<>();
            }
            subAnalyses.add(subAnalysis);
        }

        @Override
        protected Builder getThis() {
            return this;
        }

        public MultiAnalysis build() {
            MultiAnalysis m = new MultiAnalysis(this);
            m.validateParams();
            return m;
        }
    }
}
