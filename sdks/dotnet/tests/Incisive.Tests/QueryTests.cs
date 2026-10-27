using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Newtonsoft.Json.Linq;
using Xunit;

namespace Incisive.Tests
{
    public class QueryTests : ClientTestBase
    {
        private static void AssertJson(string expected, JToken actual) =>
            Assert.True(JToken.DeepEquals(RecordedRequest.Parse(expected), actual), $"expected {expected} got {actual}");

        [Fact]
        public async Task CountSendsGetWithQueryParams()
        {
            Server.Reply(200, "{\"result\": 42}");
            var filters = new[]
            {
                new QueryFilter("price", QueryFilter.FilterOperator.GreaterThan(), 1.5),
                new QueryFilter("name", QueryFilter.FilterOperator.Equals(), "a b&c"),
            };
            var result = await Client().QueryAsync(QueryType.Count(), "purchases", null,
                QueryRelativeTimeframe.ThisNDays(7), filters, "UTC");

            Assert.Equal("42", result);
            var r = Server.Last;
            Assert.Equal("GET", r.Method);
            Assert.Equal("/1.0/projects/proj%201/queries/count", r.RawPath);
            Assert.Equal("MASTER", r.Header("Authorization"));
            Assert.Equal("", r.Body);
            var q = r.Query;
            Assert.Equal("purchases", q["event_collection"]);
            Assert.Equal("this_7_days", q["timeframe"]);
            Assert.Equal("UTC", q["timezone"]);
            Assert.False(q.ContainsKey("target_property"));
            AssertJson("[{\"property_name\":\"price\",\"operator\":\"gt\",\"property_value\":1.5}," +
                       "{\"property_name\":\"name\",\"operator\":\"eq\",\"property_value\":\"a b&c\"}]", r.QueryJson("filters"));
            // RFC 3986 escaping: no raw spaces or '+' for spaces.
            Assert.DoesNotContain(" ", r.RawQuery);
            Assert.Contains("a%20b%26c", r.RawQuery);
        }

        [Fact]
        public async Task ReadKeyUsedWhenNoMasterKey()
        {
            Server.Reply(200, "{\"result\": 1}");
            await Client(master: null).QueryAsync(QueryType.Sum(), "p", "price");
            Assert.Equal("READ", Server.Last.Header("Authorization"));
            Assert.Equal("price", Server.Last.Query["target_property"]);
            Assert.False(Server.Last.Query.ContainsKey("timeframe"));
            Assert.False(Server.Last.Query.ContainsKey("filters"));
        }

        [Fact]
        public async Task AbsoluteTimeframeIsCompactJson()
        {
            Server.Reply(200, "{\"result\": 3.25}");
            var tf = new QueryAbsoluteTimeframe(new DateTime(2024, 1, 1, 0, 0, 0, DateTimeKind.Utc), new DateTime(2024, 2, 1, 0, 0, 0, DateTimeKind.Utc));
            var res = await Client().QueryAsync(QueryType.Average(), "p", "price", tf);
            Assert.Equal("3.25", res);
            Assert.Equal("{\"start\":\"2024-01-01T00:00:00Z\",\"end\":\"2024-02-01T00:00:00Z\"}", Server.Last.Query["timeframe"]);
        }

        [Theory]
        [InlineData("count_unique")]
        [InlineData("minimum")]
        [InlineData("maximum")]
        [InlineData("median")]
        [InlineData("standard_deviation")]
        public async Task AnalysisRoutes(string analysis)
        {
            var qt = new Dictionary<string, QueryType>
            {
                ["count_unique"] = QueryType.CountUnique(),
                ["minimum"] = QueryType.Minimum(),
                ["maximum"] = QueryType.Maximum(),
                ["median"] = QueryType.Median(),
                ["standard_deviation"] = QueryType.StandardDeviation(),
            }[analysis];
            Server.Reply(200, "{\"result\": 7}");
            Assert.Equal("7", await Client().QueryAsync(qt, "p", "x"));
            Assert.Equal("/1.0/projects/proj%201/queries/" + analysis, Server.Last.RawPath);
        }

        [Fact]
        public async Task PercentileSendsPercentileParam()
        {
            Server.Reply(200, "{\"result\": 99.5}");
            var res = await Client().QueryAsync(QueryType.Percentile(90), "p", "latency");
            Assert.Equal("99.5", res);
            Assert.Equal("/1.0/projects/proj%201/queries/percentile", Server.Last.RawPath);
            Assert.Equal("90", Server.Last.Query["percentile"]);
            Assert.Equal("latency", Server.Last.Query["target_property"]);
        }

