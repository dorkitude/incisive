using System;
using System.Linq;
using System.Threading.Tasks;
using Xunit;

namespace Incisive.Tests
{
    /// <summary>
    /// Runs against a real server only when INCISIVE_BASE_URL and INCISIVE_PROJECT_ID are set
    /// (plus INCISIVE_WRITE_KEY / INCISIVE_READ_KEY / INCISIVE_MASTER_KEY as needed); skipped otherwise.
    /// </summary>
    public class IntegrationTests
    {
        private static IncisiveClient ClientOrSkip(params string[] requiredKeys)
        {
            Skip.If(string.IsNullOrEmpty(Environment.GetEnvironmentVariable("INCISIVE_BASE_URL")) ||
                    string.IsNullOrEmpty(Environment.GetEnvironmentVariable("INCISIVE_PROJECT_ID")),
                "INCISIVE_BASE_URL / INCISIVE_PROJECT_ID not set");
            foreach (var k in requiredKeys)
                Skip.If(string.IsNullOrEmpty(Environment.GetEnvironmentVariable(k)), k + " not set");
            return new IncisiveClient(new ProjectSettingsProviderEnv());
        }

        private static string NewCollection() => "dotnet_it_" + Guid.NewGuid().ToString("N").Substring(0, 12);

        [SkippableFact]
        public async Task AddEventThenCount()
        {
            var client = ClientOrSkip("INCISIVE_WRITE_KEY", "INCISIVE_READ_KEY");
            var collection = NewCollection();

            await client.AddEventAsync(collection, new { price = 10, plan = "pro" });
            await client.AddEventsAsync(collection, new object[] { new { price = 20, plan = "free" }, new { price = 30, plan = "pro" } });

            string count = null;
            for (var attempt = 0; attempt < 20; attempt++)
            {
                count = await client.QueryAsync(QueryType.Count(), collection, null, QueryRelativeTimeframe.ThisNDays(1));
                if (count == "3") break;
                await Task.Delay(500);
            }
            Assert.Equal("3", count);

            var sum = await client.QueryAsync(QueryType.Sum(), collection, "price", QueryRelativeTimeframe.ThisNDays(1));
            Assert.Equal(60.0, double.Parse(sum, System.Globalization.CultureInfo.InvariantCulture));

            var groups = (await client.QueryGroupAsync(QueryType.Count(), collection, null, "plan", QueryRelativeTimeframe.ThisNDays(1))).ToList();
            Assert.Equal("2", groups.Single(g => g.Group == "pro").Value);

            var filtered = await client.QueryAsync(QueryType.Count(), collection, null, QueryRelativeTimeframe.ThisNDays(1),
                new[] { new QueryFilter("price", QueryFilter.FilterOperator.GreaterThan(), 15) });
            Assert.Equal("2", filtered);

            var extracted = (await client.QueryExtractResourceAsync(collection, QueryRelativeTimeframe.ThisNDays(1))).ToList();
            Assert.Equal(3, extracted.Count);

            if (!string.IsNullOrEmpty(Environment.GetEnvironmentVariable("INCISIVE_MASTER_KEY")))
                await client.DeleteCollectionAsync(collection);
        }

        [SkippableFact]
        public async Task SchemasAreListed()
        {
            var client = ClientOrSkip("INCISIVE_WRITE_KEY", "INCISIVE_READ_KEY");
            var collection = NewCollection();
            await client.AddEventAsync(collection, new { a = 1 });
            var schemas = await client.GetSchemasAsync();
            Assert.NotNull(schemas);
        }
    }
}
