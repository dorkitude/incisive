<?php

namespace Incisive\Exception;

use Incisive\Client\Response;

/**
 * Raised when the server answers with an HTTP status >= 400.
 *
 * The API reports errors as {"message": "...", "error_code": "..."}; both
 * fields are parsed when present and exposed through getErrorMessage() and
 * getErrorCode(). The raw response is always available via getResponse(),
 * and getCode() returns the HTTP status.
 */
class RequestException extends CommandException
{
    /** @var Response */
    private $response;

    /** @var string|null */
    private $errorCode;

    /** @var string|null */
    private $errorMessage;

    public function __construct(string $message, string $commandName, Response $response)
    {
        parent::__construct($message, $commandName);
        $this->response = $response;
        $this->code = $response->getStatusCode();

        $decoded = json_decode($response->getBody(), true);
        if (is_array($decoded)) {
            if (isset($decoded['error_code']) && is_scalar($decoded['error_code'])) {
                $this->errorCode = (string) $decoded['error_code'];
            }
            if (isset($decoded['message']) && is_scalar($decoded['message'])) {
                $this->errorMessage = (string) $decoded['message'];
            }
        }
    }

    public function getResponse(): Response
    {
        return $this->response;
    }

    public function getStatusCode(): int
    {
        return $this->response->getStatusCode();
    }

    /** The `error_code` field of the error body, if any. */
    public function getErrorCode(): ?string
    {
        return $this->errorCode;
    }

    /** The `message` field of the error body, if any. */
    public function getErrorMessage(): ?string
    {
        return $this->errorMessage;
    }
}
