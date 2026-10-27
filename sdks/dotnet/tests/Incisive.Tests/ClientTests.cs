using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Newtonsoft.Json.Linq;
using Xunit;

namespace Incisive.Tests
{
    public class SettingsTests
    {
        [Fact]
        public void DefaultsToLocalhostAndVersion()
        {
            var s = new ProjectSettingsProvider("p1", writeKey: "w");
            Assert.Equal("http://localhost:8080/1.0/", s.ServerUrl);
            Assert.Equal("http://localhost:8080", s.BaseUrl);
            Assert.Equal("1.0", s.ApiVersion);
        }

        [Theory]
        [InlineData("http://h:1", "http://h:1/1.0/")]
        [InlineData("http://h:1/", "http://h:1/1.0/")]
        [InlineData("http://h:1/1.0", "http://h:1/1.0/")]
        [InlineData("http://h:1/1.0/", "http://h:1/1.0/")]
        [InlineData("http://h:1/prefix", "http://h:1/prefix/1.0/")]
        public void ServerUrlNormalization(string input, string expected)
        {
            Assert.Equal(expected, new ProjectSettingsProvider("p", readKey: "r", serverUrl: input).ServerUrl);
        }

        [Fact]
        public void RequiresProjectIdAndAKey()
        {
            Assert.Throws<IncisiveException>(() => new ProjectSettingsProvider(""));
            Assert.Throws<IncisiveException>(() => new ProjectSettingsProvider("p"));
        }

        [Fact]
        public void ToStringDoesNotLeakKeys()
        {
            var s = new ProjectSettingsProvider("p", masterKey: "SECRETMASTER", writeKey: "SECRETWRITE");
            Assert.DoesNotContain("SECRET", s.ToString());
        }

        [Fact]
        public void EnvProviderReadsIncisiveVariables()
        {
            var names = new[] { "INCISIVE_PROJECT_ID", "INCISIVE_MASTER_KEY", "INCISIVE_WRITE_KEY", "INCISIVE_READ_KEY", "INCISIVE_BASE_URL", "INCISIVE_API_VERSION" };
            var saved = names.ToDictionary(n => n, Environment.GetEnvironmentVariable);
            try
            {
                Environment.SetEnvironmentVariable("INCISIVE_PROJECT_ID", "envproj");
                Environment.SetEnvironmentVariable("INCISIVE_MASTER_KEY", "m");
                Environment.SetEnvironmentVariable("INCISIVE_WRITE_KEY", "w");
                Environment.SetEnvironmentVariable("INCISIVE_READ_KEY", "r");
                Environment.SetEnvironmentVariable("INCISIVE_BASE_URL", "http://example.test:9000");
                Environment.SetEnvironmentVariable("INCISIVE_API_VERSION", null);
                var s = new ProjectSettingsProviderEnv();
                Assert.Equal("envproj", s.ProjectId);
                Assert.Equal("m", s.MasterKey);
                Assert.Equal("w", s.WriteKey);
                Assert.Equal("r", s.ReadKey);
                Assert.Equal("http://example.test:9000/1.0/", s.ServerUrl);
            }
            finally
            {
                foreach (var kv in saved) Environment.SetEnvironmentVariable(kv.Key, kv.Value);
            }
        }

        [Fact]
        public void FileProviderReadsJson()
        {
            var path = System.IO.Path.GetTempFileName();
            try
            {
                System.IO.File.WriteAllText(path, "{\"INCISIVE_PROJECT_ID\":\"fp\",\"INCISIVE_READ_KEY\":\"r\",\"INCISIVE_BASE_URL\":\"http://f:1\"}");
                var s = new ProjectSettingsProviderFile(path);
                Assert.Equal("fp", s.ProjectId);
                Assert.Equal("r", s.ReadKey);
                Assert.Equal("http://f:1/1.0/", s.ServerUrl);
            }
            finally { System.IO.File.Delete(path); }
        }
    }

