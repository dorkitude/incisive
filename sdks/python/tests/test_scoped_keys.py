"""Vectors in vectors.json were produced by the reference implementation of the format."""

import json
import pathlib

import pytest

from incisive import scoped_keys

VECTORS = json.loads((pathlib.Path(__file__).parent / "vectors.json").read_text())
NEW_KEY = "24077ACBCB198BAAA2110EDDB673282F8E34909FD823A15C55A6253A664BE368"
OLD_KEY = "ab428324dbdbcfe744"


@pytest.mark.parametrize("vec", VECTORS, ids=lambda v: "{0}-{1}".format(len(v["key"]), v["json_len"]))
def test_decrypts_reference_vectors(vec):
    assert scoped_keys.decrypt(vec["key"], vec["scoped_key"]) == vec["options"]
    assert scoped_keys.decrypt(vec["key"], vec["scoped_key"].encode()) == vec["options"]


@pytest.mark.parametrize("vec", VECTORS, ids=lambda v: "{0}-{1}".format(len(v["key"]), v["json_len"]))
def test_ciphertext_length_matches_reference(vec):
    mine = scoped_keys.encrypt(vec["key"], vec["options"])
    assert isinstance(mine, bytes)
    assert len(mine) == len(vec["scoped_key"])
    assert mine != vec["scoped_key"].encode()  # random IV
    assert scoped_keys.decrypt(vec["key"], mine) == vec["options"]


@pytest.mark.parametrize("key", [NEW_KEY, NEW_KEY.lower(), OLD_KEY, "0123456789abcdef" * 2])
def test_round_trip(key):
    opts = {"filters": [{"property_name": "accountId", "operator": "eq",
                         "property_value": "123456"}], "allowed_operations": ["read"]}
    assert scoped_keys.decrypt(key, scoped_keys.encrypt(key, opts)) == opts


def test_reference_format_with_fixed_iv(monkeypatch):
    # Same IV -> identical bytes: pin the exact layout hex(iv) + hex(ct).
    vec = VECTORS[0]
    iv = bytes.fromhex(vec["scoped_key"][:32])
    monkeypatch.setattr(scoped_keys.os, "urandom", lambda n: iv)
    assert scoped_keys.encrypt(vec["key"], vec["options"]).decode() == vec["scoped_key"]


@pytest.mark.parametrize("key,bad", [(NEW_KEY, NEW_KEY[:-1] + "9"), (OLD_KEY, "badbadbadbad")])
def test_wrong_key_raises_value_error(key, bad):
    enc = scoped_keys.encrypt(key, {"filters": [{"property_name": "a", "operator": "eq",
                                                  "property_value": "x" * 40}]})
    with pytest.raises(ValueError):
        scoped_keys.decrypt(bad, enc)


def test_garbage_raises_value_error():
    with pytest.raises(ValueError):
        scoped_keys.decrypt(NEW_KEY, "zz")
    with pytest.raises(ValueError):
        scoped_keys.decrypt(NEW_KEY, "00" * 16)
