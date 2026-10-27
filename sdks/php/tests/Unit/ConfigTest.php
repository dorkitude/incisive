<?php

namespace Incisive\Tests\Unit;

use Incisive\Client\IncisiveClient;
use PHPUnit\Framework\TestCase;

class ConfigTest extends TestCase
{
    public function testFactoryStoresConfig(): void
    {
        $config = [
            'projectId' => 'p', 'masterKey' => 'm', 'readKey' => 'r', 'writeKey' => 'w', 'version' => '3.0',
            'baseUrl' => 'http://h:1',
        ];
        $client = IncisiveClient::factory($config);
        foreach ($config as $key => $value) {
            $this->assertSame($value, $client->getConfig($key));
        }
        $this->assertSame('p', $client->getProjectId());
        $this->assertSame('http://h:1', $client->getBaseUrl());
    }

    public function testDefaults(): void
    {
        $client = IncisiveClient::factory(['version' => '', 'baseUrl' => '']);
        $this->assertSame('3.0', $client->getVersion());
        $this->assertSame('http://localhost:8080', $client->getBaseUrl());
        $this->assertNull($client->getProjectId());
    }

    public function testSettersAndGetters(): void
    {
        $client = IncisiveClient::factory();
        $client->setProjectId('p');
        $client->setReadKey('r');
        $client->setWriteKey('w');
        $client->setMasterKey('m');
        $client->setVersion('3.0');
        $client->setBaseUrl('http://x');
        $client->setOrganizationId('o');
        $client->setOrganizationKey('ok');
        $this->assertSame(['p', 'r', 'w', 'm', '3.0', 'http://x', 'o', 'ok'], [
            $client->getProjectId(), $client->getReadKey(), $client->getWriteKey(), $client->getMasterKey(),
            $client->getVersion(), $client->getBaseUrl(), $client->getOrganizationId(), $client->getOrganizationKey(),
        ]);
    }

    public function testKeyPickers(): void
    {
        $client = IncisiveClient::factory(['masterKey' => 'foo', 'readKey' => 'r', 'writeKey' => 'w']);
        $this->assertSame('r', $client->getKeyForReading());
        $this->assertSame('w', $client->getKeyForWriting());
        $client->setReadKey('');
        $client->setWriteKey('');
        $this->assertSame('foo', $client->getKeyForReading());
        $this->assertSame('foo', $client->getKeyForWriting());
    }

    public function testFromEnv(): void
    {
        $vars = [
            'INCISIVE_PROJECT_ID' => 'envp', 'INCISIVE_WRITE_KEY' => 'envw', 'INCISIVE_READ_KEY' => 'envr',
            'INCISIVE_MASTER_KEY' => 'envm', 'INCISIVE_BASE_URL' => 'http://env:9',
        ];
        $saved = [];
        foreach ($vars as $k => $v) {
            $saved[$k] = getenv($k);
            putenv("$k=$v");
        }
        try {
            $client = IncisiveClient::fromEnv(['readKey' => 'override']);
            $this->assertSame('envp', $client->getProjectId());
            $this->assertSame('envw', $client->getWriteKey());
            $this->assertSame('override', $client->getReadKey());
            $this->assertSame('envm', $client->getMasterKey());
            $this->assertSame('http://env:9', $client->getBaseUrl());
        } finally {
            foreach ($saved as $k => $v) {
                putenv($v === false ? $k : "$k=$v");
            }
        }
    }

    public function testCleanQueryName(): void
    {
        $this->assertSame('total-API-requests', IncisiveClient::cleanQueryName('total API requests!'));
        $this->assertSame('a_b-c', IncisiveClient::cleanQueryName('a_b-c/?'));
    }
}
