<?php

namespace Incisive\Client\Filter;

/**
 * Encodes a parameter that may be either a plain string or a structure.
 * Strings pass through untouched; anything else is JSON-encoded. Used for
 * query-string parameters such as an absolute `timeframe` object.
 */
class MultiTypeFiltering
{
    /**
     * @param mixed $value
     */
    public static function encodeValue($value): string
    {
        return is_string($value) ? $value : (string) json_encode($value);
    }
}
