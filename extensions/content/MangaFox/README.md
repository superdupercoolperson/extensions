# MangaFox

Original Rust implementation of [MangaFox](https://fanfox.net) and its public [mobile reader](https://m.fanfox.net). No third-party extension code is imported.

Supports directory discovery in site order, title search, series metadata, chapter lists including decimal chapters, and image reading. It uses the mobile reader's explicit image URL rather than evaluating the desktop reader's packed JavaScript.

Page lists contain stable mobile reader URLs. imagePageContent resolves only the requested page and brokers the actual image using the page's Referer. The first page reuses the chapter-opening request. Short-lived signed image URLs are cached for at most two minutes; catalogs and chapter indexes for five minutes. New unrecognized CDN hosts fail closed.

HTTP 429 remains a cooldown error. Visible site challenges use Manko's browser handoff. Tests use synthetic metadata and original sample reading text, including duplicates, caching, invalid paths and malformed responses. Local release 0.1.0 is prepared; publishing is separate.
