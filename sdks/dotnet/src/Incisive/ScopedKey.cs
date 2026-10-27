using System;
using System.Security.Cryptography;
using System.Text;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace Incisive
{
    /// <summary>
    /// Scoped key generation. A scoped key is
    /// <c>HEX(IV) + HEX(AES-256-CBC/PKCS7(UTF-8 JSON options))</c>, upper-case hex, where the AES
    /// key is the master key hex-decoded (64-character keys) or its UTF-8 bytes
    /// (32-character keys).
    /// </summary>
    public static class ScopedKey
    {
        private const int KeySizeShort = 32;
        private const int KeySizeLong = 64;
        private const int IVHexSize = 32;

        /// <summary>Encrypt an options object (serialized as compact JSON).</summary>
        /// <param name="apiKey">Master key (32 or 64 characters).</param>
        /// <param name="secOptions">Options, e.g. <c>new { filters = ..., allowed_operations = new[] {"read"} }</c>.</param>
        /// <param name="IV">Optional 32-character hex IV; random when omitted.</param>
        public static string Encrypt(string apiKey, object secOptions, string IV = null)
        {
            var json = JObject.FromObject(secOptions ?? new object()).ToString(Formatting.None);
            return EncryptString(apiKey, json, IV);
        }

        /// <summary>Encrypt an options string as-is.</summary>
        public static string EncryptString(string apiKey, string secOptions, string IV = null)
        {
            try
            {
                if (!(null == IV || IV.Length == IVHexSize))
                    throw new IncisiveException(string.Format("Hex-encoded IV must be exactly {0} bytes, got {1}", IVHexSize, IV.Length));

                using (var aes = CreateAes(ConvertKey(apiKey)))
                {
                    if (IV != null)
                        aes.IV = HexToBytes(IV);
                    else
                        aes.GenerateIV();

                    var plain = Encoding.UTF8.GetBytes(secOptions ?? "");
                    using (var enc = aes.CreateEncryptor())
                    {
                        var cipher = enc.TransformFinalBlock(plain, 0, plain.Length);
                        return BytesToHex(aes.IV) + BytesToHex(cipher);
                    }
                }
            }
            catch (Exception e)
            {
                throw new IncisiveException("Encryption error", e);
            }
        }

        /// <summary>Decrypt a scoped key back into its JSON options string.</summary>
        public static string Decrypt(string apiKey, string scopedKey)
        {
            try
            {
                scopedKey = scopedKey ?? "";
                var iv = HexToBytes(scopedKey.Substring(0, IVHexSize));
                var cipher = HexToBytes(scopedKey.Substring(IVHexSize));
                using (var aes = CreateAes(ConvertKey(apiKey)))
                {
                    aes.IV = iv;
                    using (var dec = aes.CreateDecryptor())
                    {
                        var plain = dec.TransformFinalBlock(cipher, 0, cipher.Length);
                        return Encoding.UTF8.GetString(plain);
                    }
                }
            }
            catch (Exception ex)
            {
                throw new IncisiveException("Decryption error", ex);
            }
        }

        private static byte[] ConvertKey(string apiKey)
        {
            if (string.IsNullOrWhiteSpace(apiKey) || (apiKey.Length != KeySizeShort && apiKey.Length != KeySizeLong))
                throw new IncisiveException("API Key must be either 32 or 64 characters");
            return apiKey.Length == KeySizeLong ? HexToBytes(apiKey) : Encoding.UTF8.GetBytes(apiKey);
        }

        private static Aes CreateAes(byte[] key)
        {
            var aes = Aes.Create();
            aes.KeySize = 256;
            aes.Mode = CipherMode.CBC;
            aes.Padding = PaddingMode.PKCS7;
            aes.Key = key;
            return aes;
        }

        private static string BytesToHex(byte[] bytes)
        {
            var sb = new StringBuilder(bytes.Length * 2);
            foreach (var b in bytes)
                sb.Append(b.ToString("X2"));
            return sb.ToString();
        }

        private static byte[] HexToBytes(string hex)
        {
            if (hex.Length % 2 == 1)
                throw new FormatException("Hex string must have an even number of characters");
            var result = new byte[hex.Length / 2];
            for (int i = 0; i < result.Length; i++)
                result[i] = (byte)((Nibble(hex[2 * i]) << 4) | Nibble(hex[2 * i + 1]));
            return result;
        }

        private static int Nibble(char c)
        {
            if (c >= '0' && c <= '9') return c - '0';
            if (c >= 'a' && c <= 'f') return c - 'a' + 10;
            if (c >= 'A' && c <= 'F') return c - 'A' + 10;
            throw new FormatException("Invalid hex character '" + c + "'");
        }
    }
}
