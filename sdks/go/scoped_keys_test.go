package incisive

import (
	"bytes"
	"encoding/hex"
	"reflect"
	"strings"
	"testing"
)

// Vectors produced by the reference Python implementation (scoped_keys.encrypt
// from the PyPI package, v0.7.0) with random IVs. plain is the exact
// json.dumps() text that was encrypted.
var scopedVectors = []struct {
	key, plain, scoped string
}{
	{
		key:    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
		plain:  `{"filters": [{"property_name": "account.id", "operator": "eq", "property_value": "abc"}], "allowed_operations": ["read"]}`,
		scoped: "446441d912fe9b2d24b4e64331b79aaf9f47c20fae48fb95d39d6ee8f95ccdcca0a1f23ba39efc9c5564c251d2d0db65b7e074ca2cf3a8afbaa2173189ed468ed1f280072e5c62b568f68fd55057fc025ea4c68448dcf98ebb6a612e39fd2cdc7ee9a4d0612c258a829ce2940896e650c88924253a9c8bfb1c20a45a1793aa08fbc46c5de67c5ea83fb0ded73715c048",
	},
	{
		key:    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
		plain:  `{"allowed_operations": ["write"]}`,
		scoped: "031ac1b7e03b39fe68451bd4ac54cf5a7b495a6216ec9d5035ebcdf3fa7246667a5dd29c683c05bd19e772f0c782e57726c1a29559eaea521f5911b3d068c7c4",
	},
	{ // 64-byte plaintext: exact block multiple, so no padding block.
		key:    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
		plain:  `{"allowed_operations": ["read"], "x": "aaaaaaaaaaaaaaaaaaaaaaa"}`,
		scoped: "1ca6e84b827ddb3ecfc11fd2cdb1d524f8e7e00ccbf974c1f7bb58519c195760547b411018c3e8e85f70038136710ec5dd4be9d4bff70b03ceba7d31ef73d7991da235913d41e1815ef1d87e9dcbe893",
	},
	{ // legacy 32-character key
		key:    "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345",
		plain:  `{"filters": [{"property_name": "account.id", "operator": "eq", "property_value": "abc"}], "allowed_operations": ["read"]}`,
		scoped: "dae66160e5d1ee33bd208f3c15ee0881ea3a64af80d3471c01a1ccfd0d7ba562db5b08e79da4df81fb6514c2724e0e4c225a6e1bebde53118bdad879334a3f26411d2e59825ea9fd2047a51737dcb02f51421167368c8d113966c1e8cd22485a2e9abad19dc01170fc60b464e7a408c9b9490b8844791168486aac76d4cefe9cac52abccdc9728a8f019266e844f2283",
	},
	{
		key:    "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345",
		plain:  `{"allowed_operations": ["write"]}`,
		scoped: "76383a8d86056b3c2790356527ed9ea36f0a94c747ed0c393104f4028e715003b846c69dfc10aecf0fb8e5cfd93583eb12a6bf4027bae30c9d82041ee99b15ac7af2cc5eebe775b3a9ee9c266318e0f5",
	},
	{
		key:    "ABCDEFGHIJKLMNOPQRSTUVWXYZ012345",
		plain:  `{"allowed_operations": ["read"], "x": "aaaaaaaaaaaaaaaaaaaaaaa"}`,
		scoped: "0b0cd96c0b7177b8c6c2916d59af6c2fa4e4ff2d6e4c29289f51f1409286074d9e6f1264fb067b91b93ed8cf3a1da9ec0a8f047383626349991a3bac0da720b11bcc6f4f038cf26d3af1a69f54da2c22",
	},
	{ // short legacy key, padded to 32 bytes
		key:    "shortkey",
		plain:  `{"allowed_operations": ["write"]}`,
		scoped: "d87160d175e286e4d8ca2972c38bce3293ac41d29f1b93ab5c5182473abee9a8a3ab66c74e9def9f09212deb7884c9263b97b456cb8bb7f5867233f2a224a1b47c969424ae911dd8ca195f6c7a414d70",
	},
}

func TestDecryptReferenceVectors(t *testing.T) {
	for i, v := range scopedVectors {
		plain, err := decryptScoped(v.key, v.scoped)
		if err != nil {
			t.Fatalf("vector %d: %v", i, err)
		}
		if string(plain) != v.plain {
			t.Fatalf("vector %d: got %q want %q", i, plain, v.plain)
		}
		if _, err := DecryptScopedKey(v.key, v.scoped); err != nil {
			t.Fatalf("vector %d: %v", i, err)
		}
	}
	got, _ := DecryptScopedKey(scopedVectors[0].key, scopedVectors[0].scoped)
	if got["allowed_operations"].([]interface{})[0] != "read" {
		t.Fatalf("decoded %v", got)
	}
}

// Encrypting the same plaintext with the reference IV must reproduce the
// reference ciphertext byte for byte.
func TestEncryptMatchesReferenceBytes(t *testing.T) {
	defer func(r interface{ Read([]byte) (int, error) }) { randReader = r }(randReader)
	for i, v := range scopedVectors {
		iv, _ := hex.DecodeString(v.scoped[:32])
		randReader = bytes.NewReader(iv)
		got, err := encryptScopedBytes(v.key, []byte(v.plain))
		if err != nil {
			t.Fatalf("vector %d: %v", i, err)
		}
		if got != v.scoped {
			t.Fatalf("vector %d mismatch\n got %s\nwant %s", i, got, v.scoped)
		}
	}
}

func TestScopedKeyRoundTrip(t *testing.T) {
	opts := ScopedKeyOptions{
		AllowedOperations: []string{"read"},
		Filters:           []Filter{{PropertyName: "customer.id", Operator: "eq", PropertyValue: "c<&>1"}},
		Extra:             map[string]interface{}{"timeframe": "this_7_days"},
	}
	for _, key := range []string{scopedVectors[0].key, scopedVectors[3].key, "abc"} {
		s, err := EncryptScopedKey(key, opts)
		if err != nil {
			t.Fatal(err)
		}
		if s != strings.ToLower(s) || len(s)%32 != 0 {
			t.Fatalf("unexpected encoding %s", s)
		}
		var back struct {
			AllowedOperations []string `json:"allowed_operations"`
			Filters           []Filter `json:"filters"`
			Timeframe         string   `json:"timeframe"`
		}
		if err := DecryptScopedKeyInto(key, s, &back); err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(back.AllowedOperations, opts.AllowedOperations) || back.Filters[0].PropertyValue != "c<&>1" || back.Timeframe != "this_7_days" {
			t.Fatalf("round trip %+v", back)
		}
	}
}

func TestScopedKeyErrors(t *testing.T) {
	k := scopedVectors[0].key
	if _, err := DecryptScopedKey(k, "zz"); err == nil {
		t.Fatal("want hex error")
	}
	if _, err := DecryptScopedKey(k, strings.Repeat("0", 40)); err == nil {
		t.Fatal("want length error")
	}
	if _, err := DecryptScopedKey(scopedVectors[3].key, scopedVectors[1].scoped); err == nil {
		t.Fatal("want error decrypting with the wrong key")
	}
	if _, err := EncryptScopedKey(strings.Repeat("g", 64), map[string]int{}); err == nil {
		t.Fatal("want non-hex 64-char key error")
	}
	if _, err := EncryptScopedKey(strings.Repeat("a", 40), map[string]int{}); err == nil {
		t.Fatal("want unusable key length error")
	}
}
