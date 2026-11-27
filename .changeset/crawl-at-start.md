---
default: patch
---

# The relays are asked to read the host as it starts

The host asks each relay in `PDS_CRAWLERS` to read its firehose (`com.atproto.sync.requestCrawl`) as it starts, before it writes anything, and every 20 minutes after, whether or not it wrote anything. It asked only after writing an event, which a relay reading the host for the first time never received, so a new host's first group could miss the relays entirely. It logs `asked the relay to crawl` or `requestCrawl failed` for each relay each time.
