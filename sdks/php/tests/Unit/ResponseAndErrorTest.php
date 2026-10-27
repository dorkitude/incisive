<?php

namespace Incisive\Tests\Unit;

use Incisive\Client\IncisiveClient;
use Incisive\Client\Response;
use Incisive\Exception\CommandClientException;
use Incisive\Exception\CommandException;
use Incisive\Exception\CommandServerException;
use Incisive\Exception\ExceptionInterface;
use Incisive\Exception\RequestException;
use Incisive\Exception\TransferException;
use Incisive\Tests\Support\MockServer;
use PHPUnit\Framework\TestCase;

class ResponseAndErrorTest extends TestCase
{
    protected function setUp(): void
    {
        MockServer::reset();
    }

    private function client(array $overrides = []): IncisiveClient
    {
        return IncisiveClient::factory(array_merge([
            'baseUrl' => MockServer::url(),
            'projectId' => 'PID',
            'masterKey' => 'MASTER',
            'writeKey' => 'WRITE',
            'readKey' => 'READ',
        ], $overrides));
    }

    public function testResultIsNotUnwrapped(): void
    {
        MockServer::enqueue(200, '{"result": [{"a": 1, "result": 3}], "metadata": {"x": true}}');
        $this->assertSame(
            ['result' => [['a' => 1, 'result' => 3]], 'metadata' => ['x' => true]],
            $this->client()->count('c', ['group_by' => 'a', 'include_metadata' => true])
        );
    }

    public function testBatchInsertResponse(): void
    {
        MockServer::enqueue(200, '{"test":[{"success":true},{"success":true}]}');
        $this->assertSame(
            ['test' => [['success' => true], ['success' => true]]],
            $this->client()->addEvents(['test' => [['foo' => 'bar'], ['bar' => 'baz']]])
        );
    }

    public function testEmptyAndNonJsonBodiesReturnNull(): void
    {
        MockServer::enqueue(204, '');
        $this->assertNull($this->client()->deleteCollection('c'));
        MockServer::enqueue(200, 'not json', ['Content-Type' => 'text/plain']);
        $this->assertNull($this->client()->count('c'));
    }

    public function testClientErrorCarriesStatusMessageAndErrorCode(): void
    {
        $body = '{"message":"Property name is invalid.","error_code":"InvalidPropertyNameError"}';
        MockServer::enqueue(400, $body);
        try {
            $this->client()->addEvent('c', ['bad.name' => 1]);
            $this->fail('expected exception');
        } catch (CommandClientException $e) {
            $this->assertInstanceOf(RequestException::class, $e);
            $this->assertInstanceOf(CommandException::class, $e);
            $this->assertInstanceOf(ExceptionInterface::class, $e);
            $this->assertSame(400, $e->getStatusCode());
            $this->assertSame(400, $e->getCode());
            $this->assertSame('InvalidPropertyNameError', $e->getErrorCode());
            $this->assertSame('Property name is invalid.', $e->getErrorMessage());
            $this->assertSame('addEvent', $e->getCommandName());
            $this->assertSame($body, $e->getResponse()->getBody());
            $this->assertStringContainsString('There was an error executing the addEvent command', $e->getMessage());
            $this->assertStringContainsString('`400 Bad Request`', $e->getMessage());
            $this->assertStringContainsString($body, $e->getMessage());
            $this->assertStringStartsWith('HTTP/1.1 400', (string) $e->getResponse());
        }
    }

    /**
     * @return array<string, array{int}>
     */
    public static function clientStatuses(): array
    {
        return ['401' => [401], '403' => [403], '404' => [404], '429' => [429]];
    }

    #[\PHPUnit\Framework\Attributes\DataProvider('clientStatuses')]
    public function testFourHundredsRaiseClientException(int $status): void
    {
        MockServer::enqueue($status, '{"message":"nope","error_code":"InvalidApiKeyError"}');
        $this->expectException(CommandClientException::class);
        $this->expectExceptionCode($status);
        $this->client()->count('c');
    }

    public function testServerErrorWithNonJsonBody(): void
    {
        MockServer::enqueue(500, 'oops', ['Content-Type' => 'text/plain']);
        try {
            $this->client()->count('c');
            $this->fail('expected exception');
        } catch (CommandServerException $e) {
            $this->assertSame(500, $e->getStatusCode());
            $this->assertNull($e->getErrorCode());
            $this->assertNull($e->getErrorMessage());
            $this->assertStringContainsString('Server error', $e->getMessage());
        }
    }

    public function testMissingRequiredParameterFailsBeforeSending(): void
    {
        try {
            $this->client()->countUnique('c', []);
            $this->fail('expected exception');
        } catch (CommandException $e) {
            $this->assertNotInstanceOf(RequestException::class, $e);
            $this->assertStringContainsString('[target_property] is required', $e->getMessage());
        }
        $this->assertSame([], MockServer::requests());
    }

    public function testMissingCollectionFailsBeforeSending(): void
    {
        $this->expectException(CommandException::class);
        $this->expectExceptionMessage('[event_collection] is required');
        $this->client()->count([]);
    }

    public function testMissingKeyFailsBeforeSending(): void
    {
        $client = $this->client(['readKey' => null, 'writeKey' => null, 'masterKey' => null]);
        try {
            $client->count('c');
            $this->fail('expected exception');
        } catch (CommandException $e) {
            $this->assertStringContainsString('[readKey]', $e->getMessage());
        }
        try {
            $client->getCollections();
            $this->fail('expected exception');
        } catch (CommandException $e) {
            $this->assertStringContainsString('[masterKey]', $e->getMessage());
        }
        $this->assertSame([], MockServer::requests());
    }

    public function testUnknownOperation(): void
    {
        $this->expectException(\BadMethodCallException::class);
        $this->client()->standardDeviation('c');
    }

    public function testConnectionFailureRaisesTransferException(): void
    {
        $probe = stream_socket_server('tcp://127.0.0.1:0');
        $name = stream_socket_get_name($probe, false);
        fclose($probe);
        $client = $this->client(['baseUrl' => 'http://' . $name, 'timeout' => 2]);
        $this->expectException(TransferException::class);
        $client->count('c');
    }

    public function testCustomHandler(): void
    {
        $seen = null;
        $client = $this->client([
            'baseUrl' => 'http://example.invalid',
            'handler' => function ($method, $url, $headers, $body, $options) use (&$seen) {
                $seen = compact('method', 'url', 'headers', 'body');

                return new Response(200, ['Content-Type' => ['application/json']], '{"result": 7}');
            },
        ]);
        $this->assertSame(['result' => 7], $client->count('c'));
        $this->assertSame('POST', $seen['method']);
        $this->assertSame('http://example.invalid/1.0/projects/PID/queries/count', $seen['url']);
        $this->assertSame('READ', $seen['headers']['Authorization']);
        $this->assertSame('{"event_collection":"c"}', $seen['body']);
    }
}
