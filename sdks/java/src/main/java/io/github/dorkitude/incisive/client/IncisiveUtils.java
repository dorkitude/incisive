package io.github.dorkitude.incisive.client;

/** Hex helpers. */
public final class IncisiveUtils {
    private static final char[] HEX = "0123456789ABCDEF".toCharArray();

    private IncisiveUtils() {}

    /** Decodes a hex string (either case). */
    public static byte[] hexStringToByteArray(String s) {
        int len = s.length();
        if (len % 2 != 0) {
            throw new IllegalArgumentException("Hex string must have an even length");
        }
        byte[] out = new byte[len / 2];
        for (int i = 0; i < len; i += 2) {
            int hi = Character.digit(s.charAt(i), 16);
            int lo = Character.digit(s.charAt(i + 1), 16);
            if (hi < 0 || lo < 0) {
                throw new IllegalArgumentException("Invalid hex character in: " + s);
            }
            out[i / 2] = (byte) ((hi << 4) | lo);
        }
        return out;
    }

    /** Encodes bytes as UPPERCASE hex. */
    public static String byteArrayToHexString(byte[] bytes) {
        char[] out = new char[bytes.length * 2];
        for (int i = 0; i < bytes.length; i++) {
            int b = bytes[i] & 0xFF;
            out[i * 2] = HEX[b >>> 4];
            out[i * 2 + 1] = HEX[b & 0x0F];
        }
        return new String(out);
    }
}
