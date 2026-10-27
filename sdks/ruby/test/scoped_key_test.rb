require "test_helper"

# Vectors produced by the reference Ruby implementation (v1.1.1) with
# IV "0123456789abcdef" and the data below.
class ScopedKeyTest < Minitest::Test
  DATA = { "filters" => [{ "property_name" => "user.id", "operator" => "eq", "property_value" => "123" }],
           "allowed_operations" => ["read"] }.freeze
  IV = "0123456789abcdef".freeze

  VECTORS = {
    "f5cdc5f5f9b6c4bc2b7f0b0a1d9a8f2ef5cdc5f5f9b6c4bc2b7f0b0a1d9a8f2e" => {
      fixed: "30313233343536373839616263646566cf938657632b24e471da0eb39ad29bb68e9619b1416eca8bef7c07fc0690491e9369bd3336707eeda97f9776cc5e66cf289e77c8e36858a08703810562b058372a09abde9a880f4d294ebb021f1ef1ddf92cf4285c65286f4b1b139eadf5d9317df5b2c334941add48c69858f5f6c7fc",
      random: "ab2ee65081d2339c0f25a269e141aed59ff721c56900de17e5743329b1f6ea7f2079d9e4a7b8dd43e9107f70392409ef73db149fa895637265ed1f42082cb7c7fb7402f7ca9114c3350f542d4f0ba118637549266bb14d3963c6de49f55a53b943f7378283257bcfa1b47aba6bb20057901ce3897ec257d1b20d7fd3f1571433"
    },
    "abcdefghijklmnopqrstuvwxyz012345" => {
      fixed: "303132333435363738396162636465665d2b9bcb9d0aaabfb645eb561695a8d8f9396c189448790f1dfd35a2e88a70eb3e28301df630cca29a20130062cd7e154872b5177c5dd586f38a9df1e3a4d3ea3f26eee76011fcbffc8b66171d9bc72d8ff0e0c619a3ac470c0fd158e63729ffc9bd026deeeb8b838d8de78c3599cbf6",
      random: "e56345118d47f344e96da8246a916ace9ca692d464ea5d10d5032084f3ea59d69d6b3ba2421e96711bc39f29ceb2f73453ecabe3b47d97cebf89ff589ef7545350a36081c875b12d3daa0ed3a39b22a5404dc1116b462122b726254861ff9e0d87e8d10cfeb0356b2b43001446bbabdb54b16cc9e3b126db8e237565e3c1af8f"
    },
    "shortmasterkey123456" => {
      fixed: "303132333435363738396162636465667ac4507926870fd374fff41cade3f8cf0fc1f8f01c07673a0823ca2e193037cb21e7890471abfb0468acdebedc2d8e1292814026f51cc60f13b66064c28231b3aa6c45061ee0873a68c56ce42604738277aafd2610e6f498a619474bd6470bd0abc691d3ba876a82425156b14aa4fe3c",
      random: "17ebf81642c24c035c8c1996b31d51a5a2fdb23958100e6990fa8c9cb1379e72c3419307f769fbc672ecadd57f9a8b884f06eef67968c1709ea1dd9e27c5490eb8eea33ea78ba34043adb7a979bc5e2a2133c99e9bf0c862cff8d35cc6228fda812e32d77a1677befd06edcfb95e8d34dd889d6f898e7320e3da9cff209cabd7"
    }
  }.freeze

  def test_encrypt_matches_reference_with_fixed_iv
    VECTORS.each do |key, v|
      assert_equal v[:fixed], Incisive::ScopedKey.new(key, DATA).encrypt!(IV), "key length #{key.length}"
    end
  end

  def test_decrypt_reference_vectors
    VECTORS.each do |key, v|
      [v[:fixed], v[:random]].each do |hex|
        sk = Incisive::ScopedKey.decrypt!(key, hex)
        assert_equal DATA, sk.data
        assert_equal key, sk.api_key
      end
    end
  end

  def test_round_trip_random_iv
    VECTORS.each_key do |key|
      a = Incisive::ScopedKey.new(key, { "x" => "ünïcode", "n" => [1, 2] }).encrypt!
      b = Incisive::ScopedKey.new(key, { "x" => "ünïcode", "n" => [1, 2] }).encrypt!
      refute_equal a, b
      assert_match(/\A[0-9a-f]+\z/, a)
      assert_equal({ "x" => "ünïcode", "n" => [1, 2] }, Incisive::ScopedKey.decrypt!(key, a).data)
    end
  end

  def test_bad_iv
    assert_raises(OpenSSL::Cipher::CipherError) { Incisive::ScopedKey.new("k" * 32, {}).encrypt!("short") }
  end

  def test_wrong_key_fails
    hex = VECTORS["abcdefghijklmnopqrstuvwxyz012345"][:fixed]
    assert_raises(OpenSSL::Cipher::CipherError, JSON::ParserError) do
      Incisive::ScopedKey.decrypt!("zyxwvutsrqponmlkjihgfedcba543210", hex)
    end
  end
end
