# Extension ABI 3.0

Modules target wasm32-unknown-unknown, export memory, and have no imports, including WASI. Manko embeds WasmKit 0.4.1 with software memory checks and fuel metering on an isolated Swift actor.

## Exports

- manko_alloc(i32 byteCount) -> i32 pointer allocates guest-owned writable bytes.
- manko_dealloc(i32 pointer, i32 byteCount) -> void frees that exact allocation.
- manko_dispatch(i32 pointer, i32 byteCount) -> i64 reads UTF-8 JSON without taking ownership.
- Dispatch packs the output pointer in the high 32 bits and byte count in the low 32 bits. The host copies the result then frees it with manko_dealloc. No pointer survives a dispatch.

Memory references are checked before access. Modules are limited to 10 MiB, memory to 128 MiB, dispatch to 100 million fuel units, and an invocation to 500 million total fuel units. Existing operation deadlines still apply. Unsupported exports, imports, API versions and malformed responses are rejected.

## Dispatch

Requests are JSON objects:

```json
{"action":"metadata"}
{"action":"invoke","invocationID":"host-owned UUID","method":"search","arguments":{"title":"example","metadata":null}}
{"action":"resume","invocationID":"host-owned UUID","responses":[{"id":"guest effect ID","payload":{},"error":null}]}
{"action":"cancel","invocationID":"host-owned UUID"}
```

Metadata returns id, kind (content or tracker), apiVersion (3.0), imageRequestMode (independent), and cachePolicy (metadata). All must match the catalog where specified.

Operations return {"ok":true,"value":...}, {"ok":false,"error":{"name":"...","message":"...","type":"..."}}, or {"pending":[{"id":"...","service":"http","arguments":{...}}]}. Effect IDs are unique within an invocation. There may be up to 64 outstanding effects; pending can be empty while prior effects are still running. The host resumes after each effect finishes. Manko replaces guest-supplied invocation/request owners with native identities. Cancellation drops guest futures and cancels owned native requests.

When a search or discover invocation explicitly includes `catalogProgress: true`, the guest may call the optional `catalogProgress` host service with `{ "items": [...] }`. Each update is a cumulative, ordered prefix of the current page (maximum 100 items), using that operation's normal item schema. These updates are transient: only the final paged response commits pagination and persistent page caching. Hosts revoke the invocation's progress callback on completion or cancellation. Guests must not call this service when the flag is absent or false, preserving compatibility with older hosts. Hitomi keeps four metadata requests active and publishes ready prefixes without waiting for the rest of its 25-card page.

Content inputs preserve Manko adapter contracts: details receives {workID}, installments {sourceWork}, imagePages {extensionInstallment}, search {title,selections,filters,sort,metadata}. The Rust SDK normalizes these to provider inputs. Other content methods include initialize, settings, discoverSections, discover, searchFilters, searchSuggestions, invalidateCache, updates and managedCollections. Tracker methods are authentication, trackerSearch, progress, update and collections. Domain result JSON is unchanged.

imagePageContents receives urls and requestIDs arrays (maximum four). Each dispatch can include completed: [{requestID,envelope}] alongside pending or the final result. The SDK emits independently completed images immediately, without waiting for other images. Opaque native resourceID values belong to the native invocation and cannot be persisted, forged or consumed by another runtime.

## Native Services

- http: url, method, headers, bodyBase64, imageResource. Manko retains its existing broker destination policy and system ATS enforcement; destinations are not pinned to advisory catalog hosts. Native HTTP handles cookies, redirects, pacing, Cloudflare inspection and verified browser transport.
- state / secureState: operation get, set, remove or reset, key and optional value. Writes require the active namespace authorization. Reserved verification keys are never exposed. Ordinary-state reset clears the namespace; secure-state reset removes only the specified key, never the native verification profile.
- cache: read/write/remove with key, ttlSeconds (positive or null for immutable data), permit and value. acquire/release serialize identical metadata loaders and release on cancellation. Revalidate cached data before reuse. Never cache tokens, cookies, HTTP envelopes, errors or image bytes.
- clock: now, milliseconds and random unsigned 32-bit integer.
- sleep: milliseconds, 0-60000, cancellation-aware.
- challenge: url requests visible native verification.
- log: accepts redacted diagnostic metadata.

There is no page execution, JavaScript evaluation, native filesystem, direct network, worker or process service. WebKit is reserved for native visible authentication, Cloudflare verification and verified transport.

Signed repository schema 3 publishes extension.runtime = wasm, apiVersion = 3.0, moduleURL, sha256, size, allowedHTTPSHosts, authenticationModes and the native account policy. Execution requires an installation grant for the reviewed publisher and permissions. Legacy API 1/2 metadata remains readable by Manko but cannot execute.

