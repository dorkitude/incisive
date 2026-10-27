using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using System.Threading.Tasks;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace Incisive
{
    /// <summary>Validation and response helpers.</summary>
    public static class IncisiveUtil
    {
        private static readonly Regex NonAscii = new Regex("[^\x00-\x7F]", RegexOptions.Compiled);

        // Error code for a bad property inside the reserved metadata object; it embeds the
        // capitalised namespace name, so derive it rather than spelling it out.
        internal static readonly string InvalidReservedNamespacePropertyCode =
            "Invalid" + char.ToUpperInvariant(IncisiveConstants.ReservedNamespace[0]) +
            IncisiveConstants.ReservedNamespace.Substring(1) + "NamespaceProperty";

        public static string GetSdkVersion() => "dotnet-" + IncisiveConstants.SdkVersion;

        public static string ToSafeString(this object obj) => (obj ?? string.Empty).ToString();

        public static void ValidatePropertyName(string property)
        {
            if (string.IsNullOrWhiteSpace(property))
                throw new IncisiveException("Property name may not be null or whitespace");
            if (property.Length >= 256)
                throw new IncisiveException("Property name must be less than 256 characters");
            if (property.StartsWith("$", StringComparison.Ordinal))
                throw new IncisiveException("Property name may not start with \"$\"");
            if (property.IndexOf('.') >= 0)
                throw new IncisiveException("Property name may not contain \".\"");
        }

        public static void ValidateEventCollectionName(string collection)
        {
            if (null == collection)
                throw new IncisiveException("Event collection name may not be null.");
            if (string.IsNullOrWhiteSpace(collection))
                throw new IncisiveException("Event collection name may not be blank.");
            if (collection.Length > 64)
                throw new IncisiveException("Event collection name may not be longer than 64 characters.");
            if (NonAscii.IsMatch(collection))
                throw new IncisiveException("Event collection name must contain only Ascii characters.");
            if (collection.IndexOf('$') >= 0)
                throw new IncisiveException("Event collection name may not contain \"$\".");
            if (collection.StartsWith("_", StringComparison.Ordinal))
                throw new IncisiveException("Event collection name may not begin with \"_\".");
        }

        /// <summary>Build the typed exception for a server error code.</summary>
        public static IncisiveException CreateApiException(string errorCode, string message, int? statusCode = null)
        {
            IncisiveException ex;
            switch (errorCode)
            {
                case "InvalidApiKeyError": ex = new IncisiveInvalidApiKeyException(message); break;
                case "ResourceNotFoundError": ex = new IncisiveResourceNotFoundException(message); break;
                case "NamespaceTypeError": ex = new IncisiveNamespaceTypeException(message); break;
                case "InvalidEventError": ex = new IncisiveInvalidEventException(message); break;
                case "ListsOfNonPrimitivesNotAllowedError": ex = new IncisiveListsOfNonPrimitivesNotAllowedException(message); break;
                case "InvalidBatchError": ex = new IncisiveInvalidBatchException(message); break;
                case "InternalServerError": ex = new IncisiveInternalServerErrorException(message); break;
                case "InvalidPropertyNameError": ex = new IncisiveInvalidPropertyNameException(message); break;
                default:
                    ex = errorCode == InvalidReservedNamespacePropertyCode
                        ? new IncisiveInvalidReservedNamespacePropertyException(message)
                        : new IncisiveException(errorCode + " : " + message);
                    break;
            }
            ex.ErrorCode = errorCode;
            ex.ServerMessage = message;
            ex.StatusCode = statusCode;
            return ex;
        }

        /// <summary>
        /// Error for one item of a bulk insert response:
        /// <c>{"success": false, "error": {"name": ..., "description": ...}}</c>. Null if none.
        /// </summary>
        public static Exception GetBulkApiError(JObject apiResponse)
        {
            var error = apiResponse?["error"] as JObject;
            if (null == error)
                return null;
            return CreateApiException((string)error["name"], (string)error["description"]);
        }

        /// <summary>Throw if a parsed response carries an <c>error_code</c>.</summary>
        public static void CheckApiErrorCode(JToken apiResponse, int? statusCode = null)
        {
            if (!(apiResponse is JObject obj))
                return;
            var errorCode = obj["error_code"];
            if (errorCode == null || errorCode.Type == JTokenType.Null)
                return;
            throw CreateApiException((string)errorCode, (string)obj["message"], statusCode);
        }

        // ---- internal helpers -------------------------------------------------------

        internal static JToken ParseJson(string text)
        {
            using (var reader = new JsonTextReader(new StringReader(text)))
            {
                reader.DateParseHandling = DateParseHandling.None;
                reader.FloatParseHandling = FloatParseHandling.Double;
                var token = JToken.ReadFrom(reader);
                return token;
            }
        }

        internal static JToken TryParseJson(string text)
        {
            if (string.IsNullOrWhiteSpace(text))
                return null;
            try { return ParseJson(text); }
            catch (JsonException) { return null; }
        }

        /// <summary>
        /// Shared response handling: typed exception for an error_code body, generic
        /// exception (with status) for other non-2xx responses, otherwise the parsed body.
        /// </summary>
        internal static JToken CheckResponse(ApiResponse response, string operation)
        {
            var token = TryParseJson(response.Body);
            CheckApiErrorCode(token, response.StatusCode);
            if (!response.IsSuccessStatusCode)
            {
                var msg = (token as JObject)?["message"];
                var ex = new IncisiveException(
                    operation + " failed with status: " + response.StatusCode +
                    (msg != null && msg.Type == JTokenType.String ? " (" + (string)msg + ")" : ""));
                ex.StatusCode = response.StatusCode;
                ex.ServerMessage = msg?.Type == JTokenType.String ? (string)msg : null;
                throw ex;
            }
            return token;
        }

        internal static JObject RequireObject(JToken token, string operation)
        {
            if (token is JObject o)
                return o;
            throw new IncisiveException(operation + " returned an unexpected response body.");
        }

        internal static JArray RequireArray(JToken token, string operation)
        {
            if (token is JArray a)
                return a;
            throw new IncisiveException(operation + " returned an unexpected response body.");
        }

        /// <summary>String form of a result value: scalars invariant, arrays comma-joined.</summary>
        internal static string TokenToString(JToken token)
        {
            if (token == null || token.Type == JTokenType.Null || token.Type == JTokenType.Undefined)
                return null;
            if (token is JArray arr)
                return string.Join(",", arr.Select(TokenToString));
            if (token is JValue v)
            {
                switch (v.Type)
                {
                    case JTokenType.String: return (string)v.Value;
                    case JTokenType.Boolean: return (bool)v.Value ? "True" : "False";
                    case JTokenType.Float:
                        return Convert.ToDouble(v.Value, CultureInfo.InvariantCulture).ToString("R", CultureInfo.InvariantCulture);
                    default: return Convert.ToString(v.Value, CultureInfo.InvariantCulture);
                }
            }
            return token.ToString(Formatting.None);
        }

        internal static DateTime ParseDate(JToken token)
        {
            if (token == null || token.Type == JTokenType.Null)
                return default(DateTime);
            if (token.Type == JTokenType.Date)
                return ((DateTime)token).ToUniversalTime();
            return DateTime.Parse((string)token, CultureInfo.InvariantCulture,
                DateTimeStyles.AdjustToUniversal | DateTimeStyles.AssumeUniversal);
        }

        internal static string EscapePath(string segment) => Uri.EscapeDataString(segment ?? "");

        internal static string BuildQueryString(IEnumerable<KeyValuePair<string, string>> parms)
        {
            if (parms == null)
                return "";
            return string.Join("&", parms
                .Where(p => !string.IsNullOrEmpty(p.Value))
                .Select(p => p.Key + "=" + Uri.EscapeDataString(p.Value)));
        }

        internal static void RunSync(Func<Task> f) => Task.Run(f).GetAwaiter().GetResult();

        internal static T RunSync<T>(Func<Task<T>> f) => Task.Run(f).GetAwaiter().GetResult();
    }
}