        [Fact]
        public async Task SelectUniqueJoinsValues()
        {
            Server.Reply(200, "{\"result\": [\"a\", \"b\", 3]}");
            Assert.Equal("a,b,3", await Client().QueryAsync(QueryType.SelectUnique(), "p", "x"));
        }

        [Fact]
        public async Task GroupByParsesRows()
        {
            Server.Reply(200, "{\"result\":[{\"platform\":\"ios\",\"result\":3},{\"platform\":\"web\",\"result\":5}]}");
            var rows = (await Client().QueryGroupAsync(QueryType.Count(), "p", null, "platform")).ToList();
            Assert.Equal("platform", Server.Last.Query["group_by"]);
            Assert.Equal(new[] { new QueryGroupValue<string>("3", "ios"), new QueryGroupValue<string>("5", "web") }, rows);
        }

        [Fact]
        public async Task IntervalParsesTimeframes()
        {
            Server.Reply(200, "{\"result\":[{\"timeframe\":{\"start\":\"2024-01-01T00:00:00.000Z\",\"end\":\"2024-01-02T00:00:00.000Z\"},\"value\":4}]}");
            var rows = (await Client().QueryIntervalAsync(QueryType.Count(), "p", null,
                QueryRelativeTimeframe.PreviousNDays(1), QueryInterval.Daily())).ToList();
            Assert.Equal("daily", Server.Last.Query["interval"]);
            Assert.Equal("previous_1_days", Server.Last.Query["timeframe"]);
            var row = Assert.Single(rows);
            Assert.Equal("4", row.Value);
            Assert.Equal(new DateTime(2024, 1, 1, 0, 0, 0, DateTimeKind.Utc), row.Start);
            Assert.Equal(DateTimeKind.Utc, row.Start.Kind);
            Assert.Equal(new DateTime(2024, 1, 2, 0, 0, 0, DateTimeKind.Utc), row.End);
        }

        [Fact]
        public async Task IntervalGroupParsesNestedRows()
        {
            Server.Reply(200, "{\"result\":[{\"timeframe\":{\"start\":\"2024-01-01T00:00:00Z\",\"end\":\"2024-01-02T00:00:00Z\"}," +
                              "\"value\":[{\"plan\":\"pro\",\"result\":[\"x\",\"y\"]}]}]}");
            var rows = (await Client().QueryIntervalGroupAsync(QueryType.SelectUnique(), "p", "user", "plan",
                QueryRelativeTimeframe.ThisWeek(), QueryInterval.EveryNHours(6))).ToList();
            Assert.Equal("every_6_hours", Server.Last.Query["interval"]);
            var g = Assert.Single(Assert.Single(rows).Value);
            Assert.Equal("pro", g.Group);
            Assert.Equal("x,y", g.Value);
        }

        [Fact]
        public async Task ExtractionSendsLatestAndEmail()
        {
            Server.Reply(200, "{\"result\":[{\"a\":1},{\"a\":2}]}");
            var rows = (await Client().QueryExtractResourceAsync("p", QueryRelativeTimeframe.ThisDay(), latest: 10)).ToList();
            Assert.Equal("/1.0/projects/proj%201/queries/extraction", Server.Last.RawPath);
            Assert.Equal("10", Server.Last.Query["latest"]);
            Assert.False(Server.Last.Query.ContainsKey("email"));
            Assert.Equal(2, rows.Count);
            Assert.Equal(2, (int)rows[1].a);

            Server.Reply(200, "{\"result\":\"Processing. Check the specified email for the extraction results.\"}");
            var none = await Client().QueryExtractResourceAsync("p", email: "me@example.com");
            Assert.Empty(none);
            Assert.Equal("me@example.com", Server.Last.Query["email"]);
        }

