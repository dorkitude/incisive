<?php

namespace Incisive\Client;

use Incisive\Client\Filter\MultiTypeFiltering;
use Incisive\Exception\CommandClientException;
use Incisive\Exception\CommandException;
use Incisive\Exception\CommandServerException;
use Incisive\Exception\RuntimeException;
use Incisive\Exception\TransferException;

/**
 * Client for the Incisive analytics HTTP API (v1.0 routes).
 *
 * Query-style methods accept either a collection name plus an argument array,
 * or a single argument array containing `event_collection`:
 *
 *     $client->count('purchases', ['timeframe' => 'this_14_days']);
 *     $client->count(['event_collection' => 'purchases', 'timeframe' => 'this_14_days']);
 *
 * Every call returns the decoded JSON response body as an associative array
 * (null for an empty or non-JSON body); nothing is unwrapped.
 */
class IncisiveClient
{
    public const VERSION = '0.1.0';

    public const DEFAULT_BASE_URL = 'http://localhost:8080';

    public const DEFAULT_API_VERSION = '1.0';

    /** Configuration keys that are never sent as request parameters. */
    private const RESERVED = ['projectId', 'masterKey', 'writeKey', 'readKey', 'organizationId', 'organizationKey'];

    /** Which parameter a lone string argument names, for operations that are not collection-scoped. */
    private const STRING_ARG = [
        'deleteCollection' => 'collection_name',
        'getSavedQuery' => 'query_name',
        'createSavedQuery' => 'query_name',
        'updateSavedQuery' => 'query_name',
        'deleteSavedQuery' => 'query_name',
        'getSavedQueryResults' => 'query_name',
    ];

    /** @var array<string, mixed> */
    private $config;

    /** @var callable|null */
    private $handler;

    /**
     * @param array<string, mixed> $config see factory()
     */
    public function __construct(array $config = [])
    {
        $defaults = [
            'projectId' => null,
            'writeKey' => null,
            'readKey' => null,
            'masterKey' => null,
            'organizationId' => null,
            'organizationKey' => null,
            'baseUrl' => self::DEFAULT_BASE_URL,
            'version' => self::DEFAULT_API_VERSION,
            'timeout' => 60,
            'headers' => [],
            'handler' => null,
        ];
        foreach ($defaults as $key => $value) {
            if (!isset($config[$key]) || $config[$key] === '') {
                $config[$key] = $value;
            }
        }
        $this->handler = $config['handler'];
        unset($config['handler']);
        $this->config = $config;
    }

    /**
     * Create a client.
     *
     * Recognised keys: projectId, writeKey, readKey, masterKey, organizationId,
     * organizationKey, baseUrl (default http://localhost:8080), version
     * (default "1.0"), timeout (seconds, default 60), headers (extra request
     * headers), handler (callable(string $method, string $url, array $headers,
     * ?string $body, array $options): Response, replaces the HTTP transport).
     *
     * @param array<string, mixed> $config
     */
    public static function factory(array $config = []): self
    {
        return new self($config);
    }

    /**
     * Create a client from INCISIVE_PROJECT_ID, INCISIVE_WRITE_KEY,
     * INCISIVE_READ_KEY, INCISIVE_MASTER_KEY and INCISIVE_BASE_URL.
     * Entries in $config win over the environment.
     *
     * @param array<string, mixed> $config
     */
    public static function fromEnv(array $config = []): self
    {
        $map = [
            'projectId' => 'INCISIVE_PROJECT_ID',
            'writeKey' => 'INCISIVE_WRITE_KEY',
            'readKey' => 'INCISIVE_READ_KEY',
            'masterKey' => 'INCISIVE_MASTER_KEY',
            'baseUrl' => 'INCISIVE_BASE_URL',
        ];
        foreach ($map as $key => $env) {
            $value = getenv($env);
            if (!isset($config[$key]) && $value !== false && $value !== '') {
                $config[$key] = $value;
            }
        }

        return new self($config);
    }

    // ------------------------------------------------------------------
    // Configuration
    // ------------------------------------------------------------------

    /**
     * @return mixed the whole config array when $key is null
     */
    public function getConfig(?string $key = null)
    {
        if ($key === null) {
            return $this->config;
        }

        return $this->config[$key] ?? null;
    }

    /**
     * @param mixed $value
     */
    public function setConfig(string $key, $value): void
    {
        $this->config[$key] = $value;
    }

