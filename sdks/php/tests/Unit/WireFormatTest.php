<?php

namespace Incisive\Tests\Unit;

use Incisive\Client\IncisiveClient;
use Incisive\Tests\Support\MockServer;
use PHPUnit\Framework\Attributes\DataProvider;
use PHPUnit\Framework\TestCase;

/**
 * Asserts the exact HTTP request produced for every operation against a
 * real local HTTP server.
 */
class WireFormatTest extends TestCase
{
    private const FILTERS = [['property_name' => 'a', 'operator' => 'eq', 'property_value' => 1]];

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

    /**
     * @return array<string, array{0: string, 1: array, 2: string, 3: string, 4: string, 5: ?array}>
     *         method name, call args, HTTP method, raw request URI, Authorization, expected JSON body
     */
    public static function operations(): array
    {
        $f = self::FILTERS;

        return [
            'addEvent escapes collection' => [
                'addEvent', ['my coll/x', ['foo' => 'bar', 'keen' => ['timestamp' => 't']]],
                'POST', '/1.0/projects/PID/events/my%20coll%2Fx', 'WRITE',
                ['foo' => 'bar', 'keen' => ['timestamp' => 't']],
            ],
            'addEvents' => [
                'addEvents', [['c1' => [['a' => 1]], 'c2' => [['b' => 2]]]],
                'POST', '/1.0/projects/PID/events', 'WRITE',
                ['c1' => [['a' => 1]], 'c2' => [['b' => 2]]],
            ],
            'count drops unknown params' => [
                'count', ['my coll/x', [
                    'timeframe' => 'this_week', 'filters' => $f, 'bogus' => 1,
                    'group_by' => ['a', 'b'], 'include_metadata' => true,
                ]],
                'POST', '/1.0/projects/PID/queries/count', 'READ',
                [
                    'event_collection' => 'my coll/x', 'filters' => $f, 'timeframe' => 'this_week',
                    'group_by' => ['a', 'b'], 'include_metadata' => true,
                ],
            ],
            'count array form, absolute timeframe' => [
                'count', [['event_collection' => 'c', 'timeframe' => ['start' => 's', 'end' => 'e']]],
                'POST', '/1.0/projects/PID/queries/count', 'READ',
                ['event_collection' => 'c', 'timeframe' => ['start' => 's', 'end' => 'e']],
            ],
            'countUnique' => [
                'countUnique', ['c', ['target_property' => 't', 'interval' => 'daily', 'timezone' => 'UTC']],
                'POST', '/1.0/projects/PID/queries/count_unique', 'READ',
                ['event_collection' => 'c', 'target_property' => 't', 'interval' => 'daily', 'timezone' => 'UTC'],
            ],
            'minimum' => [
                'minimum', ['c', ['target_property' => 't']],
                'POST', '/1.0/projects/PID/queries/minimum', 'READ', ['event_collection' => 'c', 'target_property' => 't'],
            ],
            'maximum' => [
                'maximum', ['c', ['target_property' => 't']],
                'POST', '/1.0/projects/PID/queries/maximum', 'READ', ['event_collection' => 'c', 'target_property' => 't'],
            ],
            'average' => [
                'average', ['c', ['target_property' => 't']],
                'POST', '/1.0/projects/PID/queries/average', 'READ', ['event_collection' => 'c', 'target_property' => 't'],
            ],
            'sum' => [
                'sum', ['c', ['target_property' => 't']],
                'POST', '/1.0/projects/PID/queries/sum', 'READ', ['event_collection' => 'c', 'target_property' => 't'],
            ],
            'median' => [
                'median', ['c', ['target_property' => 't']],
                'POST', '/1.0/projects/PID/queries/median', 'READ', ['event_collection' => 'c', 'target_property' => 't'],
            ],
            'percentile' => [
                'percentile', ['c', ['target_property' => 't', 'percentile' => 50]],
                'POST', '/1.0/projects/PID/queries/percentile', 'READ',
                ['event_collection' => 'c', 'target_property' => 't', 'percentile' => 50],
            ],
            'selectUnique' => [
                'selectUnique', ['c', ['target_property' => 't']],
                'POST', '/1.0/projects/PID/queries/select_unique', 'READ', ['event_collection' => 'c', 'target_property' => 't'],
            ],
            'multiAnalysis' => [
                'multiAnalysis', ['c', ['analyses' => ['n' => ['analysis_type' => 'count']]]],
                'POST', '/1.0/projects/PID/queries/multi_analysis', 'READ',
                ['event_collection' => 'c', 'analyses' => ['n' => ['analysis_type' => 'count']]],
            ],
            'extraction drops interval' => [
                'extraction', ['c', ['latest' => 10, 'email' => 'x@y', 'property_names' => ['a'], 'interval' => 'daily']],
                'POST', '/1.0/projects/PID/queries/extraction', 'READ',
                ['event_collection' => 'c', 'email' => 'x@y', 'latest' => 10, 'property_names' => ['a']],
            ],
            'funnel' => [
                'funnel', [['steps' => [['event_collection' => 'c', 'actor_property' => 'u']]]],
                'POST', '/1.0/projects/PID/queries/funnel', 'READ',
                ['steps' => [['event_collection' => 'c', 'actor_property' => 'u']]],
            ],
            'funnel ignores collection' => [
                'funnel', ['coll', ['steps' => []]],
                'POST', '/1.0/projects/PID/queries/funnel', 'READ', ['steps' => []],
            ],
            'getCollections' => ['getCollections', [], 'GET', '/1.0/projects/PID/events', 'MASTER', null],
            'getEventSchemas' => ['getEventSchemas', [], 'GET', '/1.0/projects/PID/events', 'MASTER', null],
            'getCollection' => ['getCollection', ['my coll'], 'GET', '/1.0/projects/PID/events/my%20coll', 'MASTER', null],
            'getProperty' => [
                'getProperty', ['my coll', ['property_name' => 'a.b c']],
                'GET', '/1.0/projects/PID/events/my%20coll/properties/a.b%20c', 'MASTER', null,
            ],
            'deleteCollection array form' => [
                'deleteCollection', [['collection_name' => 'my coll']],
                'DELETE', '/1.0/projects/PID/events/my%20coll', 'MASTER', null,
            ],
            'deleteCollection string form' => [
                'deleteCollection', ['my coll'], 'DELETE', '/1.0/projects/PID/events/my%20coll', 'MASTER', null,
            ],
            'deleteEvents with query string' => [
                'deleteEvents', ['my coll', ['filters' => $f, 'timeframe' => ['start' => 's', 'end' => 'e'], 'timezone' => 'UTC']],
                'DELETE',
                '/1.0/projects/PID/events/my%20coll?filters=%5B%7B%22property_name%22%3A%22a%22%2C'
                    . '%22operator%22%3A%22eq%22%2C%22property_value%22%3A1%7D%5D'
                    . '&timeframe=%7B%22start%22%3A%22s%22%2C%22end%22%3A%22e%22%7D&timezone=UTC',
                'MASTER', null,
            ],
            'deleteEvents relative timeframe' => [
                'deleteEvents', ['c', ['timeframe' => 'this_week']],
                'DELETE', '/1.0/projects/PID/events/c?timeframe=this_week', 'MASTER', null,
            ],
            'deleteEventProperties uses master key' => [
                'deleteEventProperties', ['c', ['property_name' => 'p']],
                'DELETE', '/1.0/projects/PID/events/c/properties/p', 'MASTER', null,
            ],
            'getResources hits host root' => ['getResources', [], 'GET', '/', 'MASTER', null],
            'getProjects' => ['getProjects', [], 'GET', '/1.0/projects', 'MASTER', null],
            'getProject' => ['getProject', [], 'GET', '/1.0/projects/PID', 'MASTER', null],
            'getSavedQueries' => ['getSavedQueries', [], 'GET', '/1.0/projects/PID/queries/saved', 'MASTER', null],
            'getSavedQuery keeps raw name' => [
                'getSavedQuery', [['query_name' => 'my query!']],
                'GET', '/1.0/projects/PID/queries/saved/my%20query%21', 'MASTER', null,
            ],
            'createSavedQuery cleans name, sends only query' => [
                'createSavedQuery', [[
                    'query_name' => 'my query!',
                    'query' => ['analysis_type' => 'count', 'event_collection' => 'c'],
                    'refresh_rate' => 5,
                ]],
                'PUT', '/1.0/projects/PID/queries/saved/my-query', 'MASTER',
                ['query' => ['analysis_type' => 'count', 'event_collection' => 'c']],
            ],
            'updateSavedQuery' => [
                'updateSavedQuery', [['query_name' => 'my query!', 'query' => ['analysis_type' => 'count', 'refresh_rate' => 60]]],
                'PUT', '/1.0/projects/PID/queries/saved/my-query', 'MASTER',
                ['query' => ['analysis_type' => 'count', 'refresh_rate' => 60]],
            ],
            'deleteSavedQuery' => [
                'deleteSavedQuery', [['query_name' => 'my query']],
                'DELETE', '/1.0/projects/PID/queries/saved/my%20query', 'MASTER', null,
            ],
            'getSavedQueryResults' => [
                'getSavedQueryResults', ['q'], 'GET', '/1.0/projects/PID/queries/saved/q/result', 'MASTER', null,
            ],
        ];
    }