    public abstract class ClientTestBase : IDisposable
    {
        protected readonly MockServer Server = new MockServer();

        protected IncisiveClient Client(string master = "MASTER", string write = "WRITE", string read = "READ", IEventCache cache = null) =>
            new IncisiveClient(new ProjectSettingsProvider("proj 1", master, write, read, Server.BaseUrl), cache);

        public void Dispose() => Server.Dispose();
    }

    public class EventTests : ClientTestBase
    {
        [Fact]
        public async Task AddEventPostsJsonWithWriteKey()
        {
            Server.Reply(201, new { created = true });
            await Client().AddEventAsync("purchases", new { item = "golden widget", price = 12.5 });

            var r = Server.Last;
            Assert.Equal("POST", r.Method);
            Assert.Equal("/1.0/projects/proj%201/events/purchases", r.RawPath);
            Assert.Equal("WRITE", r.Header("Authorization"));
            Assert.StartsWith("application/json", r.Header("Content-Type"));
            Assert.StartsWith("incisive-dotnet/", r.Header("User-Agent"));
            Assert.DoesNotContain(r.Headers.Keys, k => k.EndsWith("-sdk"));

            var body = (JObject)r.BodyJson;
            Assert.Equal("golden widget", (string)body["item"]);
            Assert.Equal(12.5, (double)body["price"]);
            var ts = (string)body["keen"]["timestamp"];
            Assert.EndsWith("Z", ts);
            Assert.True(DateTime.UtcNow - DateTime.Parse(ts).ToUniversalTime() < TimeSpan.FromMinutes(1));
        }

        [Fact]
        public async Task AddEventKeepsExplicitTimestampAndAddsAddOnsAndGlobals()
        {
            Server.Reply(201, new { created = true });
            var c = Client();
            c.AddGlobalProperty("app", "shop");
            var n = 0;
            c.AddGlobalProperty("seq", new DynamicPropertyValue(() => ++n));
            await c.AddEventAsync("visits",
                new Dictionary<string, object> { { "ip", "1.2.3.4" }, { "keen", new { timestamp = "2020-01-01T00:00:00.000Z" } } },
                new[] { AddOn.IpToGeo("ip", "geo"), AddOn.ReferrerParser("ref", "page", "refinfo") });

            var body = (JObject)Server.Last.BodyJson;
            Assert.Equal("shop", (string)body["app"]);
            Assert.Equal(2, (int)body["seq"]); // evaluated once at registration, once per event
            Assert.Equal("2020-01-01T00:00:00.000Z", (string)body["keen"]["timestamp"]);
            var addons = (JArray)body["keen"]["addons"];
            var ns = IncisiveConstants.ReservedNamespace;
            Assert.True(JToken.DeepEquals(RecordedRequest.Parse(
                "[{\"name\":\"" + ns + ":ip_to_geo\",\"input\":{\"ip\":\"ip\"},\"output\":\"geo\"}," +
                "{\"name\":\"" + ns + ":referrer_parser\",\"input\":{\"referrer_url\":\"ref\",\"page_url\":\"page\"},\"output\":\"refinfo\"}]"), addons));
        }

        [Fact]
        public void AddOnOutputMayNotUseReservedNamespace()
        {
            Assert.Throws<IncisiveInvalidPropertyNameException>(() => AddOn.UrlParser("u", IncisiveConstants.ReservedNamespace + ".url"));
        }

        [Fact]
        public async Task CollectionNameIsEscapedInPath()
        {
            Server.Reply(201, new { created = true });
            await Client().AddEventAsync("my events/x?y", new { a = 1 });
            Assert.Equal("/1.0/projects/proj%201/events/my%20events%2Fx%3Fy", Server.Last.RawPath);
        }