    public function setProjectId($projectId): void
    {
        $this->setConfig('projectId', $projectId);
    }

    public function getProjectId()
    {
        return $this->getConfig('projectId');
    }

    public function setOrganizationId($organizationId): void
    {
        $this->setConfig('organizationId', $organizationId);
    }

    public function getOrganizationId()
    {
        return $this->getConfig('organizationId');
    }

    public function setOrganizationKey($organizationKey): void
    {
        $this->setConfig('organizationKey', $organizationKey);
    }

    public function getOrganizationKey()
    {
        return $this->getConfig('organizationKey');
    }

    public function setWriteKey($writeKey): void
    {
        $this->setConfig('writeKey', $writeKey);
    }

    public function getWriteKey()
    {
        return $this->getConfig('writeKey');
    }

    public function setReadKey($readKey): void
    {
        $this->setConfig('readKey', $readKey);
    }

    public function getReadKey()
    {
        return $this->getConfig('readKey');
    }

    public function setMasterKey($masterKey): void
    {
        $this->setConfig('masterKey', $masterKey);
    }

    public function getMasterKey()
    {
        return $this->getConfig('masterKey');
    }

    public function setVersion($version): void
    {
        $this->setConfig('version', $version);
    }

    public function getVersion()
    {
        return $this->getConfig('version');
    }

    public function setBaseUrl($baseUrl): void
    {
        $this->setConfig('baseUrl', $baseUrl);
    }

    public function getBaseUrl()
    {
        return $this->getConfig('baseUrl');
    }

    /** The write key, or the master key when no write key is set. */
    public function getKeyForWriting()
    {
        return $this->getWriteKey() ?: $this->getMasterKey();
    }

    /** The read key, or the master key when no read key is set. */
    public function getKeyForReading()
    {
        return $this->getReadKey() ?: $this->getMasterKey();
    }

    // ------------------------------------------------------------------
    // Event ingestion
    // ------------------------------------------------------------------

    /**
     * Record a single event. POST /projects/{id}/events/{collection}, body = the event.
     *
     * @param array<string, mixed> $event
     * @return mixed
     */
    public function addEvent($collection, array $event = [])
    {
        return $this->execute(new Command('addEvent', ['event_collection' => $collection] + ['__body' => $event]));
    }

    /**
     * Record events in several collections at once.
     * POST /projects/{id}/events, body = {"collection": [event, ...], ...}.
     *
     * @param array<string, array<int, array<string, mixed>>> $events
     * @return mixed
     */
    public function addEvents(array $events = [])
    {
        return $this->execute(new Command('addEvents', ['__body' => $events]));
    }

    // ------------------------------------------------------------------
    // Analyses (POST /projects/{id}/queries/<analysis>, read key)
    // ------------------------------------------------------------------

    /** @return mixed */
    public function count($eventCollection = [], array $args = [])
    {
        return $this->call('count', func_get_args());
    }

    /** @return mixed */
    public function countUnique($eventCollection = [], array $args = [])
    {
        return $this->call('countUnique', func_get_args());
    }

    /** @return mixed */
    public function minimum($eventCollection = [], array $args = [])
    {
        return $this->call('minimum', func_get_args());
    }

    /** @return mixed */
    public function maximum($eventCollection = [], array $args = [])
    {
        return $this->call('maximum', func_get_args());
    }

    /** @return mixed */
    public function average($eventCollection = [], array $args = [])
    {
        return $this->call('average', func_get_args());
    }

    /** @return mixed */
    public function sum($eventCollection = [], array $args = [])
    {
        return $this->call('sum', func_get_args());
    }

    /** @return mixed */
    public function median($eventCollection = [], array $args = [])
    {
        return $this->call('median', func_get_args());
    }

    /** @return mixed */
    public function percentile($eventCollection = [], array $args = [])
    {
        return $this->call('percentile', func_get_args());
    }

    /** @return mixed */
    public function selectUnique($eventCollection = [], array $args = [])
    {
        return $this->call('selectUnique', func_get_args());
    }

    /** @return mixed */
    public function multiAnalysis($eventCollection = [], array $args = [])
    {
        return $this->call('multiAnalysis', func_get_args());
    }

    /**
     * Raw or emailed extraction (`latest`, `email`, `property_names` supported).
     *
     * @return mixed
     */
    public function extraction($eventCollection = [], array $args = [])
    {
        return $this->call('extraction', func_get_args());
    }

