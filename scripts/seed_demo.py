"""Sends synthetic web traffic (30 days, backdated) to a project through the event API.

Usage: uv run scripts/seed_demo.py BASE_URL PROJECT_ID WRITE_KEY [SESSIONS]
"""

import datetime as dt
import json
import random
import sys
import urllib.request
import uuid

base, project, key = sys.argv[1:4]
n_sessions = int(sys.argv[4]) if len(sys.argv) > 4 else 1500
rnd = random.Random(7)

PAGES = ["/", "/pricing", "/docs", "/docs/quickstart", "/blog/launch", "/blog/denormalize-everything", "/signup", "/about", "/careers"]
SOURCES = [
    ("https://www.google.com/", None, 40),
    ("", None, 22),
    ("https://news.ycombinator.com/", None, 10),
    ("https://t.co/abc", None, 6),
    ("https://www.linkedin.com/", None, 5),
    ("https://duckduckgo.com/", None, 4),
    ("https://chatgpt.com/", None, 4),
    ("", {"source": "newsletter", "medium": "email", "campaign": "october"}, 6),
    ("", {"source": "google", "medium": "cpc", "campaign": "brand"}, 3),
]
UAS = [
    ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36", 35),
    ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36", 25),
    ("Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1", 20),
    ("Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Mobile Safari/537.36", 10),
    ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:131.0) Gecko/20100101 Firefox/131.0", 7),
    ("Mozilla/5.0 (iPad; CPU OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1", 3),
]
COUNTRIES = [("United States", "US", 45), ("Germany", "DE", 10), ("United Kingdom", "GB", 9), ("India", "IN", 8), ("Canada", "CA", 6), ("France", "FR", 5), ("Brazil", "BR", 4), ("Japan", "JP", 4)]


def pick(items):
    return rnd.choices(items, weights=[i[-1] for i in items])[0]


def iso(t):
    return t.strftime("%Y-%m-%dT%H:%M:%S.") + f"{t.microsecond // 1000:03d}Z"


now = dt.datetime.now(dt.timezone.utc)
visitors = [str(uuid.uuid4()) for _ in range(int(n_sessions * 0.7))]
batch = {"web_session": [], "web_pageview": [], "signup": []}
for _ in range(n_sessions):
    day = rnd.random() ** 0.8 * 30
    start = now - dt.timedelta(days=day, hours=rnd.random() * 2)
    ref, utm, _ = pick(SOURCES)
    ua, _ = pick(UAS)
    country, code, _ = pick(COUNTRIES)
    landing = rnd.choice(PAGES[:6])
    q = "?" + "&".join(f"utm_{k}={v}" for k, v in utm.items()) if utm else ""
    landing_url = f"https://shop.example{landing}{q}"
    vid = rnd.choice(visitors)
    sess = {"id": str(uuid.uuid4()), "started_at": iso(start), "landing_page": {"url": landing_url, "path": landing}}
    if ref:
        sess["referrer"] = {"url": ref}
    common = {"visitor": {"id": vid}, "user_agent": {"raw": ua}, "geo": {"country": country, "country_code": code}, "browser": {"language": "en-US", "timezone": "America/New_York"}}
    batch["web_session"].append({**common, "session": dict(sess), "incisive": {"timestamp": iso(start)}})
    pages = [landing] + [rnd.choice(PAGES) for _ in range(rnd.choices([0, 1, 2, 3, 5], [45, 25, 15, 10, 5])[0])]
    t = start
    for i, p in enumerate(pages):
        if i:
            t += dt.timedelta(seconds=rnd.randint(8, 240))
        s = dict(sess, pageview_index=i + 1, elapsed_seconds=int((t - start).total_seconds()))
        url = landing_url if i == 0 else f"https://shop.example{p}"
        batch["web_pageview"].append({**common, "session": s, "page": {"url": url, "title": p.strip("/") or "Home"}, "incisive": {"timestamp": iso(t)}})
        if p == "/signup" and rnd.random() < 0.4:
            batch["signup"].append({"visitor": {"id": vid}, "plan": rnd.choice(["free", "pro"]), "incisive": {"timestamp": iso(t)}})


def post(chunk):
    req = urllib.request.Request(f"{base}/1.0/projects/{project}/events", data=json.dumps(chunk).encode(), headers={"Authorization": key, "Content-Type": "application/json"})
    with urllib.request.urlopen(req) as r:
        res = json.load(r)
    bad = [e for evs in res.values() for e in evs if not e["success"]]
    if bad:
        print("rejected:", bad[:3])


for coll, evs in batch.items():
    for i in range(0, len(evs), 1000):
        post({coll: evs[i : i + 1000]})
print({k: len(v) for k, v in batch.items()})