        [Fact]
        public async Task FunnelSendsStepsJson()
        {
            Server.Reply(200, "{\"result\":[10,4],\"steps\":[{\"event_collection\":\"signup\",\"actor_property\":\"uid\",\"timeframe\":\"this_7_days\",\"optional\":false,\"inverted\":false,\"with_actors\":false}," +
                              "{\"event_collection\":\"buy\",\"actor_property\":\"uid\",\"timeframe\":{\"start\":\"2024-01-01T00:00:00Z\",\"end\":\"2024-01-02T00:00:00Z\"},\"optional\":true,\"inverted\":false,\"with_actors\":true}]," +
                              "\"actors\":[[\"u1\"],[\"u2\"]]}");
            var steps = new[]
            {
                new FunnelStep { EventCollection = "signup", ActorProperty = "uid", Timeframe = QueryRelativeTimeframe.ThisNDays(7) },
                new FunnelStep
                {
                    EventCollection = "buy", ActorProperty = "uid", Optional = true, WithActors = true,
                    Filters = new[] { new QueryFilter("price", QueryFilter.FilterOperator.Exists(), true) },
                    Timeframe = new QueryAbsoluteTimeframe(new DateTime(2024, 1, 1, 0, 0, 0, DateTimeKind.Utc), new DateTime(2024, 1, 2, 0, 0, 0, DateTimeKind.Utc)),
                },
            };
            var res = await Client().QueryFunnelAsync(steps, timezone: "US/Pacific");

            var r = Server.Last;
            Assert.Equal("/1.0/projects/proj%201/queries/funnel", r.RawPath);
            Assert.Equal("US/Pacific", r.Query["timezone"]);
            Assert.False(r.Query.ContainsKey("timeframe"));
            AssertJson("[{\"event_collection\":\"signup\",\"actor_property\":\"uid\",\"inverted\":false,\"optional\":false,\"timeframe\":\"this_7_days\",\"with_actors\":false}," +
                       "{\"event_collection\":\"buy\",\"actor_property\":\"uid\",\"filters\":[{\"property_name\":\"price\",\"operator\":\"exists\",\"property_value\":true}]," +
                       "\"inverted\":false,\"optional\":true,\"timeframe\":{\"start\":\"2024-01-01T00:00:00Z\",\"end\":\"2024-01-02T00:00:00Z\"},\"with_actors\":true}]",
                       r.QueryJson("steps"));
            Assert.Equal(new[] { 10, 4 }, res.Result);
            Assert.Equal(2, res.Steps.Count());
            Assert.Equal(QueryRelativeTimeframe.ThisNDays(7), res.Steps.First().Timeframe);
            Assert.IsType<QueryAbsoluteTimeframe>(res.Steps.Last().Timeframe);
            Assert.Equal(2, res.Actors.Count());
        }

        private static readonly MultiAnalysisParam[] Analyses =
        {
            new MultiAnalysisParam("total", MultiAnalysisParam.Metric.Count()),
            new MultiAnalysisParam("revenue", MultiAnalysisParam.Metric.Sum("price")),
            new MultiAnalysisParam("p95", MultiAnalysisParam.Metric.Percentile("price", 95)),
        };

        [Fact]
        public async Task MultiAnalysisSendsAnalysesJson()
        {
            Server.Reply(200, "{\"result\":{\"total\":3,\"revenue\":30.5,\"p95\":12}}");
            var res = await Client().QueryMultiAnalysisAsync("p", Analyses, QueryRelativeTimeframe.ThisMonth());
            Assert.Equal("/1.0/projects/proj%201/queries/multi_analysis", Server.Last.RawPath);
            AssertJson("{\"total\":{\"analysis_type\":\"count\"},\"revenue\":{\"analysis_type\":\"sum\",\"target_property\":\"price\"}," +
                       "\"p95\":{\"analysis_type\":\"percentile\",\"target_property\":\"price\",\"percentile\":95}}", Server.Last.QueryJson("analyses"));
            Assert.Equal("3", res["total"]);
            Assert.Equal("30.5", res["revenue"]);
        }