    #[DataProvider('operations')]
    public function testRequestShape(string $method, array $args, string $httpMethod, string $uri, string $auth, ?array $body): void
    {
        MockServer::enqueue(200, '{"result": 42}');

        $result = $this->client()->$method(...$args);

        $this->assertSame(['result' => 42], $result, 'whole body is returned, not unwrapped');
        $requests = MockServer::requests();
        $this->assertCount(1, $requests);
        $req = $requests[0];
        $this->assertSame($httpMethod, $req['method']);
        $this->assertSame($uri, $req['uri']);
        $this->assertSame($auth, $req['headers']['authorization']);
        $this->assertSame('incisive-php/' . IncisiveClient::VERSION, $req['headers']['user-agent']);
        // No SDK-identification header beyond User-Agent.
        $allowed = ['authorization', 'user-agent', 'content-type', 'content-length', 'host', 'connection'];
        $this->assertSame([], array_diff(array_keys($req['headers']), $allowed));

        if ($body === null) {
            $this->assertSame('', $req['body']);
            $this->assertArrayNotHasKey('content-type', $req['headers']);
        } else {
            $this->assertSame('application/json', $req['headers']['content-type']);
            $this->assertSame($body, json_decode($req['body'], true));
            // Field order follows the operation definition, as upstream does.
            $this->assertSame(array_keys($body), array_keys(json_decode($req['body'], true)));
        }
    }

