<?php

namespace Incisive\Tests\Integration;

use Incisive\Client\IncisiveClient;
use Incisive\Exception\CommandClientException;
use PHPUnit\Framework\TestCase;

/**
 * Runs against a real server only when INCISIVE_BASE_URL and
 * INCISIVE_PROJECT_ID are set (plus the keys each test needs).
 */
class LiveServerTest extends TestCase
{
    private function client(array $needs): IncisiveClient
    {
        foreach (array_merge(['INCISIVE_BASE_URL', 'INCISIVE_PROJECT_ID'], $needs) as $var) {
            if ((string) getenv($var) === '') {
                $this->markTestSkipped("$var not set");
            }
        }

        return IncisiveClient::fromEnv();
    }

    public function testAddEventThenCount(): void
    {
        $client = $this->client(['INCISIVE_WRITE_KEY', 'INCISIVE_READ_KEY']);
        $collection = 'php_sdk_it_' . bin2hex(random_bytes(4));
        $marker = bin2hex(random_bytes(6));

        $created = $client->addEvent($collection, ['marker' => $marker, 'price' => 5]);
        $this->assertIsArray($created);

        $batch = $client->addEvents([$collection => [['marker' => $marker, 'price' => 7]]]);
        $this->assertIsArray($batch);
        $this->assertArrayHasKey($collection, $batch);

        $filters = [['property_name' => 'marker', 'operator' => 'eq', 'property_value' => $marker]];
        $count = null;
        $sum = null;
        $deadline = time() + 30;
        do {
            $count = $client->count($collection, ['filters' => $filters, 'timeframe' => 'this_1_days']);
            if (($count['result'] ?? 0) >= 2) {
                break;
            }
            usleep(500000);
        } while (time() < $deadline);
        $this->assertSame(2, $count['result']);

        $sum = $client->sum($collection, ['target_property' => 'price', 'filters' => $filters, 'timeframe' => 'this_1_days']);
        $this->assertEquals(12, $sum['result']);

        $unique = $client->selectUnique($collection, ['target_property' => 'price', 'timeframe' => 'this_1_days']);
        $this->assertEqualsCanonicalizing([5, 7], $unique['result']);
    }

    public function testBadKeyIsRejected(): void
    {
        $client = $this->client([]);
        $client->setReadKey('definitely-not-a-valid-key');
        $this->expectException(CommandClientException::class);
        $client->count('anything', ['timeframe' => 'this_1_days']);
    }

    public function testScopedKeyCanQuery(): void
    {
        $client = $this->client(['INCISIVE_MASTER_KEY']);
        $scoped = $client->createScopedKey(
            [['property_name' => 'marker', 'operator' => 'exists', 'property_value' => true]],
            ['read']
        );
        $reader = IncisiveClient::fromEnv(['readKey' => $scoped]);
        $result = $reader->count('php_sdk_it_scoped', ['timeframe' => 'this_1_days']);
        $this->assertArrayHasKey('result', $result);
    }
}
