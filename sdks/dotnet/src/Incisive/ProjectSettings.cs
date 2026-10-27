using System;
using System.IO;
using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace Incisive
{
    /// <summary>Project configuration consumed by <see cref="IncisiveClient"/>.</summary>
    public interface IProjectSettings
    {
        /// <summary>
        /// Absolute API root including the version segment and a trailing slash,
        /// e.g. <c>http://localhost:8080/3.0/</c>. Resource paths are resolved against it.
        /// </summary>
        string ServerUrl { get; }
        string ProjectId { get; }
        string MasterKey { get; }
        string WriteKey { get; }
        string ReadKey { get; }
    }

    /// <summary>Settings supplied directly in code.</summary>
    public class ProjectSettingsProvider : IProjectSettings
    {
        public string ServerUrl { get; protected set; }
        /// <summary>Server address without the API version, as configured.</summary>
        public string BaseUrl { get; protected set; }
        public string ApiVersion { get; protected set; }
        public string ProjectId { get; protected set; }
        public string MasterKey { get; protected set; }
        public string WriteKey { get; protected set; }
        public string ReadKey { get; protected set; }

        /// <param name="projectId">Project id (required).</param>
        /// <param name="masterKey">Master key, needed for deletes and admin operations.</param>
        /// <param name="writeKey">Write key, needed to record events.</param>
        /// <param name="readKey">Read key, needed for queries and schema discovery.</param>
        /// <param name="serverUrl">
        /// Server address, default <c>http://localhost:8080</c>. If it already ends with the
        /// API version segment (e.g. <c>.../3.0/</c>) it is used as-is.
        /// </param>
        /// <param name="apiVersion">API version path segment, default <c>3.0</c>.</param>
        public ProjectSettingsProvider(
            string projectId,
            string masterKey = null,
            string writeKey = null,
            string readKey = null,
            string serverUrl = null,
            string apiVersion = null)
        {
            Initialize(projectId, masterKey, writeKey, readKey, serverUrl, apiVersion);
        }

        protected ProjectSettingsProvider() { }

        protected void Initialize(
            string projectId,
            string masterKey,
            string writeKey,
            string readKey,
            string serverUrl,
            string apiVersion = null)
        {
            if (string.IsNullOrWhiteSpace(projectId))
                throw new IncisiveException("A project id must be provided.");

            if (string.IsNullOrWhiteSpace(masterKey) &&
                string.IsNullOrWhiteSpace(writeKey) &&
                string.IsNullOrWhiteSpace(readKey))
                throw new IncisiveException("A value for a read key, write key, or master key must be provided.");

            ApiVersion = string.IsNullOrWhiteSpace(apiVersion) ? IncisiveConstants.ApiVersion : apiVersion.Trim('/');
            BaseUrl = string.IsNullOrWhiteSpace(serverUrl) ? IncisiveConstants.DefaultBaseUrl : serverUrl.Trim();
            ServerUrl = BuildServerUrl(BaseUrl, ApiVersion);
            ProjectId = projectId;
            MasterKey = NullIfBlank(masterKey);
            WriteKey = NullIfBlank(writeKey);
            ReadKey = NullIfBlank(readKey);
        }

        internal static string BuildServerUrl(string baseUrl, string apiVersion)
        {
            var trimmed = baseUrl.TrimEnd('/');
            if (trimmed.EndsWith("/" + apiVersion, StringComparison.Ordinal))
                return trimmed + "/";
            return trimmed + "/" + apiVersion + "/";
        }

        private static string NullIfBlank(string s) => string.IsNullOrWhiteSpace(s) ? null : s;

        public override string ToString()
        {
            // Never print key values.
            return string.Format("ProjectSettings{{ServerUrl:{0}; ProjectId:{1}; MasterKey:{2}; WriteKey:{3}; ReadKey:{4}}}",
                ServerUrl, ProjectId,
                MasterKey == null ? "unset" : "set",
                WriteKey == null ? "unset" : "set",
                ReadKey == null ? "unset" : "set");
        }
    }

    /// <summary>
    /// Settings read from INCISIVE_PROJECT_ID, INCISIVE_MASTER_KEY, INCISIVE_WRITE_KEY,
    /// INCISIVE_READ_KEY, INCISIVE_BASE_URL and (optionally) INCISIVE_API_VERSION.
    /// </summary>
    public class ProjectSettingsProviderEnv : ProjectSettingsProvider
    {
        public ProjectSettingsProviderEnv()
            : base(Environment.GetEnvironmentVariable(IncisiveConstants.EnvProjectId),
                   masterKey: Environment.GetEnvironmentVariable(IncisiveConstants.EnvMasterKey),
                   writeKey: Environment.GetEnvironmentVariable(IncisiveConstants.EnvWriteKey),
                   readKey: Environment.GetEnvironmentVariable(IncisiveConstants.EnvReadKey),
                   serverUrl: Environment.GetEnvironmentVariable(IncisiveConstants.EnvBaseUrl),
                   apiVersion: Environment.GetEnvironmentVariable(IncisiveConstants.EnvApiVersion))
        {
        }
    }

    /// <summary>
    /// Settings read from a JSON file whose keys are the environment variable names, e.g.
    /// <c>{"INCISIVE_PROJECT_ID": "...", "INCISIVE_WRITE_KEY": "..."}</c>.
    /// </summary>
    public class ProjectSettingsProviderFile : ProjectSettingsProvider
    {
        public ProjectSettingsProviderFile(string filePath)
        {
            JObject json;
            try
            {
                json = JObject.Parse(File.ReadAllText(filePath));
            }
            catch (JsonReaderException ex)
            {
                throw new IncisiveException("Failed to read configuration file.", ex);
            }

            Initialize(
                (string)json[IncisiveConstants.EnvProjectId],
                (string)json[IncisiveConstants.EnvMasterKey],
                (string)json[IncisiveConstants.EnvWriteKey],
                (string)json[IncisiveConstants.EnvReadKey],
                (string)json[IncisiveConstants.EnvBaseUrl],
                (string)json[IncisiveConstants.EnvApiVersion]);
        }
    }
}
