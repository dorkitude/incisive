<?php

namespace Incisive\Exception;

/**
 * Generic runtime failure (e.g. scoped key creation without a master key).
 */
class RuntimeException extends \RuntimeException implements ExceptionInterface
{
}
