<?php

namespace Incisive\Tests\Support;

/**
 * A real HTTP server (`php -S` + router.php) on 127.0.0.1 with an
 * OS-assigned free port, started once for the whole test run.
 */
final class MockServer
{
    /** @var resource|null */
    private static $process;

    /** @var string */
    private static $dir;

    /** @var int */
    private static $port;

    public static function start(): void
    {
        if (self::$process !== null) {
            return;
        }
        self::$dir = sys_get_temp_dir() . '/incisive-php-mock-' . bin2hex(random_bytes(4));
        mkdir(self::$dir);
        self::reset();

        // Bind port 0 to let the OS choose a free port, then hand it to php -S.
        $probe = stream_socket_server('tcp://127.0.0.1:0', $errno, $errstr);
        if ($probe === false) {
            throw new \RuntimeException("cannot bind probe socket: $errstr");
        }
        $name = stream_socket_get_name($probe, false);
        self::$port = (int) substr($name, strrpos($name, ':') + 1);
        fclose($probe);

        $env = array_merge(getenv(), ['INCISIVE_MOCK_DIR' => self::$dir]);
        $cmd = [PHP_BINARY, '-S', '127.0.0.1:' . self::$port, __DIR__ . '/router.php'];
        self::$process = proc_open(
            $cmd,
            [0 => ['pipe', 'r'], 1 => ['file', self::$dir . '/server.log', 'a'], 2 => ['file', self::$dir . '/server.log', 'a']],
            $pipes,
            self::$dir,
            $env
        );
        if (!is_resource(self::$process)) {
            throw new \RuntimeException('cannot start php -S');
        }

        $deadline = microtime(true) + 10;
        while (microtime(true) < $deadline) {
            $sock = @fsockopen('127.0.0.1', self::$port, $errno, $errstr, 0.2);
            if ($sock !== false) {
                fclose($sock);
                register_shutdown_function([self::class, 'stop']);

                return;
            }
            usleep(50000);
        }
        throw new \RuntimeException('php -S did not come up: ' . @file_get_contents(self::$dir . '/server.log'));
    }

    public static function stop(): void
    {
        if (self::$process !== null) {
            proc_terminate(self::$process);
            proc_close(self::$process);
            self::$process = null;
        }
        foreach (glob(self::$dir . '/*') ?: [] as $file) {
            @unlink($file);
        }
        @rmdir(self::$dir);
    }

    public static function url(): string
    {
        return 'http://127.0.0.1:' . self::$port;
    }

    public static function reset(): void
    {
        file_put_contents(self::$dir . '/requests.jsonl', '');
        file_put_contents(self::$dir . '/responses.json', '[]');
    }

    /**
     * @param array<string, string> $headers
     */
    public static function enqueue(int $status, string $body, array $headers = ['Content-Type' => 'application/json']): void
    {
        $queue = json_decode((string) file_get_contents(self::$dir . '/responses.json'), true) ?: [];
        $queue[] = ['status' => $status, 'body' => $body, 'headers' => $headers];
        file_put_contents(self::$dir . '/responses.json', json_encode($queue));
    }

    /** @return array<int, array{method: string, uri: string, headers: array<string, string>, body: string}> */
    public static function requests(): array
    {
        $lines = array_filter(explode("\n", (string) file_get_contents(self::$dir . '/requests.jsonl')));

        return array_values(array_map(function ($line) {
            return json_decode($line, true);
        }, $lines));
    }

    /** @return array{method: string, uri: string, headers: array<string, string>, body: string} */
    public static function lastRequest(): array
    {
        $requests = self::requests();
        if (!$requests) {
            throw new \RuntimeException('mock server received no requests');
        }

        return end($requests);
    }
}
