require 'json'
require 'openssl'
require 'securerandom'

module Incisive
  # Scoped keys: a JSON options object encrypted with the project master key
  # (AES-256-CBC, PKCS#7 padding), serialized as lowercase hex(iv || ciphertext).
  #
  # Key derivation:
  #   * 64-character master key: hex-decoded to 32 raw bytes.
  #   * any other length: the key's bytes, padded up to a multiple of 32 with
  #     N copies of chr(N) (a 32-character key is used as-is).
  #
  # Access keys are the preferred mechanism; scoped keys remain for
  # compatibility with existing deployments.
  class ScopedKey
    attr_accessor :api_key, :data

    def self.decrypt!(api_key, scoped_key)
      new(api_key, JSON.parse(AESHelper.decrypt(api_key, scoped_key)))
    end

    def initialize(api_key, data)
      @api_key = api_key
      @data = data
    end

    # iv: optional 16 raw bytes (random when omitted).
    def encrypt!(iv = nil)
      AESHelper.encrypt(api_key, JSON.generate(data), iv)
    end
  end

  module AESHelper
    BLOCK = 32

    module_function

    def derive_key(api_key)
      return [api_key].pack('H*') if api_key.length == 64
      rem = api_key.bytesize % BLOCK
      return api_key.b if rem.zero?
      pad = BLOCK - rem
      api_key.b + (pad.chr * pad)
    end

    def encrypt(api_key, plaintext, iv = nil)
      if !iv.nil? && iv.bytesize != 16
        raise OpenSSL::Cipher::CipherError, "iv must be 16 bytes"
      end
      cipher = OpenSSL::Cipher.new('aes-256-cbc')
      cipher.encrypt
      cipher.key = derive_key(api_key)
      iv ||= SecureRandom.random_bytes(16)
      cipher.iv = iv
      (iv + cipher.update(plaintext) + cipher.final).unpack1('H*')
    end

    def decrypt(api_key, hex)
      raw = [hex].pack('H*')
      cipher = OpenSSL::Cipher.new('aes-256-cbc')
      cipher.decrypt
      cipher.key = derive_key(api_key)
      cipher.iv = raw[0, 16]
      (cipher.update(raw[16..] || "") + cipher.final).force_encoding('UTF-8')
    end
  end
end