        [Fact]
        public async Task MultiAnalysisGroupAndInterval()
        {
            var c = Client();
            Server.Reply(200, "{\"result\":[{\"plat\":\"ios\",\"total\":1,\"revenue\":2}]}");
            var g = Assert.Single(await c.QueryMultiAnalysisGroupAsync("p", Analyses, groupBy: "plat"));
            Assert.Equal("plat", Server.Last.Query["group_by"]);
            Assert.Equal("ios", g.Group);
            Assert.Equal("2", g.Value["revenue"]);
            Assert.False(g.Value.ContainsKey("plat"));

            Server.Reply(200, "{\"result\":[{\"timeframe\":{\"start\":\"2024-01-01T00:00:00Z\",\"end\":\"2024-01-02T00:00:00Z\"},\"value\":{\"total\":9}}]}");
            var i = Assert.Single(await c.QueryMultiAnalysisIntervalAsync("p", Analyses, QueryRelativeTimeframe.ThisNDays(1), QueryInterval.Hourly()));
            Assert.Equal("hourly", Server.Last.Query["interval"]);
            Assert.Equal("9", i.Value["total"]);

            Server.Reply(200, "{\"result\":[{\"timeframe\":{\"start\":\"2024-01-01T00:00:00Z\",\"end\":\"2024-01-02T00:00:00Z\"},\"value\":[{\"plat\":\"web\",\"total\":5}]}]}");
            var ig = Assert.Single(await c.QueryMultiAnalysisIntervalGroupAsync("p", Analyses, QueryRelativeTimeframe.ThisNDays(1), QueryInterval.Daily(), groupBy: "plat"));
            var inner = Assert.Single(ig.Value);
            Assert.Equal("web", inner.Group);
            Assert.Equal("5", inner.Value["total"]);
        }

        [Fact]
        public async Task AvailableQueriesAndRawQuery()
        {
            Server.Reply(200, "{\"count_url\":\"/1.0/projects/p/queries/count\"}");
            var avail = (await Client().GetQueriesAsync()).ToList();
            Assert.Equal("/1.0/projects/proj%201/queries", Server.Last.RawPath);
            Assert.Equal("count_url", avail.Single().Key);

            Server.Reply(200, "{\"result\":5,\"extra\":true}");
            var raw = await Client().QueryAsync("count", new Dictionary<string, string> { { "event_collection", "p" }, { "timeframe", "this_day" } });
            Assert.Equal(5, (int)raw["result"]);
            Assert.Equal("this_day", Server.Last.Query["timeframe"]);
        }
    }

    public class SavedQueryAndDatasetTests : ClientTestBase
    {
        [Fact]
        public async Task SavedQueryCrud()
        {
            var c = Client();
            Server.Reply(200, "[{\"query_name\":\"q1\"}]");
            var list = await c.ListSavedQueriesAsync();
            Assert.Equal("GET", Server.Last.Method);
            Assert.Equal("/1.0/projects/proj%201/queries/saved", Server.Last.RawPath);
            Assert.Equal("MASTER", Server.Last.Header("Authorization"));
            Assert.Equal("q1", (string)list[0]["query_name"]);

            Server.Reply(201, "{\"query_name\":\"my q\"}");
            await c.CreateSavedQueryAsync("my q", new { refresh_rate = 0, query = new { analysis_type = "count", event_collection = "p", timeframe = "this_7_days" } });
            Assert.Equal("PUT", Server.Last.Method);
            Assert.Equal("/1.0/projects/proj%201/queries/saved/my%20q", Server.Last.RawPath);
            Assert.StartsWith("application/json", Server.Last.Header("Content-Type"));
            Assert.True(JToken.DeepEquals(RecordedRequest.Parse("{\"refresh_rate\":0,\"query\":{\"analysis_type\":\"count\",\"event_collection\":\"p\",\"timeframe\":\"this_7_days\"}}"), Server.Last.BodyJson));

            Server.Reply(200, "{\"query_name\":\"q\"}");
            await c.CreateCachedQueryAsync("q", new { analysis_type = "sum", event_collection = "p", target_property = "x" }, 3600);
            Assert.Equal(3600, (int)Server.Last.BodyJson["refresh_rate"]);
            Assert.Equal("sum", (string)Server.Last.BodyJson["query"]["analysis_type"]);

            Server.Reply(200, "{\"query_name\":\"q\",\"refresh_rate\":0}");
            var def = await c.GetSavedQueryAsync("q");
            Assert.Equal("q", (string)def["query_name"]);
            Assert.Equal("/1.0/projects/proj%201/queries/saved/q", Server.Last.RawPath);

            Server.Reply(200, "{\"result\":12}");
            var res = await Client(master: null).GetSavedQueryResultAsync("q");
            Assert.Equal("/1.0/projects/proj%201/queries/saved/q/result", Server.Last.RawPath);
            Assert.Equal("READ", Server.Last.Header("Authorization"));
            Assert.Equal(12, (int)res["result"]);

            Server.Reply(204, (string)null);
            await c.DeleteSavedQueryAsync("q");
            Assert.Equal("DELETE", Server.Last.Method);
            Assert.Equal("/1.0/projects/proj%201/queries/saved/q", Server.Last.RawPath);

            await Assert.ThrowsAsync<IncisiveException>(() => Client(master: null).DeleteSavedQueryAsync("q"));
        }

