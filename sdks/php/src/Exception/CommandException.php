<?php

namespace Incisive\Exception;

/**
 * Raised when a command cannot be executed: client-side validation errors
 * (missing required parameters or API keys) and transport failures.
 */
class CommandException extends RuntimeException
{
    /** @var string */
    private $commandName;

    public function __construct(string $message, string $commandName = '', ?\Throwable $previous = null)
    {
        parent::__construct($message, 0, $previous);
        $this->commandName = $commandName;
    }

    public function getCommandName(): string
    {
        return $this->commandName;
    }
}