    /**
     * Funnel over `steps`; a collection name given here is not sent (each step
     * carries its own event_collection).
     *
     * @return mixed
     */
    public function funnel($eventCollection = [], array $args = [])
    {
        return $this->call('funnel', func_get_args());
    }

    // ------------------------------------------------------------------
    // Collections, properties and deletes (master key unless noted)
    // ------------------------------------------------------------------

    /** @return mixed */
    public function getCollections(array $args = [])
    {
        return $this->call('getCollections', func_get_args());
    }

    /** Alias of getCollections(). @return mixed */
    public function getEventSchemas(array $args = [])
    {
        return $this->call('getEventSchemas', func_get_args());
    }

    /** @return mixed */
    public function getCollection($eventCollection = [], array $args = [])
    {
        return $this->call('getCollection', func_get_args());
    }

    /**
     * @param string|array $collectionName name, or ['collection_name' => name]
     * @return mixed
     */
    public function deleteCollection($collectionName = [], array $args = [])
    {
        return $this->call('deleteCollection', func_get_args());
    }

    /** getProperty('collection', ['property_name' => 'x']). @return mixed */
    public function getProperty($eventCollection = [], array $args = [])
    {
        return $this->call('getProperty', func_get_args());
    }

    /**
     * Delete events matching `filters`/`timeframe`/`timezone` (sent in the query string).
     *
     * @return mixed
     */
    public function deleteEvents($eventCollection = [], array $args = [])
    {
        return $this->call('deleteEvents', func_get_args());
    }

    /** deleteEventProperties('collection', ['property_name' => 'x']); uses the write key. @return mixed */
    public function deleteEventProperties($eventCollection = [], array $args = [])
    {
        return $this->call('deleteEventProperties', func_get_args());
    }

    // ------------------------------------------------------------------
    // Saved / cached queries (master key)
    // ------------------------------------------------------------------

    /** @return mixed */
    public function getSavedQueries(array $args = [])
    {
        return $this->call('getSavedQueries', func_get_args());
    }

    /** @param string|array $queryName @return mixed */
    public function getSavedQuery($queryName = [], array $args = [])
    {
        return $this->call('getSavedQuery', func_get_args());
    }

    /**
     * createSavedQuery(['query_name' => 'n', 'query' => [...]]). Only `query` is
     * sent; put `refresh_rate` inside it to make the query cached. The name is
     * normalised with cleanQueryName().
     *
     * @param string|array $queryName
     * @return mixed
     */
    public function createSavedQuery($queryName = [], array $args = [])
    {
        return $this->call('createSavedQuery', func_get_args());
    }

    /** @param string|array $queryName @return mixed */
    public function updateSavedQuery($queryName = [], array $args = [])
    {
        return $this->call('updateSavedQuery', func_get_args());
    }

    /** @param string|array $queryName @return mixed */
    public function deleteSavedQuery($queryName = [], array $args = [])
    {
        return $this->call('deleteSavedQuery', func_get_args());
    }

    /** @param string|array $queryName @return mixed */
    public function getSavedQueryResults($queryName = [], array $args = [])
    {
        return $this->call('getSavedQueryResults', func_get_args());
    }

    // ------------------------------------------------------------------
    // Projects and resources
    // ------------------------------------------------------------------

    /** GET {baseUrl}/ (host root). @return mixed */
    public function getResources(array $args = [])
    {
        return $this->call('getResources', func_get_args());
    }

    /** @return mixed */
    public function getProjects(array $args = [])
    {
        return $this->call('getProjects', func_get_args());
    }

    /** @return mixed */
    public function getProject(array $args = [])
    {
        return $this->call('getProject', func_get_args());
    }

    /**
     * POST /organizations/{organizationId}/projects with the organization key;
     * $args become the JSON body.
     *
     * @return mixed
     */
    public function createProject(array $args = [])
    {
        return $this->call('createProject', func_get_args());
    }

    // ------------------------------------------------------------------
    // Command API
    // ------------------------------------------------------------------

    /**
     * Any operation in Operations::all() can also be invoked by name.
     *
     * @param mixed[] $args
     * @return mixed
     */
    public function __call(string $method, array $args)
    {
        if (Operations::get($method) === null) {
            throw new \BadMethodCallException(sprintf('Unknown operation "%s"', $method));
        }

        return $this->call($method, $args);
    }

