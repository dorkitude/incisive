<?php

namespace Incisive\Tests\Unit;

use Incisive\Client\IncisiveClient;
use Incisive\Exception\RuntimeException;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

/**
 * Vectors in tests/fixtures/scoped_key_vectors.json were produced by the
 * reference PHP implementation this client is compatible with.
 */
class ScopedKeyTest extends TestCase
{
    private const LEGACY = 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA';
    private const HEX = 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB';

    /** @return array<string, array{array}> */
    public static function vectors(): array
    {
        $data = json_decode((string) file_get_contents(__DIR__ . '/../fixtures/scoped_key_vectors.json'), true);
        $out = [];
        foreach ($data as $vector) {
            $out[$vector['name']] = [$vector];
        }

        return $out;
    }

    #[DataProvider('vectors')]
    public function testDecryptsReferenceVectors(array $vector): void
    {
        $client = IncisiveClient::factory(['masterKey' => $vector['masterKey']]);
        $expected = ['filters' => $vector['filters']];
        if ($vector['allowed_operations']) {
            $expected['allowed_operations'] = $vector['allowed_operations'];
        }
        $this->assertSame($expected, $client->decryptScopedKey($vector['scopedKey']));
        $this->assertSame($vector['decrypted'], $client->decryptScopedKey($vector['scopedKey']));
    }

    #[DataProvider('vectors')]
    public function testRoundTrip(array $vector): void
    {
        $client = IncisiveClient::factory(['masterKey' => $vector['masterKey']]);
        $key = $client->createScopedKey($vector['filters'], $vector['allowed_operations']);
        $this->assertMatchesRegularExpression('/^[0-9a-f]+$/', $key);
        $this->assertSame($vector['decrypted'], $client->decryptScopedKey($key));
    }

    /**
     * Ciphertext length reveals the padding scheme: legacy keys pad to 32-byte
     * blocks, hex keys to 16-byte blocks.
     */
    public function testBlockSizes(): void
    {
        // {"filters":[]} is 14 bytes.
        $legacy = IncisiveClient::factory(['masterKey' => self::LEGACY])->createScopedKey([], []);
        $hex = IncisiveClient::factory(['masterKey' => self::HEX])->createScopedKey([], []);
        $this->assertSame(32 + 64, strlen($legacy));
        $this->assertSame(32 + 32, strlen($hex));
    }

    public function testUpstreamFixturesFromItsOwnTestSuite(): void
    {
        $filters = [['property_name' => 'id', 'operator' => 'eq', 'property_value' => '123']];
        $expected = ['filters' => $filters, 'allowed_operations' => ['read']];

        $legacy = IncisiveClient::factory(['masterKey' => self::LEGACY]);
        $this->assertSame($expected, $legacy->decryptScopedKey(
            '696a2c05b060e6ed344f7e570c101e0909ff97c338dfe0f1e15c0e5c1ec0621dfcd5577f3ae58596558eed2a43c82d3a06'
            . '2fbf6455810f5c859695c766caddd283398e577b24db014fad896a6f0447a2aad9dad43cef5fa040e8f6d366085423804633'
            . 'ef3b21535b31d11eec24631f83c18e83703247f40136aeba779a840e80013e0969a8cf203295f47da1d70bfeb3'
        ));

        $hex = IncisiveClient::factory(['masterKey' => self::HEX]);
        $this->assertSame($expected, $hex->decryptScopedKey(
            '903441674b0d433ddab759bba82502ec469e00d25c373e35c4d685488bc7779a5abd7d90a03a4cb744ee6a82fa8935804348'
            . 'a5b2351f6527cd5fd6a0613cea5ec4e848f5093e41a53d570cf01066b1f3c3e9b03d4ce0929ff3e6a06e1850fb9e09b65415'
            . 'ac754bbefe9db4b1fcba7d71a9f5f9d9c05cbeffb2a33ef5f4bac131'
        ));
    }

    public function testWrongKeyOrGarbageReturnsNull(): void
    {
        $key = IncisiveClient::factory(['masterKey' => self::HEX])->createScopedKey([], ['read']);
        $other = IncisiveClient::factory(['masterKey' => str_repeat('c', 64)]);
        $this->assertNull($other->decryptScopedKey($key));
        $this->assertNull($other->decryptScopedKey('zz'));
        $this->assertNull($other->decryptScopedKey(''));
    }

    public function testRequiresMasterKey(): void
    {
        $this->expectException(RuntimeException::class);
        IncisiveClient::factory(['readKey' => 'r'])->createScopedKey([], ['read']);
    }

    public function testDecryptRequiresMasterKey(): void
    {
        $this->expectException(RuntimeException::class);
        IncisiveClient::factory()->decryptScopedKey('00');
    }

    public function testRejectsUnsupportedMasterKeyLength(): void
    {
        $this->expectException(RuntimeException::class);
        IncisiveClient::factory(['masterKey' => 'short'])->createScopedKey([], []);
    }
}
