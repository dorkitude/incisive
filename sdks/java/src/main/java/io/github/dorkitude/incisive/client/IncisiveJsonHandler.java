package io.github.dorkitude.incisive.client;

import java.io.IOException;
import java.io.Reader;
import java.io.Writer;
import java.util.Map;

/**
 * JSON codec used by the clients. {@link #readJson} returns a map; a JSON array root is returned
 * wrapped under {@link IncisiveConstants#FAKE_JSON_ROOT}.
 */
public interface IncisiveJsonHandler {
    Map<String, Object> readJson(Reader reader) throws IOException;

    void writeJson(Writer writer, Map<String, ?> value) throws IOException;
}
