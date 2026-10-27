package io.github.dorkitude.incisive.client.exceptions;

/** Raised when scoped key encryption or decryption fails. */
public class ScopedKeyException extends IncisiveException {
    private static final long serialVersionUID = 1L;

    public ScopedKeyException(String message, Throwable cause) {
        super(message, cause);
    }
}
