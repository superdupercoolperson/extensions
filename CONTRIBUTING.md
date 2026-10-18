# Contributing

Contributions certify the Developer Certificate of Origin with `git commit -s`.

This repository accepts content and tracker extensions. Theme extension declarations, directories, catalog entries, and runtime registrations are unsupported.

Before publishing an extension, provide its manifest, implementation, `LICENSE`, source and provenance links, representative fixtures, and contract and behavior tests. Add a `NOTICE` when required by the upstream license. Tracker extensions must cover authentication, search, progress, updates, collections, expiry/refresh, and logout behavior.

Increment the extension version whenever executable bytes change. Keep generated scripts and manifests deterministic and do not commit OAuth secrets, access tokens, or other credentials.

Run `npm run check && npm test && npm run bundle && npm run docs:build && npm run publish:dry-run` before opening a pull request.
