package io.github.dorkitude.incisive.client;

/**
 * Tells the client whether the network is reachable; queued events are not sent while offline.
 * On Android, implement this with ConnectivityManager.
 */
public interface IncisiveNetworkStatusHandler {
    boolean isNetworkConnected();
}
