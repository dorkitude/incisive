<?php

namespace Incisive\Client;

/**
 * Route table for API version 3.0.
 *
 * Each operation lists:
 *   - method:   HTTP verb
 *   - path:     path relative to {baseUrl}/{version}/ ('{name}' placeholders are
 *               filled from parameters and rawurlencode()d); a leading '/' means
 *               relative to {baseUrl} itself
 *   - auth:     which key goes in the Authorization header:
 *               'read' (read key, falls back to master), 'write' (write key, falls
 *               back to master), 'master', 'organization'
 *   - json:     parameters sent as JSON body fields, in this order; parameters
 *               not listed are ignored
 *   - query:    parameters sent in the query string
 *   - body:     'params' sends every non-reserved parameter as the JSON body
 *               (events and project definitions)
 *   - required: parameters that must be present and non-null
 *   - clean:    path parameters normalised with IncisiveClient::cleanQueryName()
 */
final class Operations
{
    private const COMMON = ['filters', 'timeframe', 'interval', 'timezone', 'group_by', 'include_metadata'];

    /** @var array<string, array<string, mixed>>|null */
    private static $table;

    /** @return array<string, array<string, mixed>> */
    public static function all(): array
    {
        if (self::$table === null) {
            self::$table = self::build();
        }

        return self::$table;
    }

    /** @return array<string, mixed>|null */
    public static function get(string $name): ?array
    {
        return self::all()[$name] ?? null;
    }

    /** @return array<string, array<string, mixed>> */
    private static function build(): array
    {
        $ops = [
            'getResources' => ['method' => 'GET', 'path' => '/', 'auth' => 'master'],
            'createProject' => [
                'method' => 'POST', 'path' => 'organizations/{organizationId}/projects',
                'auth' => 'organization', 'body' => 'params', 'required' => ['organizationId'],
            ],
            'getProjects' => ['method' => 'GET', 'path' => 'projects', 'auth' => 'master'],
            'getProject' => ['method' => 'GET', 'path' => 'projects/{projectId}', 'auth' => 'master'],

            'getSavedQueries' => ['method' => 'GET', 'path' => 'projects/{projectId}/queries/saved', 'auth' => 'master'],
            'getSavedQuery' => [
                'method' => 'GET', 'path' => 'projects/{projectId}/queries/saved/{query_name}',
                'auth' => 'master', 'required' => ['query_name'],
            ],
            'createSavedQuery' => [
                'method' => 'PUT', 'path' => 'projects/{projectId}/queries/saved/{query_name}',
                'auth' => 'master', 'json' => ['query'], 'required' => ['query_name'], 'clean' => ['query_name'],
            ],
            'updateSavedQuery' => [
                'method' => 'PUT', 'path' => 'projects/{projectId}/queries/saved/{query_name}',
                'auth' => 'master', 'json' => ['query'], 'required' => ['query_name'], 'clean' => ['query_name'],
            ],
            'deleteSavedQuery' => [
                'method' => 'DELETE', 'path' => 'projects/{projectId}/queries/saved/{query_name}',
                'auth' => 'master', 'required' => ['query_name'],
            ],
            'getSavedQueryResults' => [
                'method' => 'GET', 'path' => 'projects/{projectId}/queries/saved/{query_name}/result',
                'auth' => 'master', 'required' => ['query_name'],
            ],

            'getCollections' => ['method' => 'GET', 'path' => 'projects/{projectId}/events', 'auth' => 'master'],
            'getEventSchemas' => ['method' => 'GET', 'path' => 'projects/{projectId}/events', 'auth' => 'master'],
            'getCollection' => [
                'method' => 'GET', 'path' => 'projects/{projectId}/events/{event_collection}',
                'auth' => 'master', 'required' => ['event_collection'],
            ],
            'deleteCollection' => [
                'method' => 'DELETE', 'path' => 'projects/{projectId}/events/{collection_name}',
                'auth' => 'master', 'required' => ['collection_name'],
            ],
            'getProperty' => [
                'method' => 'GET', 'path' => 'projects/{projectId}/events/{event_collection}/properties/{property_name}',
                'auth' => 'master', 'required' => ['event_collection', 'property_name'],
            ],

            'addEvent' => [
                'method' => 'POST', 'path' => 'projects/{projectId}/events/{event_collection}',
                'auth' => 'write', 'body' => 'params', 'required' => ['event_collection'],
            ],
            'addEvents' => [
                'method' => 'POST', 'path' => 'projects/{projectId}/events',
                'auth' => 'write', 'body' => 'params',
            ],
            'deleteEvents' => [
                'method' => 'DELETE', 'path' => 'projects/{projectId}/events/{event_collection}',
                'auth' => 'master', 'query' => ['filters', 'timeframe', 'timezone'], 'required' => ['event_collection'],
            ],
            'deleteEventProperties' => [
                'method' => 'DELETE', 'path' => 'projects/{projectId}/events/{event_collection}/properties/{property_name}',
                'auth' => 'master', 'required' => ['event_collection', 'property_name'],
            ],

            'count' => self::query('count', ['event_collection'], array_merge(['event_collection'], self::COMMON)),
            'funnel' => self::query('funnel', [], ['steps', 'include_metadata']),
            'multiAnalysis' => self::query(
                'multi_analysis',
                ['event_collection', 'analyses'],
                array_merge(['event_collection', 'analyses'], self::COMMON)
            ),
            'extraction' => self::query(
                'extraction',
                ['event_collection'],
                ['event_collection', 'filters', 'timeframe', 'email', 'latest', 'property_names']
            ),
            'percentile' => self::query(
                'percentile',
                ['event_collection', 'target_property', 'percentile'],
                [
                    'event_collection', 'target_property', 'filters', 'timeframe', 'interval',
                    'timezone', 'group_by', 'percentile', 'include_metadata',
                ]
            ),
        ];

        $targeted = [
            'countUnique' => 'count_unique',
            'minimum' => 'minimum',
            'maximum' => 'maximum',
            'average' => 'average',
            'sum' => 'sum',
            'selectUnique' => 'select_unique',
            'median' => 'median',
        ];
        foreach ($targeted as $method => $analysis) {
            $ops[$method] = self::query(
                $analysis,
                ['event_collection', 'target_property'],
                array_merge(['event_collection', 'target_property'], self::COMMON)
            );
        }

        return $ops;
    }

    /**
     * @param string[] $required
     * @param string[] $json
     * @return array<string, mixed>
     */
    private static function query(string $analysis, array $required, array $json): array
    {
        return [
            'method' => 'POST',
            'path' => 'projects/{projectId}/queries/' . $analysis,
            'auth' => 'read',
            'json' => $json,
            'required' => $required,
        ];
    }
}
