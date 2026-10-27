using System;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Text;
using System.Threading.Tasks;

namespace Incisive
{
    /// <summary>Raw HTTP response (status and body text).</summary>
    public sealed class ApiResponse
    {
        public int StatusCode { get; }
        public string Body { get; }
        public bool IsSuccessStatusCode => StatusCode >= 200 && StatusCode < 300;

        public ApiResponse(int statusCode, string body)
        {
            StatusCode = statusCode;
            Body = body ?? "";
        }
    }

    /// <summary>
    /// Sends requests relative to the configured API root, authenticating with the
    /// <c>Authorization</c> header (the raw key, no scheme).
    /// </summary>
    internal sealed class Transport
    {
        private static readonly Lazy<HttpClient> SharedClient =
            new Lazy<HttpClient>(() => new HttpClient());

        private readonly HttpClient _http;
        private readonly Uri _apiRoot;
        private readonly string _projectPath;

        internal Transport(IProjectSettings settings, HttpMessageHandler handler)
        {
            if (string.IsNullOrWhiteSpace(settings.ServerUrl) ||
                !Uri.TryCreate(settings.ServerUrl, UriKind.Absolute, out var root))
                throw new IncisiveException("A properly formatted ServerUrl must be provided via Project Settings.");

            var s = root.ToString();
            _apiRoot = s.EndsWith("/", StringComparison.Ordinal) ? root : new Uri(s + "/");
            _projectPath = "projects/" + IncisiveUtil.EscapePath(settings.ProjectId) + "/";
            _http = handler == null ? SharedClient.Value : new HttpClient(handler, disposeHandler: false);
        }

        /// <summary>Path relative to the project, e.g. <c>events/purchases</c>.</summary>
        internal string ProjectUrl(string relative) => _projectPath + relative;

        internal async Task<ApiResponse> SendAsync(HttpMethod method, string relativeUrl, string authKey, string jsonBody = null)
        {
            if (string.IsNullOrWhiteSpace(authKey))
                throw new ArgumentNullException(nameof(authKey), "Auth key is required.");

            using (var request = new HttpRequestMessage(method, new Uri(_apiRoot, relativeUrl)))
            {
                request.Headers.TryAddWithoutValidation("Authorization", authKey);
                request.Headers.TryAddWithoutValidation("User-Agent", "incisive-dotnet/" + IncisiveConstants.SdkVersion);
                request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
                if (jsonBody != null)
                {
                    var content = new ByteArrayContent(Encoding.UTF8.GetBytes(jsonBody));
                    content.Headers.ContentType = new MediaTypeHeaderValue("application/json");
                    request.Content = content;
                }

                using (var response = await _http.SendAsync(request).ConfigureAwait(false))
                {
                    var body = response.Content == null
                        ? ""
                        : await response.Content.ReadAsStringAsync().ConfigureAwait(false);
                    return new ApiResponse((int)response.StatusCode, body);
                }
            }
        }

        internal Task<ApiResponse> GetAsync(string url, string key) => SendAsync(HttpMethod.Get, url, key);
        internal Task<ApiResponse> DeleteAsync(string url, string key) => SendAsync(HttpMethod.Delete, url, key);
        internal Task<ApiResponse> PostAsync(string url, string key, string body) => SendAsync(HttpMethod.Post, url, key, body);
        internal Task<ApiResponse> PutAsync(string url, string key, string body) => SendAsync(HttpMethod.Put, url, key, body);
    }
}
