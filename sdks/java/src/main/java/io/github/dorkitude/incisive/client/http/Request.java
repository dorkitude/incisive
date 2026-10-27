package io.github.dorkitude.incisive.client.http;

import java.net.Proxy;
import java.net.URL;

/** An HTTP request as built by the SDK. {@code authorization} is the raw API key. */
public final class Request {
    public final URL url;
    public final String method;
    public final String authorization;
    public final OutputSource body;
    public final Proxy proxy;
    public final int connectTimeout;
    public final int readTimeout;

    public Request(URL url, String method, String authorization, OutputSource body) {
        this(url, method, authorization, body, null, 30000, 30000);
    }

    public Request(URL url, String method, String authorization, OutputSource body, Proxy proxy,
                   int connectTimeout, int readTimeout) {
        this.url = url;
        this.method = method;
        this.authorization = authorization;
        this.body = body;
        this.proxy = proxy;
        this.connectTimeout = connectTimeout;
        this.readTimeout = readTimeout;
    }
}
