package io.github.dorkitude.incisive.client.exceptions;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;

/**
 * Raised when the server answers with a non-2xx status (or an unusable body).
 *
 * <p>{@link #getMessage()} is the raw response body, so code that logged or parsed the message
 * keeps working. In addition, {@link #getStatusCode()}, {@link #getErrorCode()} and
 * {@link #getServerMessage()} expose the HTTP status and the {@code error_code} / {@code message}
 * fields of a JSON error body when present.
 */
public class ServerException extends IncisiveException {
    private static final long serialVersionUID = 1L;
    private static final ObjectMapper MAPPER = new ObjectMapper();

    private final int statusCode;
    private final String errorCode;
    private final String serverMessage;
    private final String body;

    public ServerException() {
        this(-1, null, (Throwable) null);
    }

    public ServerException(Throwable cause) {
        super(cause);
        this.statusCode = -1;
        this.errorCode = null;
        this.serverMessage = null;
        this.body = null;
    }

    public ServerException(String message) {
        this(-1, message, (Throwable) null);
    }

    public ServerException(String message, Throwable cause) {
        this(-1, message, cause);
    }

    /** Builds the exception from an HTTP status and raw response body. */
    public ServerException(int statusCode, String body) {
        this(statusCode, body, (Throwable) null);
    }

    private ServerException(int statusCode, String body, Throwable cause) {
        super(body, cause);
        this.statusCode = statusCode;
        this.body = body;
        String code = null;
        String msg = null;
        if (body != null && body.trim().startsWith("{")) {
            try {
                JsonNode node = MAPPER.readTree(body);
                if (node != null && node.isObject()) {
                    JsonNode c = node.get("error_code");
                    JsonNode m = node.get("message");
                    code = (c == null || c.isNull()) ? null : c.asText();
                    msg = (m == null || m.isNull()) ? null : m.asText();
                }
            } catch (Exception ignored) {
                // not JSON: leave fields null
            }
        }
        this.errorCode = code;
        this.serverMessage = msg;
    }

    /** HTTP status code, or -1 when not produced from an HTTP response. */
    public int getStatusCode() {
        return statusCode;
    }

    /** The {@code error_code} field of a JSON error body, or null. */
    public String getErrorCode() {
        return errorCode;
    }

    /** The {@code message} field of a JSON error body, or null. */
    public String getServerMessage() {
        return serverMessage;
    }

    /** The raw response body, or null. */
    public String getBody() {
        return body;
    }
}
