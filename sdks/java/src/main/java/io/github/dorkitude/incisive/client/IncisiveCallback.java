package io.github.dorkitude.incisive.client;

/** Completion callback for tracking operations. Exceptions thrown by callbacks are swallowed. */
public interface IncisiveCallback {
    void onSuccess();

    void onFailure(Exception e);
}
