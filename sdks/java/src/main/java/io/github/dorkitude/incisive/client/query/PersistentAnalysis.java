package io.github.dorkitude.incisive.client.query;

import io.github.dorkitude.incisive.client.IncisiveProject;
import io.github.dorkitude.incisive.client.http.HttpMethods;

/** Base of saved query and cached dataset requests: a named resource and a key choice. */
abstract class PersistentAnalysis extends IncisiveQueryRequest {
    private static final String RESOURCE_NAME_REGEX = "^[\\w-]*$";

    private final String httpMethod;
    private final boolean needsMasterKey;
    private final String resourceName;
    private final String displayName;

    PersistentAnalysis(String httpMethod, boolean needsMasterKey, String resourceName, String displayName) {
        this.httpMethod = httpMethod;
        this.needsMasterKey = needsMasterKey;
        if (resourceName == null && !HttpMethods.GET.equals(httpMethod)) {
            throw new IllegalArgumentException("A resource name is required if not a GET request.");
        }
        if (resourceName != null) {
            validateResourceName(resourceName);
        }
        this.resourceName = resourceName;
        if (displayName != null) {
            validateDisplayName(displayName);
        }
        this.displayName = displayName;
    }

    String getResourceName() {
        return resourceName;
    }

    String getDisplayName() {
        return displayName;
    }

    boolean retrievingResults() {
        return false;
    }

    @Override
    String getHttpMethod() {
        return httpMethod;
    }

    @Override
    String getAuthKey(IncisiveProject project) {
        return needsMasterKey ? project.getMasterKey() : project.getReadKey();
    }

    @Override
    boolean groupedResponseExpected() {
        return false;
    }

    @Override
    boolean intervalResponseExpected() {
        return false;
    }

    static void validateResourceName(String resourceName) {
        if (resourceName == null || !resourceName.matches(RESOURCE_NAME_REGEX)) {
            throw new IllegalArgumentException("The resource name can only be comprised of alphanumerics, hyphens and underscores.");
        }
    }

    static void validateDisplayName(String displayName) {
        if (displayName == null || displayName.trim().isEmpty()) {
            throw new IllegalArgumentException("The display name cannot be null, empty or whitespace only.");
        }
    }
}