    /**
     * @param array<string, mixed> $params
     */
    public function getCommand(string $name, array $params = []): Command
    {
        if (Operations::get($name) === null) {
            throw new CommandException(sprintf('No operation found named %s', $name), $name);
        }

        return new Command($name, $params);
    }

    /**
     * Run a command and return the decoded JSON body.
     *
     * @return mixed
     * @throws CommandException        on validation or transport failure
     * @throws CommandClientException  on HTTP 4xx
     * @throws CommandServerException  on HTTP 5xx
     */
    public function execute(Command $command)
    {
        $name = $command->getName();
        $request = $this->buildRequest($command);
        $response = $this->send($name, $request['method'], $request['url'], $request['headers'], $request['body']);

        $status = $response->getStatusCode();
        if ($status >= 400) {
            $kind = $status >= 500 ? 'Server' : 'Client';
            $message = sprintf(
                'There was an error executing the %s command: %s error: `%s %s` resulted in a `%d %s` response:' . "\n%s",
                $name,
                $kind,
                $request['method'],
                $request['url'],
                $status,
                $response->getReasonPhrase(),
                self::summarize($response->getBody())
            );
            if ($status >= 500) {
                throw new CommandServerException($message, $name, $response);
            }
            throw new CommandClientException($message, $name, $response);
        }

        return json_decode($response->getBody(), true);
    }

    /**
     * Build (but do not send) the HTTP request for a command.
     *
     * @return array{method: string, url: string, headers: array<string, string>, body: ?string}
     */
    public function buildRequest(Command $command): array
    {
        $name = $command->getName();
        $op = Operations::get($name);
        if ($op === null) {
            throw new CommandException(sprintf('No operation found named %s', $name), $name);
        }

        $params = $command->toArray();
        $explicitBody = array_key_exists('__body', $params) ? $params['__body'] : null;
        unset($params['__body']);
        foreach (self::RESERVED as $reserved) {
            unset($params[$reserved]);
        }
        foreach ($op['clean'] ?? [] as $cleaned) {
            if (isset($params[$cleaned]) && is_string($params[$cleaned])) {
                $params[$cleaned] = self::cleanQueryName($params[$cleaned]);
            }
        }

        // Path placeholders may come from parameters or from configuration.
        $context = $params + [
            'projectId' => $this->getProjectId(),
            'organizationId' => $this->getOrganizationId(),
        ];

        $errors = [];
        foreach ($op['required'] ?? [] as $required) {
            if (!isset($context[$required]) || $context[$required] === '') {
                $errors[] = sprintf('[%s] is required', $required);
            }
        }
        $key = $this->keyFor($op['auth']);
        if ($key === null || $key === '') {
            $errors[] = sprintf('[%s] must be of type string', self::keyName($op['auth']));
        }
        if ($errors) {
            throw new CommandException('Validation errors: ' . implode("\n", $errors), $name);
        }

        // Path
        $pathParams = [];
        $path = preg_replace_callback('/\{(\w+)\}/', function ($m) use ($context, &$pathParams) {
            $pathParams[] = $m[1];

            return rawurlencode((string) ($context[$m[1]] ?? ''));
        }, $op['path']);

        $base = rtrim((string) $this->getBaseUrl(), '/');
        if ($path[0] === '/') {
            $url = $base . $path;
        } else {
            $url = $base . '/' . trim((string) $this->getVersion(), '/') . '/' . $path;
        }

        // Query string
        $query = [];
        foreach ($op['query'] ?? [] as $field) {
            if (!isset($params[$field])) {
                continue;
            }
            $value = $params[$field];
            // filters and absolute timeframes travel as JSON, the documented encoding.
            // (Bracket-style `filters[0][property_name]=` is not understood by every server
            // and a server that ignores it would delete the whole collection.)
            if ($field === 'filters') {
                $query[$field] = (string) json_encode($value);
            } else {
                $query[$field] = $field === 'timeframe' ? MultiTypeFiltering::encodeValue($value) : $value;
            }
        }
        if ($query) {
            $url .= '?' . http_build_query($query, '', '&', PHP_QUERY_RFC3986);
        }

        // Body
        $body = null;
        if ($explicitBody !== null) {
            $body = self::encodeObject($explicitBody);
        } elseif (($op['body'] ?? null) === 'params') {
            $body = self::encodeObject(array_diff_key($params, array_flip($pathParams)));
        } elseif (isset($op['json'])) {
            $fields = [];
            foreach ($op['json'] as $field) {
                if (isset($params[$field])) {
                    $fields[$field] = $params[$field];
                }
            }
            $body = self::encodeObject($fields);
        }

        $headers = [];
        foreach ((array) $this->getConfig('headers') as $hName => $hValue) {
            $headers[$hName] = (string) $hValue;
        }
        $headers['User-Agent'] = 'incisive-php/' . self::VERSION;
        $headers['Authorization'] = (string) $key;
        if ($body !== null) {
            $headers['Content-Type'] = 'application/json';
        }

        return ['method' => $op['method'], 'url' => $url, 'headers' => $headers, 'body' => $body];
    }

