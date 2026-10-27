package io.github.dorkitude.incisive.client.http;

import java.io.IOException;
import java.io.OutputStream;

/** Lazily writes a request body to the connection's output stream. */
public interface OutputSource {
    void writeTo(OutputStream out) throws IOException;
}
