package io.github.dorkitude.incisive.client.exceptions;

/** Raised when an event collection name is null, empty or too long. */
public class InvalidEventCollectionException extends IncisiveException {
    private static final long serialVersionUID = 1L;

    public InvalidEventCollectionException(String message) {
        super(message);
    }
}