        [Fact]
        public async Task CreateDatasetPutsSnakeCaseBody()
        {
            Server.Reply(201, "{\"dataset_name\":\"ds\",\"display_name\":\"DS\",\"index_by\":[\"cust\"],\"milliseconds_behind\":5," +
                              "\"last_scheduled_date\":\"2024-01-01T00:00:00.000Z\",\"query\":{\"analysis_type\":\"count\",\"event_collection\":\"p\",\"timeframe\":\"this_30_days\",\"interval\":\"daily\"}}");
            var ds = new DatasetDefinition
            {
                DatasetName = "ds",
                DisplayName = "DS",
                IndexBy = new[] { "cust" },
                Query = new QueryDefinition
                {
                    AnalysisType = "count",
                    EventCollection = "p",
                    Timeframe = "this_30_days",
                    Interval = "daily",
                    GroupBy = new[] { "plan" },
                    Filters = new[] { new QueryFilter("x", QueryFilter.FilterOperator.NotEqual(), 0) },
                },
            };
            var created = await Client().CreateDatasetAsync(ds);
            var r = Server.Last;
            Assert.Equal("PUT", r.Method);
            Assert.Equal("/1.0/projects/proj%201/datasets/ds", r.RawPath);
            Assert.Equal("MASTER", r.Header("Authorization"));
            Assert.True(JToken.DeepEquals(RecordedRequest.Parse(
                "{\"dataset_name\":\"ds\",\"display_name\":\"DS\",\"query\":{\"analysis_type\":\"count\",\"event_collection\":\"p\"," +
                "\"filters\":[{\"property_name\":\"x\",\"operator\":\"ne\",\"property_value\":0}],\"timeframe\":\"this_30_days\",\"interval\":\"daily\",\"group_by\":[\"plan\"]}," +
                "\"index_by\":[\"cust\"]}"), r.BodyJson), r.Body);
            Assert.Equal(5, created.MillisecondsBehind);
            Assert.Equal(new DateTime(2024, 1, 1, 0, 0, 0, DateTimeKind.Utc), created.LastScheduledDate.Value.ToUniversalTime());
        }

        [Fact]
        public async Task DatasetReadsAndDelete()
        {
            var c = Client();
            Server.Reply(200, "{\"result\":[{\"value\":1}]}");
            var res = await c.QueryDatasetAsync("ds", "cust 1", "this_7_days");
            Assert.Equal("/1.0/projects/proj%201/datasets/ds/results", Server.Last.RawPath);
            Assert.Equal("READ", Server.Last.Header("Authorization"));
            Assert.Equal("cust 1", Server.Last.Query["index_by"]);
            Assert.Equal("this_7_days", Server.Last.Query["timeframe"]);
            Assert.NotNull(res["result"]);

            Server.Reply(200, "{\"dataset_name\":\"ds\",\"display_name\":\"DS\"}");
            Assert.Equal("DS", (await c.GetDatasetDefinitionAsync("ds")).DisplayName);

            Server.Reply(200, "{\"datasets\":[{\"dataset_name\":\"a\"}],\"next_page_url\":\"/x\",\"count\":2}");
            var page = await c.ListDatasetDefinitionsAsync(1, "z&y");
            Assert.Equal("/1.0/projects/proj%201/datasets", Server.Last.RawPath);
            Assert.Equal("1", Server.Last.Query["limit"]);
            Assert.Equal("z&y", Server.Last.Query["after_name"]);
            Assert.Equal(2, page.Count);
            Assert.Equal("/x", page.NextPageUrl);

            Server.Reply(200, "{\"datasets\":[{\"dataset_name\":\"a\"}],\"count\":2}");
            Server.Reply(200, "{\"datasets\":[{\"dataset_name\":\"b\"}],\"count\":2}");
            var all = (await c.ListAllDatasetDefinitionsAsync()).Select(d => d.DatasetName).ToList();
            Assert.Equal(new[] { "a", "b" }, all);
            Assert.Equal("100", Server.Requests[Server.Requests.Count - 2].Query["limit"]);
            Assert.Equal("a", Server.Last.Query["after_name"]);

            Server.Reply(204, (string)null);
            await c.DeleteDatasetAsync("ds");
            Assert.Equal("DELETE", Server.Last.Method);
            Assert.Equal("MASTER", Server.Last.Header("Authorization"));
        }
    }
}
