"""Scoped keys: options JSON encrypted under a master key.

Format (byte-compatible with the established wire format):

* plaintext = ``json.dumps(options)`` (default separators, ASCII)
* 64-char master key: AES-256-CBC, AES key = the 32 bytes the hex string decodes to,
  plaintext padded PKCS#7-style to 16 bytes.
* any other master key ("legacy"): AES-CBC, AES key = the key's ASCII bytes padded
  PKCS#7-style to 32 bytes, plaintext padded PKCS#7-style to a 32-byte block.
* Padding is skipped entirely when the length is already a block multiple, and on
  decrypt it is stripped only if the last byte is <= the block size.
* IV = 16 random bytes; output = hex(IV) + hex(ciphertext), lowercase, as bytes.
"""

import binascii
import json
import os

from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

AES_BLOCK_SIZE = 16
LEGACY_BLOCK_SIZE = 32


def _to_bytes(value):
    return value.encode("utf-8") if isinstance(value, str) else bytes(value)


def _pad(data, block_size):
    if len(data) % block_size == 0:
        return data
    n = block_size - (len(data) % block_size)
    return data + bytes([n]) * n


def _unpad(data, block_size):
    if not data:
        return data
    n = data[-1]
    if n <= block_size and n <= len(data):
        return data[:-n]
    return data


def _is_new_style(api_key):
    return len(api_key) == 64


def _cipher_key(api_key):
    if _is_new_style(api_key):
        return binascii.unhexlify(_to_bytes(api_key))
    return _pad(_to_bytes(api_key), LEGACY_BLOCK_SIZE)


def _block_size(api_key):
    return AES_BLOCK_SIZE if _is_new_style(api_key) else LEGACY_BLOCK_SIZE


def _aes(key, iv):
    return Cipher(algorithms.AES(key), modes.CBC(iv))


def encrypt(api_key, options):
    """Return a scoped key (hex bytes) granting ``options`` under ``api_key``."""
    plaintext = _pad(json.dumps(options).encode("utf-8"), _block_size(api_key))
    iv = os.urandom(AES_BLOCK_SIZE)
    enc = _aes(_cipher_key(api_key), iv).encryptor()
    ciphertext = enc.update(plaintext) + enc.finalize()
    return binascii.hexlify(iv) + binascii.hexlify(ciphertext)


def decrypt(api_key, scoped_key):
    """Return the options dict inside ``scoped_key``. Raises ValueError on a wrong key."""
    raw = _to_bytes(scoped_key)
    try:
        iv = binascii.unhexlify(raw[: 2 * AES_BLOCK_SIZE])
        ciphertext = binascii.unhexlify(raw[2 * AES_BLOCK_SIZE:])
    except binascii.Error as exc:
        raise ValueError("scoped key is not valid hex: {0}".format(exc)) from exc
    if len(iv) != AES_BLOCK_SIZE or not ciphertext or len(ciphertext) % AES_BLOCK_SIZE:
        raise ValueError("scoped key has an invalid length")
    dec = _aes(_cipher_key(api_key), iv).decryptor()
    plaintext = _unpad(dec.update(ciphertext) + dec.finalize(), _block_size(api_key))
    return json.loads(plaintext.decode("utf-8"))
