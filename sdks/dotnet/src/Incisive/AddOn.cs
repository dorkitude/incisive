using System;
using System.Collections.Generic;
using Newtonsoft.Json;

namespace Incisive
{
    /// <summary>
    /// A data enrichment add-on, sent in the reserved metadata object as
    /// <c>addons: [{"name": ..., "input": {...}, "output": ...}]</c>.
    /// </summary>
    public sealed class AddOn
    {
        [JsonProperty(PropertyName = "name")]
        public string Name { get; }

        [JsonProperty(PropertyName = "input")]
        public Dictionary<string, string> Input { get; }

        [JsonProperty(PropertyName = "output")]
        public string Output { get; }

        public AddOn(string name, IDictionary<string, string> input, string output)
        {
            if (output == null)
                throw new ArgumentNullException(nameof(output));
            if (output.StartsWith(IncisiveConstants.ReservedNamespace + ".", StringComparison.Ordinal))
                throw new IncisiveInvalidPropertyNameException(
                    "Add-on event output name may not be in the reserved namespace: " + output);
            Name = name;
            Input = new Dictionary<string, string>(input);
            Output = output;
        }

        private static string BuiltIn(string name) => IncisiveConstants.ReservedNamespace + ":" + name;

        public static AddOn IpToGeo(string ipField, string outputField) =>
            new AddOn(BuiltIn("ip_to_geo"), new Dictionary<string, string> { { "ip", ipField } }, outputField);

        public static AddOn UserAgentParser(string userAgentString, string outputField) =>
            new AddOn(BuiltIn("ua_parser"), new Dictionary<string, string> { { "ua_string", userAgentString } }, outputField);

        public static AddOn UrlParser(string urlField, string outputField) =>
            new AddOn(BuiltIn("url_parser"), new Dictionary<string, string> { { "url", urlField } }, outputField);

        public static AddOn ReferrerParser(string referrerUrlField, string pageUrlField, string outputField) =>
            new AddOn(BuiltIn("referrer_parser"),
                new Dictionary<string, string> { { "referrer_url", referrerUrlField }, { "page_url", pageUrlField } },
                outputField);
    }

    /// <summary>A global property whose value is computed each time an event is prepared.</summary>
    public interface IDynamicPropertyValue
    {
        object Value();
    }

    public class DynamicPropertyValue : IDynamicPropertyValue
    {
        private readonly Func<object> _value;

        public DynamicPropertyValue(Func<object> value)
        {
            _value = value ?? throw new ArgumentNullException(nameof(value));
        }

        public object Value() => _value();
    }
}
