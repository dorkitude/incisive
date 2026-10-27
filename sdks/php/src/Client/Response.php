<?php

namespace Incisive\Client;

/**
 * Minimal immutable HTTP response (status, headers, raw body).
 */
class Response
{
    /** @var int */
    private $statusCode;

    /** @var string */
    private $reasonPhrase;

    /** @var array<string, string[]> lower-cased header name => values */
    private $headers;

    /** @var string */
    private $body;

    /** @var string */
    private $protocolVersion;

    /**
     * @param array<string, string[]> $headers
     */
    public function __construct(
        int $statusCode,
        array $headers = [],
        string $body = '',
        string $reasonPhrase = '',
        string $protocolVersion = '1.1'
    ) {
        $this->statusCode = $statusCode;
        $this->headers = [];
        foreach ($headers as $name => $values) {
            $this->headers[strtolower($name)] = array_values((array) $values);
        }
        $this->body = $body;
        $this->reasonPhrase = $reasonPhrase;
        $this->protocolVersion = $protocolVersion;
    }

    /**
     * Build a response from the raw header lines PHP's stream wrapper reports.
     * When redirects were followed the lines of every hop are present; only
     * the final block is kept.
     *
     * @param string[] $lines
     */
    public static function fromHeaderLines(array $lines, string $body): self
    {
        $status = 0;
        $reason = '';
        $protocol = '1.1';
        $headers = [];
        foreach ($lines as $line) {
            if (preg_match('#^HTTP/(\S+)\s+(\d{3})\s*(.*)$#', $line, $m)) {
                $protocol = $m[1];
                $status = (int) $m[2];
                $reason = trim($m[3]);
                $headers = [];
                continue;
            }
            $pos = strpos($line, ':');
            if ($pos === false) {
                continue;
            }
            $headers[strtolower(trim(substr($line, 0, $pos)))][] = trim(substr($line, $pos + 1));
        }

        return new self($status, $headers, $body, $reason, $protocol);
    }

    public function getStatusCode(): int
    {
        return $this->statusCode;
    }

    public function getReasonPhrase(): string
    {
        return $this->reasonPhrase;
    }

    /** @return array<string, string[]> */
    public function getHeaders(): array
    {
        return $this->headers;
    }

    /** @return string[] */
    public function getHeader(string $name): array
    {
        return $this->headers[strtolower($name)] ?? [];
    }

    public function getHeaderLine(string $name): string
    {
        return implode(', ', $this->getHeader($name));
    }

    public function getBody(): string
    {
        return $this->body;
    }

    /** Full HTTP message, handy for debugging error responses. */
    public function __toString(): string
    {
        $out = sprintf("HTTP/%s %d %s\r\n", $this->protocolVersion, $this->statusCode, $this->reasonPhrase);
        foreach ($this->headers as $name => $values) {
            foreach ($values as $value) {
                $out .= $name . ': ' . $value . "\r\n";
            }
        }

        return $out . "\r\n" . $this->body;
    }
}
