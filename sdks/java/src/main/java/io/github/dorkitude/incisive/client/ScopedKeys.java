package io.github.dorkitude.incisive.client;

import io.github.dorkitude.incisive.client.exceptions.ScopedKeyException;

import java.io.StringReader;
import java.io.StringWriter;
import java.nio.charset.StandardCharsets;
import java.security.SecureRandom;
import java.util.HashMap;
import java.util.Map;
import javax.crypto.Cipher;
import javax.crypto.spec.IvParameterSpec;
import javax.crypto.spec.SecretKeySpec;

/**
 * Scoped key encryption and decryption.
 *
 * <p>Algorithm: AES/CBC/PKCS5Padding with a random 16-byte IV; the plaintext is the JSON encoding
 * of the options map; the output is UPPERCASE hex(IV) followed by hex(ciphertext).
 * <ul>
 *   <li>64-character keys are hex-decoded to a 32-byte AES-256 key.</li>
 *   <li>Any other key is used as text: if its length is not a multiple of 32 it is padded with
 *   {@code n} copies of {@code (char) n} up to the next multiple of 32, and the UTF-8 bytes of the
 *   result are the AES key (so a 32-character key gives AES-256 over its ASCII bytes).</li>
 * </ul>
 */
public final class ScopedKeys {
    private static final int BLOCK_SIZE = 32;
    private static final String TRANSFORMATION = "AES/CBC/PKCS5Padding";
    private static final SecureRandom RANDOM = new SecureRandom();

    private ScopedKeys() {}

    /** Encrypts {@code options} with {@code apiKey}, dispatching on key length. */
    public static String encrypt(String apiKey, Map<String, Object> options) throws ScopedKeyException {
        return apiKey != null && apiKey.length() == 64
                ? encrypt(defaultClient(), apiKey, options)
                : encrypt32CharacterKey(defaultClient(), apiKey, options);
    }

    /** Encrypts with a 64-character hex key, using {@code client}'s JSON handler (may be null). */
    public static String encrypt(IncisiveClient client, String apiKey, Map<String, Object> options)
            throws ScopedKeyException {
        try {
            return encryptWith(client, IncisiveUtils.hexStringToByteArray(apiKey), options);
        } catch (Exception e) {
            throw new ScopedKeyException("An error occurred while attempting to encrypt a Scoped Key", e);
        }
    }

    /** Encrypts with a text key (32-character style), using {@code client}'s JSON handler (may be null). */
    public static String encrypt32CharacterKey(IncisiveClient client, String apiKey, Map<String, Object> options)
            throws ScopedKeyException {
        try {
            return encryptWith(client, padApiKey(apiKey).getBytes(StandardCharsets.UTF_8), options);
        } catch (Exception e) {
            throw new ScopedKeyException("An error occurred while attempting to encrypt a Scoped Key", e);
        }
    }

    /** Decrypts {@code scopedKey} with {@code apiKey}, dispatching on key length. */
    public static Map<String, Object> decrypt(String apiKey, String scopedKey) throws ScopedKeyException {
        return apiKey != null && apiKey.length() == 64
                ? decrypt(defaultClient(), apiKey, scopedKey)
                : decrypt32CharacterKey(defaultClient(), apiKey, scopedKey);
    }

    public static Map<String, Object> decrypt(IncisiveClient client, String apiKey, String scopedKey)
            throws ScopedKeyException {
        try {
            return decryptWith(client, IncisiveUtils.hexStringToByteArray(apiKey), scopedKey);
        } catch (Exception e) {
            throw new ScopedKeyException("An error occurred while attempting to decrypt a Scoped Key", e);
        }
    }

    public static Map<String, Object> decrypt32CharacterKey(IncisiveClient client, String apiKey, String scopedKey)
            throws ScopedKeyException {
        try {
            return decryptWith(client, padApiKey(apiKey).getBytes(StandardCharsets.UTF_8), scopedKey);
        } catch (Exception e) {
            throw new ScopedKeyException("An error occurred while attempting to decrypt a Scoped Key", e);
        }
    }

    private static String encryptWith(IncisiveClient client, byte[] key, Map<String, Object> options)
            throws Exception {
        Map<String, Object> opts = options == null ? new HashMap<>() : options;
        StringWriter writer = new StringWriter();
        jsonHandler(client).writeJson(writer, opts);
        byte[] iv = new byte[16];
        RANDOM.nextBytes(iv);
        Cipher cipher = Cipher.getInstance(TRANSFORMATION);
        cipher.init(Cipher.ENCRYPT_MODE, new SecretKeySpec(key, "AES"), new IvParameterSpec(iv));
        byte[] cipherText = cipher.doFinal(writer.toString().getBytes(StandardCharsets.UTF_8));
        return IncisiveUtils.byteArrayToHexString(iv) + IncisiveUtils.byteArrayToHexString(cipherText);
    }

    private static Map<String, Object> decryptWith(IncisiveClient client, byte[] key, String scopedKey)
            throws Exception {
        byte[] iv = IncisiveUtils.hexStringToByteArray(scopedKey.substring(0, 32));
        byte[] cipherText = IncisiveUtils.hexStringToByteArray(scopedKey.substring(32));
        Cipher cipher = Cipher.getInstance(TRANSFORMATION);
        cipher.init(Cipher.DECRYPT_MODE, new SecretKeySpec(key, "AES"), new IvParameterSpec(iv));
        String plain = new String(cipher.doFinal(cipherText), StandardCharsets.UTF_8);
        return jsonHandler(client).readJson(new StringReader(plain));
    }

    static String padApiKey(String apiKey) {
        int rem = apiKey.length() % BLOCK_SIZE;
        if (rem == 0) {
            return apiKey;
        }
        int n = BLOCK_SIZE - rem;
        StringBuilder sb = new StringBuilder(apiKey);
        for (int i = 0; i < n; i++) {
            sb.append((char) n);
        }
        return sb.toString();
    }

    private static IncisiveClient defaultClient() {
        return IncisiveClient.isInitialized() ? IncisiveClient.client() : null;
    }

    private static IncisiveJsonHandler jsonHandler(IncisiveClient client) {
        if (client != null && client.getJsonHandler() != null) {
            return client.getJsonHandler();
        }
        return new JacksonJsonHandler();
    }
}
