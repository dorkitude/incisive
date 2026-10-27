package io.github.dorkitude.incisive.client;

/** Default network status handler: always online. */
public class AlwaysConnectedNetworkStatusHandler implements IncisiveNetworkStatusHandler {
    @Override
    public boolean isNetworkConnected() {
        return true;
    }
}
