using System;
using System.Collections.Generic;

namespace Incisive
{
    /// <summary>Base type for every error raised by the client.</summary>
    public class IncisiveException : Exception
    {
        /// <summary>Server <c>error_code</c>, when the response carried one.</summary>
        public string ErrorCode { get; internal set; }

        /// <summary>HTTP status code, when the error came from a response.</summary>
        public int? StatusCode { get; internal set; }

        /// <summary>Server <c>message</c>, when the response carried one.</summary>
        public string ServerMessage { get; internal set; }

        public IncisiveException() { }
        public IncisiveException(string message) : base(message) { }
        public IncisiveException(string message, Exception inner) : base(message, inner) { }
    }

    public class IncisiveInvalidApiKeyException : IncisiveException
    {
        public IncisiveInvalidApiKeyException() { }
        public IncisiveInvalidApiKeyException(string message) : base(message) { }
        public IncisiveInvalidApiKeyException(string message, Exception inner) : base(message, inner) { }
    }

    public class IncisiveResourceNotFoundException : IncisiveException
    {
        public IncisiveResourceNotFoundException() { }
        public IncisiveResourceNotFoundException(string message) : base(message) { }
        public IncisiveResourceNotFoundException(string message, Exception inner) : base(message, inner) { }
    }

    public class IncisiveNamespaceTypeException : IncisiveException
    {
        public IncisiveNamespaceTypeException() { }
        public IncisiveNamespaceTypeException(string message) : base(message) { }
        public IncisiveNamespaceTypeException(string message, Exception inner) : base(message, inner) { }
    }

    public class IncisiveInvalidEventException : IncisiveException
    {
        public IncisiveInvalidEventException() { }
        public IncisiveInvalidEventException(string message) : base(message) { }
        public IncisiveInvalidEventException(string message, Exception inner) : base(message, inner) { }
    }

    public class IncisiveListsOfNonPrimitivesNotAllowedException : IncisiveException
    {
        public IncisiveListsOfNonPrimitivesNotAllowedException() { }
        public IncisiveListsOfNonPrimitivesNotAllowedException(string message) : base(message) { }
        public IncisiveListsOfNonPrimitivesNotAllowedException(string message, Exception inner) : base(message, inner) { }
    }

    public class IncisiveInvalidBatchException : IncisiveException
    {
        public IncisiveInvalidBatchException() { }
        public IncisiveInvalidBatchException(string message) : base(message) { }
        public IncisiveInvalidBatchException(string message, Exception inner) : base(message, inner) { }
    }

    public class IncisiveInternalServerErrorException : IncisiveException
    {
        public IncisiveInternalServerErrorException() { }
        public IncisiveInternalServerErrorException(string message) : base(message) { }
        public IncisiveInternalServerErrorException(string message, Exception inner) : base(message, inner) { }
    }

    /// <summary>A property inside the reserved metadata object was invalid.</summary>
    public class IncisiveInvalidReservedNamespacePropertyException : IncisiveException
    {
        public IncisiveInvalidReservedNamespacePropertyException() { }
        public IncisiveInvalidReservedNamespacePropertyException(string message) : base(message) { }
        public IncisiveInvalidReservedNamespacePropertyException(string message, Exception inner) : base(message, inner) { }
    }

    public class IncisiveInvalidPropertyNameException : IncisiveException
    {
        public IncisiveInvalidPropertyNameException() { }
        public IncisiveInvalidPropertyNameException(string message) : base(message) { }
        public IncisiveInvalidPropertyNameException(string message, Exception inner) : base(message, inner) { }
    }

    /// <summary>One or more events in a bulk insert were rejected.</summary>
    public class IncisiveBulkException : IncisiveException
    {
        public IEnumerable<CachedEvent> FailedEvents { get; }

        public IncisiveBulkException(IEnumerable<CachedEvent> failedEvents) { FailedEvents = failedEvents; }
        public IncisiveBulkException(string message, IEnumerable<CachedEvent> failedEvents) : base(message) { FailedEvents = failedEvents; }
    }
}
