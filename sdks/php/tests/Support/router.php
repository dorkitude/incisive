<?php

/*
 * Router script for `php -S`: records every request as one JSON line in
 * $INCISIVE_MOCK_DIR/requests.jsonl and answers with the next queued response
 * from $INCISIVE_MOCK_DIR/responses.json (default: 200 {"result": 1}).
 */

$dir = getenv('INCISIVE_MOCK_DIR');

$headers = function_exists('getallheaders') ? getallheaders() : [];
$record = [
    'method' => $_SERVER['REQUEST_METHOD'],
    'uri' => $_SERVER['REQUEST_URI'],
    'headers' => array_change_key_case($headers, CASE_LOWER),
    'body' => file_get_contents('php://input'),
];
file_put_contents($dir . '/requests.jsonl', json_encode($record) . "\n", FILE_APPEND | LOCK_EX);

$response = ['status' => 200, 'headers' => ['Content-Type' => 'application/json'], 'body' => '{"result": 1}'];
$queueFile = $dir . '/responses.json';
$fp = fopen($queueFile, 'c+');
if ($fp !== false && flock($fp, LOCK_EX)) {
    $queue = json_decode(stream_get_contents($fp) ?: '[]', true) ?: [];
    if ($queue) {
        $response = array_shift($queue);
        ftruncate($fp, 0);
        rewind($fp);
        fwrite($fp, json_encode($queue));
    }
    flock($fp, LOCK_UN);
    fclose($fp);
}

http_response_code($response['status']);
foreach ($response['headers'] ?? [] as $name => $value) {
    header($name . ': ' . $value);
}
echo $response['body'];