    // ------------------------------------------------------------------
    // Scoped keys
    // ------------------------------------------------------------------

    /**
     * Encrypt filters (and optional allowed operations) into a scoped key
     * with the master key: AES-256-CBC, random IV, hex(IV) . hex(ciphertext).
     *
     * A 64-hex-character master key is hex-decoded into the 32-byte AES key and
     * standard PKCS#7 padding is used. A 32-character (legacy) master key is
     * used verbatim as the AES key and the plaintext is PKCS#7-padded to a
     * 32-byte block size.
     *
     * @param array $filters
     * @param array $allowedOperations
     * @throws RuntimeException if no master key is set
     */
    public function createScopedKey($filters, $allowedOperations = []): string
    {
        $masterKey = $this->getMasterKey();
        if (!$masterKey) {
            throw new RuntimeException('A master key is needed to create a scoped key');
        }

        $options = ['filters' => $filters];
        if (!empty($allowedOperations)) {
            $options['allowed_operations'] = $allowedOperations;
        }
        $plaintext = json_encode($options);

        [$aesKey, $legacy] = self::scopedKeyMaterial((string) $masterKey);
        $flags = OPENSSL_RAW_DATA;
        if ($legacy) {
            $plaintext = self::pkcs7Pad($plaintext, 32);
            $flags |= OPENSSL_ZERO_PADDING;
        }

        $iv = random_bytes(openssl_cipher_iv_length('aes-256-cbc'));
        $ciphertext = openssl_encrypt($plaintext, 'aes-256-cbc', $aesKey, $flags, $iv);
        if ($ciphertext === false) {
            throw new RuntimeException('Scoped key encryption failed');
        }

        return bin2hex($iv) . bin2hex($ciphertext);
    }

    /**
     * Decrypt a scoped key produced by createScopedKey() (or any compatible
     * implementation). Returns the decoded options array, or null when the key
     * cannot be decrypted.
     *
     * @return mixed
     * @throws RuntimeException if no master key is set
     */
    public function decryptScopedKey($scopedKey)
    {
        $masterKey = $this->getMasterKey();
        if (!$masterKey) {
            throw new RuntimeException('A master key is needed to decrypt a scoped key');
        }

        $scopedKey = (string) $scopedKey;
        $ivHexLength = openssl_cipher_iv_length('aes-256-cbc') * 2;
        if (strlen($scopedKey) <= $ivHexLength || strlen($scopedKey) % 2 !== 0 || !ctype_xdigit($scopedKey)) {
            return null;
        }
        $iv = (string) hex2bin(substr($scopedKey, 0, $ivHexLength));
        $ciphertext = (string) hex2bin(substr($scopedKey, $ivHexLength));

        [$aesKey, $legacy] = self::scopedKeyMaterial((string) $masterKey);
        $flags = OPENSSL_RAW_DATA | ($legacy ? OPENSSL_ZERO_PADDING : 0);
        $plaintext = openssl_decrypt($ciphertext, 'aes-256-cbc', $aesKey, $flags, $iv);
        if ($plaintext === false) {
            return null;
        }
        if ($legacy) {
            $plaintext = self::pkcs7Unpad($plaintext);
            if ($plaintext === null) {
                return null;
            }
        }

        return json_decode($plaintext, true);
    }