        [Fact]
        public async Task CollectionNameValidation()
        {
            var c = Client();
            await Assert.ThrowsAsync<IncisiveException>(() => c.AddEventAsync("_hidden", new { a = 1 }));
            await Assert.ThrowsAsync<IncisiveException>(() => c.AddEventAsync("a$b", new { a = 1 }));
            await Assert.ThrowsAsync<IncisiveException>(() => c.AddEventAsync(new string('x', 65), new { a = 1 }));
            Assert.Empty(Server.Requests);
        }

        [Fact]
        public async Task AddEventRequiresWriteKey()
        {
            var c = Client(write: null);
            await Assert.ThrowsAsync<IncisiveException>(() => c.AddEventAsync("x", new { a = 1 }));
        }

        [Fact]
        public async Task AddEventsPostsBulkBody()
        {
            Server.Reply(200, "{\"purchases\":[{\"success\":true},{\"success\":true}]}");
            await Client().AddEventsAsync("purchases", new object[] { new { n = 1 }, new { n = 2 } });

            var r = Server.Last;
            Assert.Equal("POST", r.Method);
            Assert.Equal("/1.0/projects/proj%201/events", r.RawPath);
            Assert.Equal("WRITE", r.Header("Authorization"));
            var body = (JObject)r.BodyJson;
            var events = (JArray)body["purchases"];
            Assert.Equal(2, events.Count);
            Assert.Equal(1, (int)events[0]["n"]);
            Assert.NotNull(events[1]["keen"]["timestamp"]);
        }

        [Fact]
        public async Task AddEventsReportsRejectedEvents()
        {
            Server.Reply(200, "{\"purchases\":[{\"success\":true},{\"success\":false,\"error\":{\"name\":\"InvalidPropertyNameError\",\"description\":\"bad name\"}}]}");
            var ex = await Assert.ThrowsAsync<IncisiveBulkException>(() =>
                Client().AddEventsAsync("purchases", new object[] { new { n = 1 }, new { n = 2 } }));
            var failed = ex.FailedEvents.Single();
            Assert.Equal("purchases", failed.Collection);
            Assert.Equal(2, (int)failed.Event["n"]);
            var err = Assert.IsType<IncisiveInvalidPropertyNameException>(failed.Error);
            Assert.Equal("bad name", err.Message);
        }

        [Fact]
        public async Task CachedEventsAreBatchedPerCollection()
        {
            var cache = new EventCacheMemory();
            var c = Client(cache: cache);
            await c.AddEventAsync("a", new { i = 1 });
            await c.AddEventAsync("b", new { i = 2 });
            await c.AddEventsAsync("a", new object[] { new { i = 3 } });
            Assert.Empty(Server.Requests);
            Assert.Equal(3, cache.Count);

            Server.Reply(200, "{\"a\":[{\"success\":true},{\"success\":true}]}");
            Server.Reply(200, "{\"b\":[{\"success\":true}]}");
            await c.SendCachedEventsAsync();

            Assert.Equal(2, Server.Requests.Count);
            var bodies = Server.Requests.Select(r => (JObject)r.BodyJson).ToList();
            var a = bodies.Single(b => b["a"] != null);
            Assert.Equal(new[] { 1, 3 }, ((JArray)a["a"]).Select(e => (int)e["i"]));
            Assert.Equal(0, cache.Count);
        }

        [Fact]
        public async Task GetSchemasAndSchema()
        {
            Server.Reply(200, "[{\"name\":\"purchases\",\"properties\":{\"price\":\"num\"}}]");
            Server.Reply(200, "{\"properties\":{\"price\":\"num\"}}");
            var c = Client();
            JArray all = await c.GetSchemasAsync();
            Assert.Equal("purchases", (string)all[0]["name"]);
            Assert.Equal("GET", Server.Requests[0].Method);
            Assert.Equal("/1.0/projects/proj%201/events", Server.Requests[0].RawPath);
            Assert.Equal("READ", Server.Requests[0].Header("Authorization"));

            dynamic one = await c.GetSchemaAsync("purchases");
            Assert.Equal("num", (string)one.properties.price);
            Assert.Equal("/1.0/projects/proj%201/events/purchases", Server.Requests[1].RawPath);
        }

