# MangaFire

Rust/WASM extension for [MangaFire](https://mangafire.to), using its current `/api` JSON endpoints and native request signing. No remote JavaScript is executed. The signing implementation is adapted from the Apache-2.0 keiyoushi source; see NOTICE and provenance.json for the pinned revision.

Supports the native weekly trending shelf, paginated latest updates, title search, genre inclusion/exclusion, type/status/language filters, sorting, series details, and image chapters. Chapter IDs retain separate translations and official/unofficial releases. Seven site languages are supported: English, Japanese, French, Spanish, Latin American Spanish, Portuguese and Brazilian Portuguese. Manko can filter the returned chapter languages.

Metadata is cached for five minutes and genre options for one hour. Chapter page URLs are resolved again each time a reader opens; image bytes and protection tokens are never cached by the guest. Images use HTTPS subdomains of mfcdn.nl and mfcdn3.xyz, checked against exact DNS suffix boundaries. HTTP 429 preserves its cooldown; Cloudflare verification uses Manko's visible challenge handoff. Rejected signing requests report that an extension update may be needed.

Account features and volume releases are outside this initial version. MangaFire can change its signing protocol or CDN domains. All behavior fixtures contain original synthetic metadata and signing vectors from the pinned reference. Release 0.1.0 is prepared locally; publication remains a separate release step.

Verification: `cargo test -p manko-mangafire --locked`, `npm run bundle`, then `node --test extensions/content/MangaFire/tests/*.test.mjs`. Live smoke verification additionally uses the built WASM module against catalog, details, chapter and image endpoints; network availability and site verification can affect it.
