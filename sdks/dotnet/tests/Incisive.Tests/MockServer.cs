using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Newtonsoft.Json.Linq;

namespace Incisive.Tests
{
    /// <summary>A request captured by <see cref="MockServer"/>.</summary>
    public sealed class RecordedRequest
    {
        public string Method { get; set; }
        /// <summary>Raw request target as sent (path + query, still percent-encoded).</summary>
        public string RawUrl { get; set; }
        public string RawPath => RawUrl.Split('?')[0];
        public string RawQuery => RawUrl.Contains('?') ? RawUrl.Substring(RawUrl.IndexOf('?') + 1) : "";
        public Dictionary<string, string> Headers { get; set; }
        public string Body { get; set; }

        /// <summary>Query parameters, percent-decoded.</summary>
        public Dictionary<string, string> Query =>
            RawQuery.Length == 0
                ? new Dictionary<string, string>()
                : RawQuery.Split('&').Select(p => p.Split(new[] { '=' }, 2))
                    .ToDictionary(kv => Uri.UnescapeDataString(kv[0]), kv => kv.Length > 1 ? Uri.UnescapeDataString(kv[1]) : "");

        public JToken QueryJson(string name) => Parse(Query[name]);
        public JToken BodyJson => Parse(Body);

        public static JToken Parse(string s)
        {
            using (var r = new Newtonsoft.Json.JsonTextReader(new StringReader(s)) { DateParseHandling = Newtonsoft.Json.DateParseHandling.None })
                return JToken.ReadFrom(r);
        }
        public string Header(string name) => Headers.TryGetValue(name.ToLowerInvariant(), out var v) ? v : null;
    }

    /// <summary>In-process HTTP server on 127.0.0.1 with a free port; replies with queued responses.</summary>
    public sealed class MockServer : IDisposable
    {
        private readonly HttpListener _listener = new HttpListener();
        private readonly ConcurrentQueue<(int status, string body)> _responses = new ConcurrentQueue<(int, string)>();
        private readonly CancellationTokenSource _cts = new CancellationTokenSource();
        public List<RecordedRequest> Requests { get; } = new List<RecordedRequest>();
        public string BaseUrl { get; }

        public MockServer()
        {
            var port = FreePort();
            BaseUrl = $"http://127.0.0.1:{port}";
            _listener.Prefixes.Add(BaseUrl + "/");
            _listener.Start();
            Task.Run(Loop);
        }

        private static int FreePort()
        {
            var l = new TcpListener(IPAddress.Loopback, 0);
            l.Start();
            var port = ((IPEndPoint)l.LocalEndpoint).Port;
            l.Stop();
            return port;
        }

        public MockServer Reply(int status, string body)
        {
            _responses.Enqueue((status, body));
            return this;
        }

        public MockServer Reply(int status, object body) => Reply(status, JToken.FromObject(body).ToString());

        public RecordedRequest Last
        {
            get { lock (Requests) return Requests.Last(); }
        }

        private async Task Loop()
        {
            while (!_cts.IsCancellationRequested)
            {
                HttpListenerContext ctx;
                try { ctx = await _listener.GetContextAsync(); }
                catch { return; }

                string body;
                using (var r = new StreamReader(ctx.Request.InputStream, Encoding.UTF8))
                    body = await r.ReadToEndAsync();

                var rec = new RecordedRequest
                {
                    Method = ctx.Request.HttpMethod,
                    RawUrl = ctx.Request.RawUrl,
                    Headers = ctx.Request.Headers.AllKeys.ToDictionary(k => k.ToLowerInvariant(), k => ctx.Request.Headers[k]),
                    Body = body,
                };
                lock (Requests) Requests.Add(rec);

                var (status, respBody) = _responses.TryDequeue(out var resp) ? resp : (200, "{}");
                ctx.Response.StatusCode = status;
                if (respBody != null)
                {
                    var bytes = Encoding.UTF8.GetBytes(respBody);
                    ctx.Response.ContentType = "application/json";
                    ctx.Response.ContentLength64 = bytes.Length;
                    await ctx.Response.OutputStream.WriteAsync(bytes, 0, bytes.Length);
                }
                ctx.Response.Close();
            }
        }

        public void Dispose()
        {
            _cts.Cancel();
            try { _listener.Stop(); _listener.Close(); } catch { }
        }
    }
}
