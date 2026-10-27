package io.github.dorkitude.incisive.client;

import com.fasterxml.jackson.core.JsonGenerator;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.SerializationFeature;
import com.fasterxml.jackson.databind.type.CollectionType;
import com.fasterxml.jackson.databind.type.MapType;
import com.fasterxml.jackson.databind.type.TypeFactory;

import java.io.IOException;
import java.io.Reader;
import java.io.Writer;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** Jackson-backed {@link IncisiveJsonHandler}. Integers decode as Integer/Long, decimals as Double. */
public class JacksonJsonHandler implements IncisiveJsonHandler {
    private static final MapType MAP_TYPE =
            TypeFactory.defaultInstance().constructMapType(LinkedHashMap.class, String.class, Object.class);
    private static final CollectionType LIST_TYPE =
            TypeFactory.defaultInstance().constructCollectionType(List.class, Object.class);

    private final ObjectMapper mapper;

    public JacksonJsonHandler() {
        mapper = new ObjectMapper();
        mapper.configure(SerializationFeature.WRITE_DATES_AS_TIMESTAMPS, false);
        mapper.configure(JsonGenerator.Feature.AUTO_CLOSE_TARGET, false);
    }

    @Override
    public Map<String, Object> readJson(Reader reader) throws IOException {
        JsonNode root = mapper.readTree(reader);
        if (root == null || root.isMissingNode()) {
            throw new IllegalArgumentException("Empty reader or ill-formatted JSON encountered.");
        }
        if (root.isArray()) {
            Map<String, Object> wrapped = new LinkedHashMap<>();
            wrapped.put(IncisiveConstants.FAKE_JSON_ROOT, mapper.convertValue(root, LIST_TYPE));
            return wrapped;
        }
        if (root.isObject()) {
            return mapper.convertValue(root, MAP_TYPE);
        }
        throw new IllegalArgumentException("JSON root must be an object or an array.");
    }

    @Override
    public void writeJson(Writer writer, Map<String, ?> value) throws IOException {
        mapper.writeValue(writer, value);
        writer.flush();
    }

    /** Serializes any value (used for query-string JSON parameters). */
    public String writeValueAsString(Object value) throws IOException {
        return mapper.writeValueAsString(value);
    }
}
