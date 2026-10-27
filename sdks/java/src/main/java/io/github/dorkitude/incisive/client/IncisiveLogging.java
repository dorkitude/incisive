package io.github.dorkitude.incisive.client;

import java.util.logging.Level;
import java.util.logging.Logger;

/** Minimal java.util.logging wrapper. Disabled by default. */
public final class IncisiveLogging {
    private static final Logger LOGGER = Logger.getLogger("io.github.dorkitude.incisive");
    private static volatile boolean enabled = false;

    private IncisiveLogging() {}

    public static void enableLogging() {
        enabled = true;
    }

    public static void disableLogging() {
        enabled = false;
    }

    public static boolean isLoggingEnabled() {
        return enabled;
    }

    public static void log(String msg) {
        if (enabled) {
            LOGGER.log(Level.INFO, msg);
        }
    }

    public static void log(String msg, Throwable t) {
        if (enabled) {
            LOGGER.log(Level.INFO, msg, t);
        }
    }
}