    public function testMagicCallMatchesExplicitMethod(): void
    {
        $client = $this->client();
        $client->__call('count', ['c', ['timeframe' => 'this_day']]);
        $a = MockServer::lastRequest();
        $client->count('c', ['timeframe' => 'this_day']);
        $b = MockServer::lastRequest();
        $this->assertSame($a['uri'], $b['uri']);
        $this->assertSame($a['body'], $b['body']);
    }

    public function testCommandApi(): void
    {
        $client = $this->client();
        $command = $client->getCommand('sum', ['event_collection' => 'c', 'target_property' => 'p']);
        $this->assertSame(['result' => 1], $client->execute($command));
        $req = MockServer::lastRequest();
        $this->assertSame('/1.0/projects/PID/queries/sum', $req['uri']);
        $this->assertSame(['event_collection' => 'c', 'target_property' => 'p'], json_decode($req['body'], true));
    }

    public function testJsonEncodingMatchesPhpDefaults(): void
    {
        $this->client()->addEvent('c', ['url' => 'http://x/y', 'name' => 'é']);
        // Slashes and non-ASCII characters are escaped, as with json_encode() defaults.
        $expected = '{"url":"http:\\/\\/x\\/y","name":"' . chr(92) . 'u00e9"}';
        $this->assertSame($expected, MockServer::lastRequest()['body']);
    }

    public function testEmptyEventIsAnObject(): void
    {
        $this->client()->addEvent('c');
        $this->assertSame('{}', MockServer::lastRequest()['body']);
    }

    public function testReservedKeysInParamsAreNotSentOrUsed(): void
    {
        $this->client()->count('c', ['projectId' => 'OTHER', 'readKey' => 'nope']);
        $req = MockServer::lastRequest();
        $this->assertSame('/1.0/projects/PID/queries/count', $req['uri']);
        $this->assertSame('READ', $req['headers']['authorization']);
        $this->assertSame(['event_collection' => 'c'], json_decode($req['body'], true));
    }

    public function testMasterKeyFallbackForReadAndWrite(): void
    {
        $client = $this->client(['readKey' => null, 'writeKey' => null]);
        $client->count('c');
        $this->assertSame('MASTER', MockServer::lastRequest()['headers']['authorization']);
        $client->addEvent('c', ['a' => 1]);
        $this->assertSame('MASTER', MockServer::lastRequest()['headers']['authorization']);
    }

    public function testVersionAndBaseUrlShapeThePath(): void
    {
        $client = $this->client(['baseUrl' => MockServer::url() . '/', 'version' => '9.9']);
        $client->getCollections();
        $this->assertSame('/9.9/projects/PID/events', MockServer::lastRequest()['uri']);

        $client->setVersion('1.0');
        $client->setProjectId('P2');
        $client->getCollections();
        $this->assertSame('/1.0/projects/P2/events', MockServer::lastRequest()['uri']);
    }

    public function testExtraHeaders(): void
    {
        $this->client(['headers' => ['X-Trace' => 'abc']])->count('c');
        $this->assertSame('abc', MockServer::lastRequest()['headers']['x-trace']);
    }

    public function testCreateProject(): void
    {
        $client = $this->client(['organizationId' => 'ORG', 'organizationKey' => 'ORGKEY']);
        $client->createProject(['name' => 'proj', 'users' => [['email' => 'a@b']]]);
        $req = MockServer::lastRequest();
        $this->assertSame('POST', $req['method']);
        $this->assertSame('/1.0/organizations/ORG/projects', $req['uri']);
        $this->assertSame('ORGKEY', $req['headers']['authorization']);
        $this->assertSame(['name' => 'proj', 'users' => [['email' => 'a@b']]], json_decode($req['body'], true));
    }
}
