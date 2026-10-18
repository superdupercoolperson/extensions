# Extension security — API 3

Report vulnerabilities through GitHub Security Advisories. Do not include credentials in public issues.

## Execution and website access

Manko executes import-free WASM with bounded memory, fuel and operation deadlines. Modules have no direct sockets, filesystem, WebKit, Keychain or app-database access. The native host checks declared permissions for every effect. Every public destination must match a user-approved exact HTTPS hostname or explicit subdomain wildcard. Wildcards do not include the parent hostname; Manko rejects public-suffix wildcards and non-public DNS answers. Redirects are checked again. The native local CONNECT broker resolves once and connects to the checked numeric address; TLS remains end-to-end. Public extension HTTPS traffic never falls back to direct networking if the broker is unavailable. Explicitly configured personal HTTP servers use the native origin-checked transport; they are not public-host grants.

Personal server connectors are restricted to the exact scheme, hostname and port configured by the user. Local/private hosts and explicitly enabled HTTP are exceptions only for that server origin. Covers, pages and downloads use that same boundary. Constrained file downloads cannot continue after the local broker's process terminates; the durable queue supports retry without an unrestricted fallback. Revoked downloads retain their access provenance and cannot retry under another grant.

A domain grant is permission to contact a website, not evidence that its operator is honest. The operator sees the user's IP address, requests and any data the extension sends. An extension can disclose the source content, search terms, settings and account responses that the host gives it to its approved destinations. Approving an attacker's hostname still permits that contact. Manko does not verify the publisher's identity or a claimed brand or domain's ownership.

## Accounts

Tracker modules use `Context.authenticated_request`; they never read or construct bearer tokens. The user-reviewed `accountPolicy` pins the authorization and token URLs, callback, API origins, path prefixes, HTTP methods and invocation names. OAuth secrets remain in native Keychain keys inaccessible to the guest. Native authenticated requests reject redirects and ambiguous paths (dot segments, repeated encoding, backslashes, control characters and matrix parameters) that could escape the approved API prefix. GraphQL read rules use `bodyKind: "graphqlQuery"`; write rules use `"graphqlMutation"` and the `update` invocation. REST methods should be separated into read and write rules. The native host also requires `trackerUpdates` for the update invocation.

OAuth client IDs are public. Never commit private OAuth secrets, access/refresh tokens, cookies, connector passwords. The host owns browser cookies and sends them only to the matching origin; guest storage cannot read native OAuth or browser keys. Extension-local secure storage remains available only when explicitly declared. Per-namespace storage is limited to 256 entries / 4 MiB and each value to 1 MiB. Ordinary HTTP responses are bounded; image and publication paths have their own limits.

Existing API 1/2 packages remain metadata but cannot execute. Users need an API 3 package and a new access review. Old tracker tokens are not handed to upgraded guests; users sign in again. Backups do not restore executable access grants.

## Catalogs and publishing

`npm run bundle` writes ordinary schema-3 `versioning.json` catalogs. No signing keys, signatures, catalog revisions or expiry are required. All package versions must increase for an API 3 migration; a prerelease suffix on the old version is not an upgrade.

Run `npm run publish:dry-run` before publishing. It verifies current catalog metadata, package membership, module hashes, sizes and WASM validation. The Pages workflow builds from the exact source revision and requires no publisher secret.

Manko binds installation approval to the repository root URL and ID, extension ID, artifact hash, permission digest and revocation generation. It rejects repository-address changes and replacement bytes under the same module version. New installations and changed access require native review. Updates with unchanged permissions may reuse approval; an update from an approved repository is not a guarantee of trustworthy code. Old saved approvals without repository-address binding require another review; repository and installed metadata remain available. Backups clear executable grants.

## Limits of this boundary

The WASM broker is the security boundary for module effects. Native browser verification still executes the approved website's JavaScript within WebKit; do not interpret domain checking as a general-purpose browser exploit or WebRTC containment guarantee. System OAuth browsing, OS vulnerabilities, compromised approved websites, malicious publisher updates within an already approved permission set, and phishing that convinces the user to approve a malicious destination require separate defenses. Use the native destination display, minimal grants, prompt revocation and independent publisher verification.
