package io.github.dorkitude.incisive.client.exceptions;

/**
 * Raised by the query client for malformed responses, URL construction failures, or a 2xx body
 * that carries an {@code error_code} instead of a {@code result}.
 */
public class IncisiveQueryClientException extends IncisiveException {
    private static final long serialVersionUID = 1L;

    private final String errorCode;

    public IncisiveQueryClientException(Throwable cause) {
        super(cause);
        this.errorCode = null;
    }

    public IncisiveQueryClientException(String message) {
        this(message, (String) null);
    }

    public IncisiveQueryClientException(String message, Throwable cause) {
        super(message, cause);
        this.errorCode = null;
    }

    public IncisiveQueryClientException(String message, String errorCode) {
        super(message);
        this.errorCode = errorCode;
    }

    /** The server's {@code error_code}, when this was raised from an error body. */
    public String getErrorCode() {
        return errorCode;
    }
}