    /**
     * Normalise a saved query name: spaces become '-', and anything other than
     * letters, digits, '_' and '-' is removed.
     */
    public static function cleanQueryName($raw): string
    {
        return (string) preg_replace('/[^A-Za-z0-9_\-]/', '', str_replace(' ', '-', (string) $raw));
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    /**
     * @param mixed[] $args
     * @return mixed
     */
    private function call(string $name, array $args)
    {
        return $this->execute($this->getCommand($name, self::combineArgs($name, $args)));
    }

    /**
     * Merge a leading string argument (usually the collection name) into the
     * parameter array.
     *
     * @param mixed[] $args
     * @return array<string, mixed>
     */
    private static function combineArgs(string $name, array $args): array
    {
        if (isset($args[0]) && is_string($args[0])) {
            $params = [(self::STRING_ARG[$name] ?? 'event_collection') => $args[0]];
            if (isset($args[1]) && is_array($args[1])) {
                $params = array_merge($params, $args[1]);
            }

            return $params;
        }
        if (isset($args[0]) && is_array($args[0])) {
            return $args[0];
        }

        return [];
    }

    private function keyFor(string $auth)
    {
        switch ($auth) {
            case 'read':
                return $this->getKeyForReading();
            case 'write':
                return $this->getKeyForWriting();
            case 'organization':
                return $this->getOrganizationKey();
            default:
                return $this->getMasterKey();
        }
    }

    private static function keyName(string $auth): string
    {
        $names = ['read' => 'readKey', 'write' => 'writeKey', 'organization' => 'organizationKey'];

        return $names[$auth] ?? 'masterKey';
    }

    /**
     * JSON-encode a body; an empty array becomes {} rather than [].
     *
     * @param mixed $value
     */
    private static function encodeObject($value): string
    {
        if ($value === [] || $value === null) {
            return '{}';
        }
        $json = json_encode($value);
        if ($json === false) {
            throw new CommandException('Unable to JSON-encode request body: ' . json_last_error_msg());
        }

        return $json;
    }

    /**
     * @return array{0: string, 1: bool} AES key and whether the legacy 32-byte-block scheme applies
     */
    private static function scopedKeyMaterial(string $masterKey): array
    {
        if (strlen($masterKey) === 32) {
            return [$masterKey, true];
        }
        if (strlen($masterKey) === 64 && ctype_xdigit($masterKey)) {
            return [(string) hex2bin($masterKey), false];
        }
        throw new RuntimeException('Scoped keys need a 32-character or 64-hex-character master key');
    }

    private static function pkcs7Pad(string $data, int $blockSize): string
    {
        $pad = $blockSize - (strlen($data) % $blockSize);

        return $data . str_repeat(chr($pad), $pad);
    }

    private static function pkcs7Unpad(string $data): ?string
    {
        $len = strlen($data);
        if ($len === 0) {
            return null;
        }
        $pad = ord($data[$len - 1]);
        if ($pad < 1 || $pad > $len) {
            return null;
        }

        return substr($data, 0, $len - $pad);
    }

    private static function summarize(string $body): string
    {
        return strlen($body) > 1000 ? substr($body, 0, 1000) . ' (truncated...)' : $body;
    }

    /**
     * @param array<string, string> $headers
     */
    private function send(string $name, string $method, string $url, array $headers, ?string $body): Response
    {
        $options = ['timeout' => (float) $this->getConfig('timeout')];
        if ($this->handler !== null) {
            $response = call_user_func($this->handler, $method, $url, $headers, $body, $options);
            if (!$response instanceof Response) {
                throw new TransferException('Custom handler must return an ' . Response::class, $name);
            }

            return $response;
        }

        $headerLines = ['Connection: close'];
        foreach ($headers as $hName => $hValue) {
            $headerLines[] = $hName . ': ' . $hValue;
        }
        $http = [
            'method' => $method,
            'header' => implode("\r\n", $headerLines),
            'ignore_errors' => true,
            'protocol_version' => 1.1,
            'follow_location' => 1,
        ];
        if ($options['timeout'] > 0) {
            $http['timeout'] = $options['timeout'];
        }
        if ($body !== null) {
            $http['content'] = $body;
        }
        $context = stream_context_create(['http' => $http]);

        $error = null;
        set_error_handler(function ($severity, $message) use (&$error) {
            $error = $message;

            return true;
        });
        try {
            $responseBody = file_get_contents($url, false, $context);
            $responseHeaders = function_exists('http_get_last_response_headers')
                ? (http_get_last_response_headers() ?? [])
                : ($http_response_header ?? []);
        } finally {
            restore_error_handler();
        }

        if ($responseBody === false || $responseHeaders === []) {
            throw new TransferException(
                sprintf('Error sending %s %s: %s', $method, $url, $error ?: 'unknown transport error'),
                $name
            );
        }

        return Response::fromHeaderLines($responseHeaders, $responseBody);
    }
}
