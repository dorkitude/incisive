using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Newtonsoft.Json.Linq;

namespace Incisive
{
    /// <summary>An event waiting to be sent (or one that failed in a bulk insert).</summary>
    public class CachedEvent
    {
        public string Collection { get; set; }
        public JObject Event { get; set; }
        public Exception Error { get; set; }

        public CachedEvent(string collection, JObject e, Exception err = null)
        {
            Collection = collection;
            Event = e;
            Error = err;
        }

        public override string ToString()
        {
            return string.Format("CachedEvent:{{\n\"Collection\": \"{0}\",\n\"Event\":{1},\n\"Error\":\"{2}:{3}\"\n}}",
                Collection, Event, Error == null ? "null" : Error.GetType().Name, Error == null ? "" : Error.Message);
        }
    }

    /// <summary>
    /// Storage for events queued by <see cref="IncisiveClient.AddEvent"/> when caching is enabled;
    /// flushed by <see cref="IncisiveClient.SendCachedEvents"/>.
    /// </summary>
    public interface IEventCache
    {
        Task AddAsync(CachedEvent e);
        /// <summary>Remove and return the oldest event, or null when empty.</summary>
        Task<CachedEvent> TryTakeAsync();
        Task ClearAsync();
    }

    /// <summary>In-memory FIFO event cache.</summary>
    public class EventCacheMemory : IEventCache
    {
        private readonly Queue<CachedEvent> _events = new Queue<CachedEvent>();

        public Task AddAsync(CachedEvent e)
        {
            if (null == e)
                throw new IncisiveException("Cached events may not be null");
            lock (_events)
                _events.Enqueue(e);
            return Task.CompletedTask;
        }

        public Task ClearAsync()
        {
            lock (_events)
                _events.Clear();
            return Task.CompletedTask;
        }

        public Task<CachedEvent> TryTakeAsync()
        {
            lock (_events)
                return Task.FromResult(_events.Any() ? _events.Dequeue() : null);
        }

        public int Count
        {
            get { lock (_events) return _events.Count; }
        }
    }
}
