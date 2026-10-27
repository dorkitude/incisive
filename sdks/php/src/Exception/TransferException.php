<?php

namespace Incisive\Exception;

/**
 * Raised when the HTTP request could not be completed at all
 * (connection refused, DNS failure, timeout).
 */
class TransferException extends CommandException
{
}