        [Fact]
        public void DeleteCollectionUsesMasterKey()
        {
            Server.Reply(204, (string)null);
            Client().DeleteCollection("old");
            var r = Server.Last;
            Assert.Equal("DELETE", r.Method);
            Assert.Equal("/1.0/projects/proj%201/events/old", r.RawPath);
            Assert.Equal("MASTER", r.Header("Authorization"));
            Assert.Throws<IncisiveException>(() => Client(master: null).DeleteCollection("old"));
        }
    }

    public class ErrorTests : ClientTestBase
    {
        [Fact]
        public async Task ErrorCodeMapsToTypedException()
        {
            Server.Reply(401, "{\"message\":\"bad key\",\"error_code\":\"InvalidApiKeyError\"}");
            var ex = await Assert.ThrowsAsync<IncisiveInvalidApiKeyException>(() =>
                Client().QueryAsync(QueryType.Count(), "x", null));
            Assert.Equal("bad key", ex.Message);
            Assert.Equal("InvalidApiKeyError", ex.ErrorCode);
            Assert.Equal(401, ex.StatusCode);
        }

        [Theory]
        [InlineData("ResourceNotFoundError", typeof(IncisiveResourceNotFoundException))]
        [InlineData("NamespaceTypeError", typeof(IncisiveNamespaceTypeException))]
        [InlineData("InvalidEventError", typeof(IncisiveInvalidEventException))]
        [InlineData("ListsOfNonPrimitivesNotAllowedError", typeof(IncisiveListsOfNonPrimitivesNotAllowedException))]
        [InlineData("InvalidBatchError", typeof(IncisiveInvalidBatchException))]
        [InlineData("InternalServerError", typeof(IncisiveInternalServerErrorException))]
        [InlineData("InvalidPropertyNameError", typeof(IncisiveInvalidPropertyNameException))]
        public async Task ErrorCodeTable(string code, Type expected)
        {
            Server.Reply(400, new { message = "m", error_code = code });
            var ex = await Assert.ThrowsAnyAsync<IncisiveException>(() => Client().AddEventAsync("x", new { a = 1 }));
            Assert.IsType(expected, ex);
            Assert.Equal(400, ex.StatusCode);
        }

        [Fact]
        public async Task ReservedNamespacePropertyError()
        {
            Server.Reply(400, new { message = "m", error_code = IncisiveUtil.InvalidReservedNamespacePropertyCode });
            await Assert.ThrowsAsync<IncisiveInvalidReservedNamespacePropertyException>(() => Client().AddEventAsync("x", new { a = 1 }));
        }

        [Fact]
        public async Task UnknownErrorCodeUsesBaseType()
        {
            Server.Reply(400, "{\"message\":\"nope\",\"error_code\":\"WeirdError\"}");
            var ex = await Assert.ThrowsAsync<IncisiveException>(() => Client().AddEventAsync("x", new { a = 1 }));
            Assert.Equal("WeirdError : nope", ex.Message);
            Assert.Equal("WeirdError", ex.ErrorCode);
        }

        [Fact]
        public async Task NonJsonErrorReportsStatus()
        {
            Server.Reply(502, "<html>bad gateway</html>");
            var ex = await Assert.ThrowsAsync<IncisiveException>(() => Client().QueryAsync(QueryType.Count(), "x", null));
            Assert.Equal(502, ex.StatusCode);
            Assert.Contains("502", ex.Message);
            Assert.Null(ex.ErrorCode);
        }

        [Fact]
        public void SyncWrappersThrowUnwrappedExceptions()
        {
            Server.Reply(404, "{\"message\":\"no such\",\"error_code\":\"ResourceNotFoundError\"}");
            Assert.Throws<IncisiveResourceNotFoundException>(() => Client().GetSchema("missing"));
        }
    }

