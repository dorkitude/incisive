package incisive

import (
	"bytes"
	"crypto/aes"
	"crypto/cipher"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
)

// ScopedKeyOptions is the common shape of a scoped key's payload. Any
// JSON-encodable value may be passed to EncryptScopedKey instead.
type ScopedKeyOptions struct {
	// AllowedOperations is a subset of "read", "write".
	AllowedOperations []string `json:"allowed_operations,omitempty"`
	// Filters are applied to every query made with the scoped key.
	Filters []Filter `json:"filters,omitempty"`
	// Extra members are merged into the payload.
	Extra map[string]interface{} `json:"-"`
}

// MarshalJSON implements json.Marshaler, merging Extra.
func (o ScopedKeyOptions) MarshalJSON() ([]byte, error) {
	m := map[string]interface{}{}
	for k, v := range o.Extra {
		m[k] = v
	}
	if o.AllowedOperations != nil {
		m["allowed_operations"] = o.AllowedOperations
	}
	if o.Filters != nil {
		m["filters"] = o.Filters
	}
	return json.Marshal(m)
}

// randReader is swapped in tests for deterministic IVs.
var randReader io.Reader = rand.Reader

// EncryptScopedKey derives a scoped key from a master (or other API) key.
//
// The format is hex(IV) + hex(AES-CBC ciphertext of the JSON options):
//   - 64-character keys are hex-decoded into a 32-byte AES-256 key and the
//     plaintext is PKCS#7-padded to 16 bytes;
//   - any other key (the legacy 32-character form) is used as raw bytes,
//     PKCS#7-padded to 32 bytes, and the plaintext is padded to 32 bytes.
//
// In both cases no padding is added when the length is already a multiple of
// the block size, matching the established implementations byte for byte.
func EncryptScopedKey(apiKey string, options interface{}) (string, error) {
	plain, err := json.Marshal(options)
	if err != nil {
		return "", fmt.Errorf("incisive: encoding scoped key options: %w", err)
	}
	return encryptScopedBytes(apiKey, plain)
}

func encryptScopedBytes(apiKey string, plain []byte) (string, error) {
	block, padTo, err := scopedCipher(apiKey)
	if err != nil {
		return "", err
	}
	iv := make([]byte, aes.BlockSize)
	if _, err := io.ReadFull(randReader, iv); err != nil {
		return "", err
	}
	plain = lenientPad(plain, padTo)
	ct := make([]byte, len(plain))
	cipher.NewCBCEncrypter(block, iv).CryptBlocks(ct, plain)
	return hex.EncodeToString(iv) + hex.EncodeToString(ct), nil
}

// DecryptScopedKey recovers the options a scoped key was created with.
func DecryptScopedKey(apiKey, scopedKey string) (map[string]interface{}, error) {
	var out map[string]interface{}
	if err := DecryptScopedKeyInto(apiKey, scopedKey, &out); err != nil {
		return nil, err
	}
	return out, nil
}

// DecryptScopedKeyInto decrypts a scoped key and unmarshals its JSON into v.
func DecryptScopedKeyInto(apiKey, scopedKey string, v interface{}) error {
	plain, err := decryptScoped(apiKey, scopedKey)
	if err != nil {
		return err
	}
	if err := json.Unmarshal(plain, v); err != nil {
		return fmt.Errorf("incisive: scoped key payload is not JSON (wrong key?): %w", err)
	}
	return nil
}

var errBadScopedKey = errors.New("incisive: malformed scoped key")

func decryptScoped(apiKey, scopedKey string) ([]byte, error) {
	block, padTo, err := scopedCipher(apiKey)
	if err != nil {
		return nil, err
	}
	raw, err := hex.DecodeString(scopedKey)
	if err != nil {
		return nil, fmt.Errorf("%w: %v", errBadScopedKey, err)
	}
	if len(raw) < 2*aes.BlockSize || len(raw)%aes.BlockSize != 0 {
		return nil, errBadScopedKey
	}
	iv, ct := raw[:aes.BlockSize], raw[aes.BlockSize:]
	plain := make([]byte, len(ct))
	cipher.NewCBCDecrypter(block, iv).CryptBlocks(plain, ct)
	return lenientUnpad(plain, padTo), nil
}

// scopedCipher builds the AES cipher for apiKey and returns the plaintext
// padding block size for that key format.
func scopedCipher(apiKey string) (cipher.Block, int, error) {
	var key []byte
	padTo := aes.BlockSize
	if len(apiKey) == 64 {
		k, err := hex.DecodeString(apiKey)
		if err != nil {
			return nil, 0, fmt.Errorf("incisive: 64-character API key must be hex: %w", err)
		}
		key = k
	} else {
		padTo = 32
		key = lenientPad([]byte(apiKey), 32)
	}
	block, err := aes.NewCipher(key)
	if err != nil {
		return nil, 0, fmt.Errorf("incisive: unusable API key for scoped keys: %w", err)
	}
	return block, padTo, nil
}

// lenientPad applies PKCS#7 padding to a multiple of size, except that input
// already aligned (including empty input) is returned unchanged.
func lenientPad(b []byte, size int) []byte {
	if len(b)%size == 0 {
		return b
	}
	n := size - len(b)%size
	return append(append([]byte(nil), b...), bytes.Repeat([]byte{byte(n)}, n)...)
}

// lenientUnpad strips trailing PKCS#7-style padding when the last byte is a
// plausible pad length; otherwise the input is returned unchanged.
func lenientUnpad(b []byte, size int) []byte {
	if len(b) == 0 {
		return b
	}
	n := int(b[len(b)-1])
	if n == 0 || n > size || n > len(b) {
		return b
	}
	return b[:len(b)-n]
}