### Optional tracker reading metadata (API 3.0)

Trackers may implement `trackingCapabilities`, returning `{ "supportsDateUpdates": true|false }`. Older modules may reject this operation; hosts then treat date writes as unsupported. `progress` may add `startDate` and `finishDate` as objects containing optional Gregorian `year`, `month`, `day`, and positive `totalChapters` / `totalVolumes` (null or zero means unknown).

Scores use the host’s 1–10 scale (fractional inbound scores remain fractional), with null for unrated. Status uses `planning`, `reading`, `completed`, `paused`, `dropped`, `rereading`. Updates may contain `dateUpdates`: missing start/finish keys leave remote values untouched; explicit null clears them. AniList implements date writes. MyAnimeList exposes dates read-only because its documented manga-list update endpoint does not include date fields. Metadata-only totals are never written back.

## User-owned server connector profile v1

An API 3.0 content package may declare `connectorKind` (`opds`, `komga`, `kavita`, or `suwayomi`) in its local manifest and catalog extension payload. The guest retains the same WASM exports and must implement `connectorInfo`, returning `{ "kind": "komga", "version": 1 }`. The CLI and native staged installer validate this response. Initialization has no server side effects and must work before a connection exists. Older hosts cannot execute the configured connector operations; publish only with a connector-capable Manko release.

One installed package serves multiple native connection records. Connections retain their source ID, endpoint policy, Keychain account and non-secret settings. Every connection requires a matching installed connector. Disabled, removed or incompatible selected packages make that connection unavailable; they never silently switch implementation. Install or enable the selected connector to recover; production builds contain no built-in server implementation.

The host invokes `connector` with `{ operation, baseURL, options, ...arguments }`. Credentials and native source/work/chapter IDs are absent. Operations are:

- `capabilities`: validates the server and returns supported native source capability names.
- `browse` / `search`: receive `query` (text, one-based page, pageSize and filters) and return `{ items, nextPage, totalCount }`.
- `details`: receives `remoteID` and returns one work.
- `installments`: receives a series `remoteID` and one-based `page`, returning the same page envelope.
- `acquisition`: receives a book `remoteID`, returning its publication URL without fetching file bytes.
- `imagePages` (Suwayomi): receives chapter `remoteID`, optional `pageCount`, and page query; returns bounded index/resourceURL descriptors. Native image transport owns credentials, cancellation and downloads.

Work DTOs contain `remoteID`, `title`, `authors`, `summary`, `tags`, nullable `languageCode`, native `kind` / `status` values, and nullable `coverURL` and optional `alternativeTitles`. Installment DTOs contain `remoteID`, `title`, nullable `number`, nullable ISO-8601 `publishedAt`, `format` (`epub`, `pdf`, `cbz`, or Suwayomi `remoteImageSequence`), `acquisitionURL`, and nullable `pageCount`. The native adapter validates identities/URLs and uses the original stable server ID algorithm, preserving library associations and download history.

Native HTTP applies each connection’s endpoint policy and injects its credentials at its exact origin. Connector HTTP effects, acquisition and image-page URLs must use that configured origin. Cover URLs and redirects must stay within that exact origin. Runtime cookies, state and cache namespaces are separate per package/connection; package or connection removal revokes runtimes and cleans their namespaces. Credentials remain owned by the connection and are not deleted when its package is removed. Publication acquisition uses the existing native download queue and format validation.

### Optional LNORI progressive publication descriptor (API 3.0)

LNORI 0.1.1 implements `publicationStream` through the existing invocation export. Arguments are `{ "installmentId": "<numeric-id>/<slug>", "workId": "<series-id>/<slug>" }`. It returns `{ "id", "workId", "url", "headers" }` without fetching the volume. The host validates the returned identities, reviewed HTTPS destination, and response's structured book/series metadata before presenting any text. Native transport applies the installed source's permissions, credentials, redirects, verification, pacing and cooldowns. This is an optional source-specific method; ABI exports and `publicationContent` remain unchanged. Older native hosts continue using `publicationContent`.

## API 3 security contract

See [SECURITY.md](SECURITY.md) for signed catalogs, destination grants, native account policies and migration. Raw native OAuth keys and browser cookies are unavailable to guest state services. `Context.authenticated_request` requests native credential attachment under the signed account policy; ordinary requests cannot supply Authorization or Cookie headers. Offline installation validation runs module metadata and connector introspection without network or storage effects.

Discovery and search work items may include `previewImageUrl`, an optional smaller image of the same cover. `imageUrl` remains the final-quality image. Hosts without preview support ignore the optional field; hosts must validate and broker both URLs under the same source permissions. Missing or failed previews fall back to `imageUrl`.