    public class ScopedKeyTests
    {
        private const string K32 = "0123456789ABCDEF0123456789ABCDEF";
        private const string K64 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        private const string Iv = "000102030405060708090A0B0C0D0E0F";
        private const string Json = "{\"filters\":[{\"property_name\":\"account_id\",\"operator\":\"eq\",\"property_value\":123}],\"allowed_operations\":[\"read\"]}";

        // Produced by running the upstream .NET library (v0.4.0) with the inputs above.
        private const string Upstream32 = "000102030405060708090A0B0C0D0E0FEED92D12F0C141024A7766636EC2E298B0D84700A87DF8F6F13AEA7BC9AD553D4E864254F41E8AAE270A3DEDF6185A60F7FC5111D5A52919CD862248060C0E47C9AFFFD48195518589B81D2327B91F151FF05CABDA213B56D30726A625B18B7925FC4372275DB661AFA03E3CC6D02416";
        private const string Upstream64 = "000102030405060708090A0B0C0D0E0F515EE9C21BB99220C296C8901577948BA3A93158683514BC85536116D3DE088B638EBBC8FEDB47ED9BAC2B53CD53D06AC54C7668922AAC2B8971279CF7E0B8A819D4280C9042B2586E22032EAEF54CEEB2EEDABE7C8D71D2DAE4A4921D77F77219B8DAA4512C75A1775310017B270B22";
        private const string Upstream32RandomIv = "3068C17DC8BE983F67F4DB2AE1F24364379B29DF455EA723F073EEE7A52558D1B231047CD794CF2B92ECCA005C0871416264EB15E718EFE7A2E0CD0304B1B61D";
        private const string UpstreamObject64 = "FFEEDDCCBBAA99887766554433221100EA147EB9A1AB3E57A299482E744E761B6B4C0D4E0E432CBDD572E6E1EFC3CBA2B64E44BBE6958201DF6E33C8FE575C64";

        [Fact]
        public void EncryptMatchesUpstreamVectors()
        {
            Assert.Equal(Upstream32, ScopedKey.EncryptString(K32, Json, Iv));
            Assert.Equal(Upstream64, ScopedKey.EncryptString(K64, Json, Iv));
            Assert.Equal(UpstreamObject64, ScopedKey.Encrypt(K64, new { allowed_operations = new[] { "read" }, timestamp = 5 }, "FFEEDDCCBBAA99887766554433221100"));
        }

        [Fact]
        public void DecryptsUpstreamVectors()
        {
            Assert.Equal(Json, ScopedKey.Decrypt(K32, Upstream32));
            Assert.Equal(Json, ScopedKey.Decrypt(K64, Upstream64));
            Assert.Equal("{\"allowed_operations\":[\"write\"]}", ScopedKey.Decrypt(K32, Upstream32RandomIv));
            Assert.Equal(Json, ScopedKey.Decrypt(K64, Upstream64.ToLowerInvariant()));
        }

        [Fact]
        public void RoundTripsWithRandomIv()
        {
            var a = ScopedKey.Encrypt(K32, new { allowed_operations = new[] { "read" } });
            var b = ScopedKey.Encrypt(K32, new { allowed_operations = new[] { "read" } });
            Assert.NotEqual(a, b);
            Assert.Equal("{\"allowed_operations\":[\"read\"]}", ScopedKey.Decrypt(K32, a));
            Assert.Matches("^[0-9A-F]+$", a);
        }

        [Fact]
        public void RejectsBadKeysAndIvs()
        {
            Assert.Throws<IncisiveException>(() => ScopedKey.Encrypt(null, new { x = 1 }));
            Assert.Throws<IncisiveException>(() => ScopedKey.Encrypt("short", new { x = 1 }));
            Assert.Throws<IncisiveException>(() => ScopedKey.EncryptString(K32, "{}", "00"));
            Assert.Throws<IncisiveException>(() => ScopedKey.Decrypt(K64, Upstream32));
        }
    }
}
