package io.github.dorkitude.incisive.client.http;

/** Status code and body of an HTTP response. */
public final class Response {
    public final int statusCode;
    public final String body;

    public Response(int statusCode, String body) {
        this.statusCode = statusCode;
        this.body = body;
    }

    /** True for any 2xx status. */
    public boolean isSuccess() {
        return statusCode / 100 == 2;
    }
}
