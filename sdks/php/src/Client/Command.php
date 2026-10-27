<?php

namespace Incisive\Client;

/**
 * A named operation plus its parameters, created by IncisiveClient::getCommand()
 * and run by IncisiveClient::execute().
 */
class Command
{
    /** @var string */
    private $name;

    /** @var array<string, mixed> */
    private $params;

    /**
     * @param array<string, mixed> $params
     */
    public function __construct(string $name, array $params = [])
    {
        $this->name = $name;
        $this->params = $params;
    }

    public function getName(): string
    {
        return $this->name;
    }

    /** @return array<string, mixed> */
    public function toArray(): array
    {
        return $this->params;
    }

    public function hasParam(string $name): bool
    {
        return array_key_exists($name, $this->params);
    }

    /** @return mixed */
    public function get(string $name)
    {
        return $this->params[$name] ?? null;
    }
}
