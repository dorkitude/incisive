package io.github.dorkitude.incisive.client;

import io.github.dorkitude.incisive.client.query.IncisiveQueryClient;
import io.github.dorkitude.incisive.client.query.RelativeTimeframe;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.condition.EnabledIfEnvironmentVariable;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Runs against a real server only when INCISIVE_BASE_URL, INCISIVE_PROJECT_ID,
 * INCISIVE_WRITE_KEY and INCISIVE_READ_KEY are set; skipped otherwise.
 */
@EnabledIfEnvironmentVariable(named = "INCISIVE_BASE_URL", matches = ".+")
@EnabledIfEnvironmentVariable(named = "INCISIVE_PROJECT_ID", matches = ".+")
@EnabledIfEnvironmentVariable(named = "INCISIVE_WRITE_KEY", matches = ".+")
@EnabledIfEnvironmentVariable(named = "INCISIVE_READ_KEY", matches = ".+")
class IntegrationTest {

    @Test
    void addEventThenCount() throws Exception {
        IncisiveClient client = new JavaIncisiveClientBuilder().build();
        client.setDebugMode(true);
        String collection = "java_it_" + UUID.randomUUID().toString().replace("-", "");

        Map<String, Object> event = new LinkedHashMap<>();
        event.put("n", 1);
        client.addEvent(collection, event);

        Map<String, Object> e2 = new LinkedHashMap<>();
        e2.put("n", 2);
        client.queueEvent(collection, e2);
        List<Exception> failures = new ArrayList<>();
        client.sendQueuedEvents(null, new IncisiveCallback() {
            @Override public void onSuccess() {}
            @Override public void onFailure(Exception e) { failures.add(e); }
        });
        assertTrue(failures.isEmpty(), failures.toString());

        IncisiveQueryClient queries = new IncisiveQueryClient.Builder().build();
        long count = 0;
        for (int i = 0; i < 20 && count < 2; i++) {
            count = queries.count(collection, new RelativeTimeframe("this_1_days"));
            if (count < 2) {
                Thread.sleep(500);
            }
        }
        assertEquals(2, count);
        assertEquals(3.0, queries.sum(collection, "n", new RelativeTimeframe("this_1_days")));
    }
}
