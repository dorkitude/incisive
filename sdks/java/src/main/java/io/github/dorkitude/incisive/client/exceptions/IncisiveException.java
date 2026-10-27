package io.github.dorkitude.incisive.client.exceptions;

/** Base class of all SDK exceptions (unchecked). */
public abstract class IncisiveException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    public IncisiveException() {
        super();
    }

    public IncisiveException(Throwable cause) {
        super(cause);
    }

    public IncisiveException(String message) {
        super(message);
    }

    public IncisiveException(String message, Throwable cause) {
        super(message, cause);
    }
}
