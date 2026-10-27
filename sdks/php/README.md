# incisive PHP client

PHP client for an Incisive analytics server: record events, run analyses,
manage saved queries and create scoped keys. No runtime dependencies beyond
`ext-json` and `ext-openssl` (HTTP goes through PHP's stream wrapper).

## Install

```sh
composer require dorkitude/incisive
```

PHP 8.1 or newer.

## Configure

```php
use Incisive\Client\IncisiveClient;

$client = IncisiveClient::factory([
    'projectId' => $projectId,
    'writeKey'  => $writeKey,
    'readKey'   => $readKey,
    'masterKey' => $masterKey,                // only for admin operations / scoped keys
    'baseUrl'   => 'https://analytics.example.com', // default http://localhost:8080
    'version'   => '1.0',                     // API version path segment (default)
]);

// or from INCISIVE_PROJECT_ID, INCISIVE_WRITE_KEY, INCISIVE_READ_KEY,
// INCISIVE_MASTER_KEY and INCISIVE_BASE_URL:
$client = IncisiveClient::fromEnv();
```

Other options: `timeout` (seconds, default 60), `headers` (extra request
headers), `organizationId` / `organizationKey` (for `createProject`), and
`handler` (a callable `fn($method, $url, $headers, $body, $options): Response`
that replaces the HTTP transport).

Every option has a getter/setter (`setProjectId()`, `getReadKey()`,
`setBaseUrl()`, ...). `getKeyForReading()` / `getKeyForWriting()` fall back to
the master key when the read/write key is empty.

The key goes in the `Authorization` header: write key for `addEvent` and
`addEvents`; read key for analyses; master key for everything else (deletes,
discovery, saved queries, projects).

## Record events

```php
$client->addEvent('purchases', ['item' => 'golden widget', 'price' => 500]);

$client->addEvents([
    'purchases' => [['item' => 'a'], ['item' => 'b']],
    'sign_ups'  => [['name' => 'foo']],
]);
```

Properties under the reserved metadata object (`'keen' => ['timestamp' => ...,
'addons' => [...]]`) are passed through untouched.

## Analyses

Each analysis takes a collection name plus an argument array, or a single
array that contains `event_collection`:

```php
$client->count('purchases', ['timeframe' => 'this_14_days']);
$client->countUnique('purchases', ['target_property' => 'user.id', 'timeframe' => 'this_14_days']);
$client->sum(['event_collection' => 'purchases', 'target_property' => 'price', 'group_by' => 'item']);
$client->percentile('purchases', ['target_property' => 'price', 'percentile' => 90]);
$client->multiAnalysis('purchases', ['analyses' => [
    'clicks'    => ['analysis_type' => 'count'],
    'avg price' => ['analysis_type' => 'average', 'target_property' => 'price'],
]]);
$client->funnel(['steps' => [
    ['event_collection' => 'sign_ups', 'actor_property' => 'user.id', 'timeframe' => 'this_7_days'],
    ['event_collection' => 'purchases', 'actor_property' => 'user.id', 'timeframe' => 'this_7_days'],
]]);
$client->extraction('purchases', ['timeframe' => 'this_day', 'latest' => 100, 'property_names' => ['item']]);
```

Available: `count`, `countUnique`, `minimum`, `maximum`, `average`, `sum`,
`median`, `percentile`, `selectUnique`, `multiAnalysis`, `funnel`,
`extraction`. Each sends a JSON `POST` to `/1.0/projects/{id}/queries/<analysis>`
containing only the parameters that analysis accepts (`event_collection`,
`target_property`, `filters`, `timeframe`, `interval`, `timezone`, `group_by`,
`include_metadata`, plus `percentile`, `analyses`, `steps`, `email`, `latest`,
`property_names` where relevant); anything else is dropped.

Responses are returned as the decoded JSON body (associative array), e.g.
`['result' => 42]`; nothing is unwrapped. Empty or non-JSON bodies give `null`.

## Collections, properties, deletes

```php
$client->getCollections();                                  // also getEventSchemas()
$client->getCollection('purchases');
$client->getProperty('purchases', ['property_name' => 'item']);
$client->deleteEvents('purchases', ['filters' => $filters, 'timeframe' => 'this_week']);
$client->deleteEventProperties('purchases', ['property_name' => 'obsolete']);
$client->deleteCollection('purchases');                     // or ['collection_name' => 'purchases']
```

`deleteEvents` sends its `filters`, `timeframe` and `timezone` in the query
string; `filters` and an absolute timeframe are JSON encoded
(`filters=[{"property_name":...}]`). Without filters or a timeframe it deletes
every event in the collection.

## Saved and cached queries (master key)

```php
$client->createSavedQuery(['query_name' => 'total-purchases', 'query' => [
    'analysis_type' => 'count', 'event_collection' => 'purchases',
    'timeframe' => 'this_2_weeks', 'refresh_rate' => 14400, // refresh_rate => cached query
]]);
$client->updateSavedQuery(['query_name' => 'total-purchases', 'query' => [...]]);
$client->getSavedQuery('total-purchases');
$client->getSavedQueryResults('total-purchases');
$client->getSavedQueries();
$client->deleteSavedQuery('total-purchases');
```

Create/update send `{"query": {...}}` and normalise the name with
`IncisiveClient::cleanQueryName()` (spaces become `-`, other characters outside
`[A-Za-z0-9_-]` are removed).

## Projects

`getResources()` (host root `/`), `getProjects()`, `getProject()`, and
`createProject([...])` (`POST /1.0/organizations/{organizationId}/projects`
with the organization key).

## Scoped keys

```php
$scoped = $client->createScopedKey(
    [['property_name' => 'user_id', 'operator' => 'eq', 'property_value' => '123']],
    ['read']
);
$client->decryptScopedKey($scoped); // ['filters' => [...], 'allowed_operations' => ['read']]
```

AES-256-CBC with a random IV; the key is `hex(iv) . hex(ciphertext)`. A
64-hex-character master key is hex-decoded into the AES key (PKCS#7, 16-byte
blocks); a 32-character master key is used as-is with PKCS#7 padding to
32-byte blocks.

## Errors

| Situation | Exception |
|---|---|
| Missing required parameter or key (nothing is sent) | `Incisive\Exception\CommandException` |
| Connection failure / timeout | `Incisive\Exception\TransferException` |
| HTTP 4xx | `Incisive\Exception\CommandClientException` |
| HTTP 5xx | `Incisive\Exception\CommandServerException` |

The HTTP exceptions extend `RequestException`, which offers `getStatusCode()`
(also `getCode()`), `getErrorCode()` and `getErrorMessage()` (the
`error_code` / `message` fields of the error body), and `getResponse()`.
Everything implements `Incisive\Exception\ExceptionInterface`.

## Lower-level API

```php
$command = $client->getCommand('count', ['event_collection' => 'purchases']);
$request = $client->buildRequest($command); // inspect method/url/headers/body
$result  = $client->execute($command);
```

## Tests

PHP is run through Docker:

```sh
docker run --rm --user "$(id -u):$(id -g)" -e COMPOSER_HOME=/tmp/ch -v "$PWD":/app -w /app composer:2.8.3 composer install
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD":/app -w /app php:8.3.14-cli vendor/bin/phpunit
```

Unit tests start a local `php -S` server on a free port and assert the exact
requests. Integration tests (`tests/Integration`) run only when
`INCISIVE_BASE_URL` and `INCISIVE_PROJECT_ID` (plus `INCISIVE_WRITE_KEY`,
`INCISIVE_READ_KEY`, `INCISIVE_MASTER_KEY` as needed) are set:

```sh
docker run --rm --network host -v "$PWD":/app -w /app \
  -e INCISIVE_BASE_URL=http://localhost:8080 -e INCISIVE_PROJECT_ID=... \
  -e INCISIVE_WRITE_KEY=... -e INCISIVE_READ_KEY=... -e INCISIVE_MASTER_KEY=... \
  php:8.3.14-cli vendor/bin/phpunit --testsuite integration
```
