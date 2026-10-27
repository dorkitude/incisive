package io.github.dorkitude.incisive.client;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.sun.net.httpserver.HttpServer;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.net.URLDecoder;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Deque;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** In-process HTTP server on 127.0.0.1:0 that records requests and replays canned responses. */
public final class MockServer implements AutoCloseable {
    public static final ObjectMapper MAPPER = new ObjectMapper();

    /** A recorded request. */
    public static final class Recorded {
        public final String method;
        public final String rawPath;
        public final String rawQuery;
        public final Map<String, String> headers;
        public final String body;

        Recorded(String method, String rawPath, String rawQuery, Map<String, String> headers, String body) {
            this.method = method;
            this.rawPath = rawPath;
            this.rawQuery = rawQuery;
            this.headers = headers;
            this.body = body;
        }

        /** Header lookup, case-insensitive; null if absent. */
        public String header(String name) {
            for (Map.Entry<String, String> e : headers.entrySet()) {
                if (e.getKey().equalsIgnoreCase(name)) {
                    return e.getValue();
                }
            }
            return null;
        }

        public JsonNode json() throws IOException {
            return MAPPER.readTree(body);
        }

        /** Decoded query parameters (form decoding). */
        public Map<String, String> query() {
            Map<String, String> out = new LinkedHashMap<>();
            if (rawQuery == null || rawQuery.isEmpty()) {
                return out;
            }
            for (String pair : rawQuery.split("&")) {
                int i = pair.indexOf('=');
                String k = i < 0 ? pair : pair.substring(0, i);
                String v = i < 0 ? "" : pair.substring(i + 1);
                out.put(URLDecoder.decode(k, StandardCharsets.UTF_8), URLDecoder.decode(v, StandardCharsets.UTF_8));
            }
            return out;
        }
    }

    private static final class Canned {
        final int status;
        final String body;

        Canned(int status, String body) {
            this.status = status;
            this.body = body;
        }
    }

    private final HttpServer server;
    private final Deque<Canned> responses = new ArrayDeque<>();
    private final List<Recorded> requests = Collections.synchronizedList(new ArrayList<>());

    public MockServer() throws IOException {
        server = HttpServer.create(new InetSocketAddress("127.0.0.1", 0), 0);
        server.createContext("/", exchange -> {
            byte[] reqBody;
            try (InputStream in = exchange.getRequestBody()) {
                ByteArrayOutputStream buf = new ByteArrayOutputStream();
                in.transferTo(buf);
                reqBody = buf.toByteArray();
            }
            Map<String, String> headers = new LinkedHashMap<>();
            exchange.getRequestHeaders().forEach((k, v) -> headers.put(k, String.join(",", v)));
            requests.add(new Recorded(exchange.getRequestMethod(), exchange.getRequestURI().getRawPath(),
                    exchange.getRequestURI().getRawQuery(), headers, new String(reqBody, StandardCharsets.UTF_8)));
            Canned c;
            synchronized (responses) {
                c = responses.isEmpty() ? new Canned(200, "{}") : responses.poll();
            }
            byte[] out = c.body == null ? new byte[0] : c.body.getBytes(StandardCharsets.UTF_8);
            exchange.getResponseHeaders().add("Content-Type", "application/json");
            if (c.status == 204 || out.length == 0) {
                exchange.sendResponseHeaders(c.status, -1);
            } else {
                exchange.sendResponseHeaders(c.status, out.length);
                try (OutputStream os = exchange.getResponseBody()) {
                    os.write(out);
                }
            }
            exchange.close();
        });
        server.start();
    }

    public String baseUrl() {
        return "http://127.0.0.1:" + server.getAddress().getPort();
    }

    public MockServer respond(int status, String body) {
        synchronized (responses) {
            responses.add(new Canned(status, body));
        }
        return this;
    }

    public List<Recorded> requests() {
        return new ArrayList<>(requests);
    }

    public Recorded last() {
        return requests.get(requests.size() - 1);
    }

    public static JsonNode json(String s) throws IOException {
        return MAPPER.readTree(s);
    }

    @Override
    public void close() {
        server.stop(0);
    }
}
