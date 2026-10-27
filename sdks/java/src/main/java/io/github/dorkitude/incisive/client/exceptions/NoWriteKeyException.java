package io.github.dorkitude.incisive.client.exceptions;

/** Raised when an event is sent for a project that has no write key. */
public class NoWriteKeyException extends IncisiveException {
    private static final long serialVersionUID = 1L;

    public NoWriteKeyException(String message) {
        super(message);
    }
}
