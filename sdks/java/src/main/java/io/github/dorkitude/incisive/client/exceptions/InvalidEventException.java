package io.github.dorkitude.incisive.client.exceptions;

/** Raised when an event fails client-side validation. */
public class InvalidEventException extends IncisiveException {
    private static final long serialVersionUID = 1L;

    public InvalidEventException(String message) {
        super(message);
    }
}
