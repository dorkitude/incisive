package io.github.dorkitude.incisive.client;

/**
 * Builder for plain JVM applications (Jackson JSON, RAM event store, HttpURLConnection).
 * Equivalent to {@link IncisiveClient.Builder}; kept as a separate type so code written against a
 * platform-specific builder ports directly.
 */
public class JavaIncisiveClientBuilder extends IncisiveClient.Builder {
    @Override
    protected IncisiveJsonHandler getDefaultJsonHandler() {
        return new JacksonJsonHandler();
    }
}
