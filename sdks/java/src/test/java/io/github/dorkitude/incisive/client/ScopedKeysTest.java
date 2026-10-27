package io.github.dorkitude.incisive.client;

import io.github.dorkitude.incisive.client.exceptions.ScopedKeyException;
import org.junit.jupiter.api.Test;

import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Vectors were produced by running the upstream 6.0.0 Java library (ScopedKeys.encrypt /
 * encrypt32CharacterKey) outside this repository with the same options map.
 */
class ScopedKeysTest {
    static final String KEY_64 = "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF";
    static final String KEY_32 = "abcdefghijklmnopqrstuvwxyz012345";
    static final String KEY_20 = "shortkey0123456789ab";

    static final String VECTOR_64 = "5D64330C70EB50AEF13580194886C5C93F74C86C0FDB951CBE9C32D5A299FAADFA82F5C0EFC25C482BE60B1B388E5684CBE78FFF1F076D49989500F7DE6F94ABD9D2B4A4A19159E62888A01EAC73B10960707002F5BD9D3B77044D203DB1EA31FCB66B05419A8F1ACAF4E29E6F5416B2F3FBBCA3425E4789DAC14B06D0C4B18B";
    static final String VECTOR_32 = "E7907E7798549160AD1F9F131ADCFCCAC9E7E1940EE55908BFE655745E4280E856841AEB0035F4898BA42AD363BA700227D75EB30C34501A3CBCD70ECAF53E7684514743CC326A769536B18A8155A37B8DA747619347649B2BD282EC7EBD25DEABE698413B2ED47619E433F3A52DEAC4E329F1D1FC2B617F35933EA4E37EDA89";
    static final String VECTOR_20 = "E5246705CB5B1EF14A63CF3A76024B46AEBECC89431CCFE0FC6EB9BC098C875FBF28453B328BA6D48AFD99D4535362979D8B91B9B09879E486415AF4876C2990DEF119735C107F162A5F0790E426C452C9EBB2511A397587A2C82004A68473B7AF99D59174009E0153FDF9DBC04F2C5B0FE1799660BB57C3EE335B7529B41BD5";

    static Map<String, Object> options() {
        Map<String, Object> filter = new LinkedHashMap<>();
        filter.put("property_name", "user.id");
        filter.put("operator", "eq");
        filter.put("property_value", "123");
        Map<String, Object> opts = new LinkedHashMap<>();
        opts.put("allowed_operations", Arrays.asList("read"));
        opts.put("filters", Arrays.asList(filter));
        return opts;
    }

    @SuppressWarnings("unchecked")
    static void assertOptions(Map<String, Object> decrypted) {
        assertEquals(List.of("read"), decrypted.get("allowed_operations"));
        Map<String, Object> f = ((List<Map<String, Object>>) decrypted.get("filters")).get(0);
        assertEquals("user.id", f.get("property_name"));
        assertEquals("eq", f.get("operator"));
        assertEquals("123", f.get("property_value"));
    }

    @Test
    void decryptsUpstreamVectors() {
        assertOptions(ScopedKeys.decrypt(KEY_64, VECTOR_64));
        assertOptions(ScopedKeys.decrypt(KEY_32, VECTOR_32));
        assertOptions(ScopedKeys.decrypt(KEY_20, VECTOR_20));
        assertEquals(options(), ScopedKeys.decrypt(KEY_64, VECTOR_64.toLowerCase()));
    }

    @Test
    void roundTripsAndFormat() {
        for (String key : new String[]{KEY_64, KEY_32, KEY_20}) {
            String scoped = ScopedKeys.encrypt(key, options());
            assertTrue(scoped.matches("[0-9A-F]+"), scoped);
            assertEquals(0, (scoped.length() - 32) % 32, "IV + whole AES blocks");
            assertEquals(VECTOR_64.length(), scoped.length());
            assertEquals(options(), ScopedKeys.decrypt(key, scoped));
            assertNotEquals(scoped, ScopedKeys.encrypt(key, options()), "random IV per call");
        }
    }

    @Test
    void wrongKeyFails() {
        assertThrows(ScopedKeyException.class,
                () -> ScopedKeys.decrypt("0000000000000000000000000000000000000000000000000000000000000000", VECTOR_64));
    }

    @Test
    void padding() {
        assertEquals(KEY_32, ScopedKeys.padApiKey(KEY_32));
        String padded = ScopedKeys.padApiKey(KEY_20);
        assertEquals(32, padded.length());
        assertEquals((char) 12, padded.charAt(31));
    }

    @Test
    void nullOptionsEncryptAsEmptyObject() {
        assertTrue(ScopedKeys.decrypt(KEY_64, ScopedKeys.encrypt(KEY_64, null)).isEmpty());
    }
}
