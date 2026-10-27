package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.IncisiveProject;
import io.github.dorkitude.incisive.client.exceptions.IncisiveQueryClientException;
import io.github.dorkitude.incisive.client.http.HttpMethods;

import java.net.URL;
import java.util.Collection;
import java.util.Map;

/**
 * Base type of everything the query client can execute ({@link Query}, {@link SingleAnalysis},
 * {@link MultiAnalysis}, {@link Funnel}, saved query and dataset requests).
 */
public abstract class IncisiveQueryRequest {
    IncisiveQueryRequest() {}

    abstract URL getRequestURL(RequestUrlBuilder urlBuilder, String projectId) throws IncisiveQueryClientException;

    /** POST by default. */
    String getHttpMethod() {
        return HttpMethods.POST;
    }

    /** Read key by default. */
    String getAuthKey(IncisiveProject project) {
        return project.getReadKey();
    }

    abstract String getAnalysisType();

    /** JSON body (or empty for GET/DELETE). */
    abstract Map<String, Object> constructRequestArgs();

    abstract boolean groupedResponseExpected();

    abstract boolean intervalResponseExpected();

    Collection<String> getGroupByParams() {
        throw new IllegalStateException("This request type cannot provide group_by parameters.");
    }
}
