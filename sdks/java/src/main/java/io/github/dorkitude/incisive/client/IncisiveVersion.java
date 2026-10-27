package io.github.dorkitude.incisive.client;

/** SDK version, used in the User-Agent header. */
public final class IncisiveVersion {
    private static final String SDK_VERSION = "0.1.0";

    private IncisiveVersion() {}

    public static String getSdkVersion() {
        return SDK_VERSION;
    }
}
