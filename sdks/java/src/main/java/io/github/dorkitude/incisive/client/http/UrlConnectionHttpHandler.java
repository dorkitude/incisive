package io.github.dorkitude.incisive.client.http;

import io.github.dorkitude.incisive.client.IncisiveVersion;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.nio.charset.StandardCharsets;

/**
 * Default {@link HttpHandler} built on {@link HttpURLConnection}.
 *
 * <p>Sends {@code Accept: application/json}, {@code Authorization: <key>} (raw key, no scheme),
 * {@code User-Agent: incisive-java/<version>} and, only when a body is present,
 * {@code Content-Type: application/json}.
 */
public class UrlConnectionHttpHandler implements HttpHandler {

    @Override
    public Response execute(Request request) throws IOException {
        HttpURLConnection connection = openConnection(request);
        try {
            sendRequest(connection, request);
            return readResponse(connection);
        } finally {
            connection.disconnect();
        }
    }

    protected HttpURLConnection openConnection(Request request) throws IOException {
        HttpURLConnection result = (HttpURLConnection) (request.proxy != null
                ? request.url.openConnection(request.proxy)
                : request.url.openConnection());
        result.setConnectTimeout(request.connectTimeout);
        result.setReadTimeout(request.readTimeout);
        return result;
    }

    protected void sendRequest(HttpURLConnection connection, Request request) throws IOException {
        connection.setRequestMethod(request.method);
        connection.setRequestProperty("Accept", "application/json");
        if (request.authorization != null) {
            connection.setRequestProperty("Authorization", request.authorization);
        }
        connection.setRequestProperty("User-Agent", "incisive-java/" + IncisiveVersion.getSdkVersion());

        if (request.body != null) {
            if (HttpMethods.GET.equals(request.method) || HttpMethods.DELETE.equals(request.method)) {
                throw new IllegalStateException("Trying to send a " + request.method
                        + " request with a request body.");
            }
            connection.setDoOutput(true);
            connection.setRequestProperty("Content-Type", "application/json");
            try (OutputStream out = connection.getOutputStream()) {
                request.body.writeTo(out);
                out.flush();
            }
        } else {
            connection.connect();
        }
    }

    protected Response readResponse(HttpURLConnection connection) throws IOException {
        int status = connection.getResponseCode();
        InputStream in;
        try {
            in = connection.getInputStream();
        } catch (IOException e) {
            in = connection.getErrorStream();
        }
        String body = "";
        if (in != null) {
            try (InputStream stream = in) {
                ByteArrayOutputStream buffer = new ByteArrayOutputStream();
                byte[] chunk = new byte[8192];
                int n;
                while ((n = stream.read(chunk)) != -1) {
                    buffer.write(chunk, 0, n);
                }
                body = buffer.toString(StandardCharsets.UTF_8);
            }
        }
        return new Response(status, body);
    }
}
