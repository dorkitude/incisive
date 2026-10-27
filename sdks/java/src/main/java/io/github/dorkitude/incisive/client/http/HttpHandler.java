package io.github.dorkitude.incisive.client.http;

import java.io.IOException;

/**
 * Pluggable HTTP transport. Implementations execute a {@link Request} and return the status code
 * and body of the response; they must not throw for non-2xx statuses.
 */
public interface HttpHandler {
    Response execute(Request request) throws IOException;
}
