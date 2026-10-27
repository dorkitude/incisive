using System;
using System.Collections.Generic;
using System.Globalization;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace Incisive
{
    /// <summary>An analysis type for <see cref="IncisiveClient.Query(QueryType,string,string,IQueryTimeframe,IEnumerable{QueryFilter},string)"/>.</summary>
    public sealed class QueryType : IEquatable<QueryType>
    {
        private readonly string _value;

        /// <summary>Percentile to compute (only for <see cref="Percentile"/>).</summary>
        public double? PercentileValue { get; }

        private QueryType(string value, double? percentile = null)
        {
            _value = value;
            PercentileValue = percentile;
        }

        public override string ToString() => _value;
        public static implicit operator string(QueryType value) => value?.ToString();

        public static QueryType Count() => new QueryType(IncisiveConstants.QueryCount);
        public static QueryType CountUnique() => new QueryType(IncisiveConstants.QueryCountUnique);
        public static QueryType Minimum() => new QueryType(IncisiveConstants.QueryMinimum);
        public static QueryType Maximum() => new QueryType(IncisiveConstants.QueryMaximum);
        public static QueryType Average() => new QueryType(IncisiveConstants.QueryAverage);
        public static QueryType Sum() => new QueryType(IncisiveConstants.QuerySum);
        public static QueryType Median() => new QueryType(IncisiveConstants.QueryMedian);
        public static QueryType StandardDeviation() => new QueryType(IncisiveConstants.QueryStandardDeviation);
        public static QueryType SelectUnique() => new QueryType(IncisiveConstants.QuerySelectUnique);

        /// <summary>Percentile analysis; sends <c>percentile=&lt;p&gt;</c>.</summary>
        public static QueryType Percentile(double percentile)
        {
            if (percentile < 0 || percentile > 100)
                throw new ArgumentOutOfRangeException(nameof(percentile), "Percentile must be between 0 and 100.");
            return new QueryType(IncisiveConstants.QueryPercentile, percentile);
        }

        public bool Equals(QueryType other) =>
            !(other is null) && _value == other._value && PercentileValue == other.PercentileValue;
        public override bool Equals(object obj) => Equals(obj as QueryType);
        public override int GetHashCode() => (_value ?? "").GetHashCode() ^ PercentileValue.GetHashCode();
        public static bool operator ==(QueryType a, QueryType b) => a is null ? b is null : a.Equals(b);
        public static bool operator !=(QueryType a, QueryType b) => !(a == b);

        internal bool IsSelectUnique => _value == IncisiveConstants.QuerySelectUnique;
        internal bool IsCount => _value == IncisiveConstants.QueryCount;
    }

    /// <summary>Interval for series queries.</summary>
    public sealed class QueryInterval
    {
        private readonly string _value;
        private QueryInterval(string value) { _value = value; }
        public override string ToString() => _value;
        public static implicit operator string(QueryInterval value) => value?.ToString();

        public static QueryInterval Minutely() => new QueryInterval("minutely");
        public static QueryInterval Hourly() => new QueryInterval("hourly");
        public static QueryInterval Daily() => new QueryInterval("daily");
        public static QueryInterval Weekly() => new QueryInterval("weekly");
        public static QueryInterval Monthly() => new QueryInterval("monthly");
        public static QueryInterval Yearly() => new QueryInterval("yearly");
        public static QueryInterval EveryNMinutes(int n) => new QueryInterval($"every_{n}_minutes");
        public static QueryInterval EveryNHours(int n) => new QueryInterval($"every_{n}_hours");
        public static QueryInterval EveryNDays(int n) => new QueryInterval($"every_{n}_days");
        public static QueryInterval EveryNWeeks(int n) => new QueryInterval($"every_{n}_weeks");
        public static QueryInterval EveryNMonths(int n) => new QueryInterval($"every_{n}_months");
        public static QueryInterval EveryNYears(int n) => new QueryInterval($"every_{n}_years");
    }

    /// <summary>Marker for relative and absolute timeframes.</summary>
    [JsonConverter(typeof(TimeframeConverter))]
    public interface IQueryTimeframe
    {
    }

    /// <summary>Relative timeframe such as <c>this_7_days</c>; sent as a plain string.</summary>
    [JsonConverter(typeof(TimeframeConverter))]
    public sealed class QueryRelativeTimeframe : IQueryTimeframe
    {
        private readonly string _value;
        private QueryRelativeTimeframe(string value) { _value = value; }

        /// <summary>Wrap an arbitrary relative timeframe string.</summary>
        public static QueryRelativeTimeframe Create(string value) => new QueryRelativeTimeframe(value);

        public override string ToString() => _value;

        public static QueryRelativeTimeframe ThisMinute() => Create("this_minute");
        public static QueryRelativeTimeframe ThisHour() => Create("this_hour");
        public static QueryRelativeTimeframe ThisDay() => Create("this_day");
        public static QueryRelativeTimeframe ThisWeek() => Create("this_week");
        public static QueryRelativeTimeframe ThisMonth() => Create("this_month");
        public static QueryRelativeTimeframe ThisYear() => Create("this_year");
        public static QueryRelativeTimeframe ThisNMinutes(int n) => Create($"this_{n}_minutes");
        public static QueryRelativeTimeframe ThisNHours(int n) => Create($"this_{n}_hours");
        public static QueryRelativeTimeframe ThisNDays(int n) => Create($"this_{n}_days");
        public static QueryRelativeTimeframe ThisNWeeks(int n) => Create($"this_{n}_weeks");
        public static QueryRelativeTimeframe ThisNMonths(int n) => Create($"this_{n}_months");
        public static QueryRelativeTimeframe ThisNYears(int n) => Create($"this_{n}_years");
        public static QueryRelativeTimeframe PreviousNMinutes(int n) => Create($"previous_{n}_minutes");
        public static QueryRelativeTimeframe PreviousNHours(int n) => Create($"previous_{n}_hours");
        public static QueryRelativeTimeframe PreviousNDays(int n) => Create($"previous_{n}_days");
        public static QueryRelativeTimeframe PreviousNWeeks(int n) => Create($"previous_{n}_weeks");
        public static QueryRelativeTimeframe PreviousNMonths(int n) => Create($"previous_{n}_months");
        public static QueryRelativeTimeframe PreviousNYears(int n) => Create($"previous_{n}_years");
        public static QueryRelativeTimeframe PreviousMinute() => Create("previous_minute");
        public static QueryRelativeTimeframe PreviousHour() => Create("previous_hour");
        public static QueryRelativeTimeframe Yesterday() => Create("yesterday");
        public static QueryRelativeTimeframe PreviousWeek() => Create("previous_week");
        public static QueryRelativeTimeframe PreviousMonth() => Create("previous_month");
        public static QueryRelativeTimeframe PreviousYear() => Create("previous_year");

        public override bool Equals(object obj) => obj is QueryRelativeTimeframe t && _value == t._value;
        public override int GetHashCode() => (_value ?? "").GetHashCode();
    }

    /// <summary>Absolute timeframe; sent as compact JSON <c>{"start": ISO-8601, "end": ISO-8601}</c>.</summary>
    [JsonConverter(typeof(TimeframeConverter))]
    public sealed class QueryAbsoluteTimeframe : IQueryTimeframe
    {
        public DateTime Start { get; }
        public DateTime End { get; }

        public QueryAbsoluteTimeframe(DateTime start, DateTime end)
        {
            if (start >= end)
                throw new ArgumentException("Start date must be before stop date.");
            Start = start;
            End = end;
        }

        internal JObject ToJObject() => new JObject
        {
            { "start", FormatDate(Start) },
            { "end", FormatDate(End) },
        };

        internal static string FormatDate(DateTime d)
        {
            // ISO-8601 with the same zone conventions as the default JSON date format:
            // UTC -> "Z", Local -> "+hh:mm", Unspecified -> no designator.
            return JsonConvert.SerializeObject(d).Trim('"');
        }

        public override string ToString() => ToJObject().ToString(Formatting.None);

        public override bool Equals(object obj) => obj is QueryAbsoluteTimeframe t && Start == t.Start && End == t.End;
        public override int GetHashCode() => Start.GetHashCode() * 31 + End.GetHashCode();
    }

    /// <summary>Reads/writes timeframes: string = relative, object = absolute.</summary>
    public sealed class TimeframeConverter : JsonConverter
    {
        public override bool CanConvert(Type objectType) => typeof(IQueryTimeframe).IsAssignableFrom(objectType);

        public override void WriteJson(JsonWriter writer, object value, JsonSerializer serializer)
        {
            switch (value)
            {
                case null: writer.WriteNull(); break;
                case QueryRelativeTimeframe r: writer.WriteValue(r.ToString()); break;
                case QueryAbsoluteTimeframe a: a.ToJObject().WriteTo(writer); break;
                default: throw new ArgumentException("Type not supported by TimeframeConverter", nameof(value));
            }
        }

        public override object ReadJson(JsonReader reader, Type objectType, object existingValue, JsonSerializer serializer)
        {
            var token = JToken.Load(reader);
            switch (token.Type)
            {
                case JTokenType.Null: return null;
                case JTokenType.String: return QueryRelativeTimeframe.Create((string)token);
                case JTokenType.Object:
                    return new QueryAbsoluteTimeframe(IncisiveUtil.ParseDate(token["start"]), IncisiveUtil.ParseDate(token["end"]));
                default: throw new JsonSerializationException("Unexpected timeframe token " + token.Type);
            }
        }
    }

    /// <summary>A query filter: <c>{"property_name", "operator", "property_value"}</c>.</summary>
    public sealed class QueryFilter
    {
        public sealed class OperatorConverter : JsonConverter
        {
            public override bool CanConvert(Type objectType) => objectType == typeof(FilterOperator);
            public override object ReadJson(JsonReader reader, Type objectType, object existingValue, JsonSerializer serializer)
                => reader.Value == null ? null : new FilterOperator(reader.Value.ToString());
            public override void WriteJson(JsonWriter writer, object value, JsonSerializer serializer)
                => writer.WriteValue(value.ToString());
        }

        [JsonConverter(typeof(OperatorConverter))]
        public sealed class FilterOperator
        {
            private readonly string _value;
            internal FilterOperator(string value) { _value = value; }
            public override string ToString() => _value;
            public static implicit operator string(FilterOperator value) => value?.ToString();

            /// <summary>Any operator string the server understands.</summary>
            public static FilterOperator Create(string op) => new FilterOperator(op);

            public static FilterOperator Equals() => new FilterOperator("eq");
            public static FilterOperator NotEqual() => new FilterOperator("ne");
            public static FilterOperator LessThan() => new FilterOperator("lt");
            public static FilterOperator LessThanOrEqual() => new FilterOperator("lte");
            public static FilterOperator GreaterThan() => new FilterOperator("gt");
            public static FilterOperator GreaterThanOrEqual() => new FilterOperator("gte");
            public static FilterOperator Exists() => new FilterOperator("exists");
            public static FilterOperator In() => new FilterOperator("in");
            public static FilterOperator Contains() => new FilterOperator("contains");
            public static FilterOperator NotContains() => new FilterOperator("not_contains");
            public static FilterOperator Within() => new FilterOperator("within");

            public override bool Equals(object obj) => obj is FilterOperator o && o._value == _value;
            public override int GetHashCode() => (_value ?? "").GetHashCode();
        }

        /// <summary>Value for the <c>within</c> operator.</summary>
        public class GeoValue
        {
            [JsonProperty(PropertyName = "coordinates")]
            public double[] Coordinates { get; private set; }

            [JsonProperty(PropertyName = "max_distance_miles")]
            public double MaxDistanceMiles { get; private set; }

            public GeoValue(double longitude, double latitude, double maxDistanceMiles)
            {
                Coordinates = new[] { longitude, latitude };
                MaxDistanceMiles = maxDistanceMiles;
            }
        }

        [JsonProperty(PropertyName = "property_name")]
        public string PropertyName { get; set; }

        [JsonProperty(PropertyName = "operator")]
        public FilterOperator Operator { get; set; }

        [JsonProperty(PropertyName = "property_value")]
        public object Value { get; set; }

        public QueryFilter() { }

        public QueryFilter(string property, FilterOperator op, object value)
        {
            if (string.IsNullOrWhiteSpace(property))
                throw new ArgumentNullException(nameof(property), "Property name is required.");
            PropertyName = property;
            Operator = op ?? throw new ArgumentNullException(nameof(op), "Filter operator is required.");
            Value = value;
        }
    }

    /// <summary>One analysis inside a multi-analysis query.</summary>
    public sealed class MultiAnalysisParam
    {
        public sealed class Metric
        {
            private readonly string _value;
            public readonly string TargetProperty;
            public readonly double? PercentileValue;

            internal Metric(string value, string targetProperty = null, double? percentile = null)
            {
                _value = value;
                TargetProperty = targetProperty;
                PercentileValue = percentile;
            }

            public override string ToString() => _value;
            public static implicit operator string(Metric value) => value?.ToString();

            public static Metric Count() => new Metric(IncisiveConstants.QueryCount);
            public static Metric CountUnique(string targetProperty) => new Metric(IncisiveConstants.QueryCountUnique, targetProperty);
            public static Metric Sum(string targetProperty) => new Metric(IncisiveConstants.QuerySum, targetProperty);
            public static Metric Average(string targetProperty) => new Metric(IncisiveConstants.QueryAverage, targetProperty);
            public static Metric Minimum(string targetProperty) => new Metric(IncisiveConstants.QueryMinimum, targetProperty);
            public static Metric Maximum(string targetProperty) => new Metric(IncisiveConstants.QueryMaximum, targetProperty);
            public static Metric Median(string targetProperty) => new Metric(IncisiveConstants.QueryMedian, targetProperty);
            public static Metric Percentile(string targetProperty, double percentile) => new Metric(IncisiveConstants.QueryPercentile, targetProperty, percentile);
            public static Metric SelectUnique(string targetProperty) => new Metric(IncisiveConstants.QuerySelectUnique, targetProperty);
        }

        public string Label { get; private set; }
        public string Analysis { get; private set; }
        public string TargetProperty { get; private set; }
        public double? Percentile { get; private set; }

        public MultiAnalysisParam(string label, Metric analysis)
        {
            Label = label;
            Analysis = analysis;
            TargetProperty = analysis.TargetProperty;
            Percentile = analysis.PercentileValue;
        }

        internal JObject ToJObject()
        {
            var o = new JObject { { "analysis_type", Analysis } };
            if (!string.IsNullOrEmpty(TargetProperty))
                o.Add("target_property", TargetProperty);
            if (Percentile.HasValue)
            {
                var pct = Percentile.Value;
                if (pct == Math.Floor(pct) && Math.Abs(pct) < 1e15)
                    o.Add("percentile", (long)pct);
                else
                    o.Add("percentile", pct);
            }
            return o;
        }
    }

    /// <summary>A group-by result row.</summary>
    public sealed class QueryGroupValue<T>
    {
        public T Value { get; private set; }
        public string Group { get; private set; }

        public QueryGroupValue(T value, string group)
        {
            Value = value;
            Group = group;
        }

        public override bool Equals(object obj) =>
            obj is QueryGroupValue<T> v && EqualityComparer<T>.Default.Equals(Value, v.Value) && Group == v.Group;
        public override int GetHashCode() => EqualityComparer<T>.Default.GetHashCode(Value) * 31 + (Group ?? "").GetHashCode();
        public override string ToString() => Group + ": " + Value;
    }

    /// <summary>A series (interval) result row.</summary>
    public sealed class QueryIntervalValue<T>
    {
        public T Value { get; private set; }
        public DateTime Start { get; private set; }
        public DateTime End { get; private set; }

        public QueryIntervalValue(T value, DateTime start, DateTime end)
        {
            Value = value;
            Start = start;
            End = end;
        }

        public override bool Equals(object obj) =>
            obj is QueryIntervalValue<T> v && EqualityComparer<T>.Default.Equals(Value, v.Value) && Start == v.Start && End == v.End;
        public override int GetHashCode() => (EqualityComparer<T>.Default.GetHashCode(Value) * 31 + Start.GetHashCode()) * 31 + End.GetHashCode();
    }

    /// <summary>One step of a funnel query.</summary>
    public class FunnelStep
    {
        [JsonProperty(PropertyName = "event_collection")]
        public string EventCollection { get; set; }

        [JsonProperty(PropertyName = "actor_property")]
        public string ActorProperty { get; set; }

        [JsonProperty(PropertyName = "filters", NullValueHandling = NullValueHandling.Ignore)]
        public IEnumerable<QueryFilter> Filters { get; set; }

        [JsonProperty(PropertyName = "inverted")]
        public bool Inverted { get; set; }

        [JsonProperty(PropertyName = "optional")]
        public bool Optional { get; set; }

        [JsonProperty(PropertyName = "timeframe", NullValueHandling = NullValueHandling.Ignore)]
        [JsonConverter(typeof(TimeframeConverter))]
        public IQueryTimeframe Timeframe { get; set; }

        [JsonProperty(PropertyName = "timezone", NullValueHandling = NullValueHandling.Ignore)]
        public string TimeZone { get; set; }

        [JsonProperty(PropertyName = "with_actors")]
        public bool WithActors { get; set; }
    }

    /// <summary>Funnel response: per-step counts, echoed steps and optional actors.</summary>
    public class FunnelResult
    {
        [JsonProperty(PropertyName = "actors", NullValueHandling = NullValueHandling.Ignore)]
        public IEnumerable<IEnumerable<dynamic>> Actors { get; set; }

        [JsonProperty(PropertyName = "steps", NullValueHandling = NullValueHandling.Ignore)]
        public IEnumerable<FunnelResultStep> Steps { get; set; }

        [JsonProperty(PropertyName = "result", NullValueHandling = NullValueHandling.Ignore)]
        public IEnumerable<int> Result { get; set; }
    }

    public class FunnelResultStep
    {
        [JsonProperty(PropertyName = "with_actors")]
        public bool WithActors { get; set; }

        [JsonProperty(PropertyName = "actor_property")]
        public string ActorProperty { get; set; }

        [JsonProperty(PropertyName = "filters")]
        public IEnumerable<QueryFilter> Filters { get; set; }

        [JsonProperty(PropertyName = "timeframe")]
        [JsonConverter(typeof(TimeframeConverter))]
        public IQueryTimeframe Timeframe { get; set; }

        [JsonProperty(PropertyName = "timezone")]
        public string TimeZone { get; set; }

        [JsonProperty(PropertyName = "event_collection")]
        public string EventCollection { get; set; }

        [JsonProperty(PropertyName = "optional")]
        public bool Optional { get; set; }

        [JsonProperty(PropertyName = "inverted")]
        public bool Inverted { get; set; }
    }

    /// <summary>Query definition embedded in a cached dataset (serialized snake_case).</summary>
    public class QueryDefinition
    {
        public string ProjectId { get; set; }
        public string AnalysisType { get; set; }
        public string EventCollection { get; set; }
        public string TargetProperty { get; set; }
        public IEnumerable<QueryFilter> Filters { get; set; }
        public string Timeframe { get; set; }
        public string Timezone { get; set; }
        public string Interval { get; set; }
        public IEnumerable<string> GroupBy { get; set; }
    }

    internal static class Fmt
    {
        internal static string Number(double d) => d.ToString("R", CultureInfo.InvariantCulture);
    }
}
