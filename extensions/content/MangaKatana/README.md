# MangaKatana

Original Apache-2.0 Rust implementation of [MangaKatana](https://mangakatana.com), using Manko Extension API 3.0. No third-party extension code is imported. Public HTML and filter/reader behavior were inspected on 18 October 2026. The service favicon has separate artwork provenance in `icon-provenance.json`.

Supports Hot Updates, Latest Updates, New Manga, the Manga Directory, title and author search, inclusive/exclusive genres (all/any), status, chapter-count thresholds, and directory sorting. Title search, author search, and directory filters are separate site operations; unsupported combinations produce an explicit error. Genre suggestions use the site's published genre slugs. Accounts, bookmarks, comments, and alternate image servers are excluded.

Details include alternate titles, authors, synopsis, genres and status. Chapters preserve site order, decimal numbers, titles and publication dates. Stable chapter/page references resolve the ordered `thzq` string array without JavaScript execution. The CDN’s generic octet-stream responses are accepted only when their bytes identify a JPEG, PNG, WebP or GIF. Images are requested independently with their chapter Referer. Metadata caches last five minutes, tokenized reader URLs at most two minutes; a 401/403/404 image response refreshes the reader once. Cooldowns and visible verification challenges retain the shared runtime behavior.

Only the site and its observed `i1.mangakatana.com` image host are permitted. New CDN hosts require an update. No cookies, image bytes or errors are stored in metadata caches. Tests use original synthetic titles and sample images, including long chapter lists, escaped strings, malformed responses, expiration and unsafe destinations.

Build and validation use the repository's existing `target` directory and standard `npm run bundle`, `npm test`, `npm run check`, documentation checks and `npm run publish:dry-run`. Publication is separate from preparing the local community package.
