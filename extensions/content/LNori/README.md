# LNori

Original Rust implementation of the public [LNORI library](https://lnori.com/library), series JSON-LD, and public volume pages. No third-party extension code is imported.

Supports English series discovery, title/author search, genre/author filters, local sorting, volume lists and sanitized XHTML publications. Catalog metadata is cached for five minutes, series and volumes for thirty minutes. Series and volume IDs retain the site's numeric ID and slug because numeric-only routes return 404.

The publication serializer retains reading structure and JPEG/PNG illustration URLs, discards executable markup and does not run remote JavaScript. Manko's progressive LNORI renderer opens validated text blocks while the volume response continues. `publicationStream` supplies the validated request descriptor without fetching a body; `publicationContent` remains available for older hosts and explicit offline EPUB acquisition. Its current EPUB sanitizer removes external illustrations, so installed volume reading is text only.

HTTP 429 remains a cooldown error. Visible site challenges use Manko's browser handoff. Tests use synthetic metadata and original sample reading text, including duplicates, caching, invalid paths and malformed responses. Local release 0.1.1 is prepared; publishing is separate.
