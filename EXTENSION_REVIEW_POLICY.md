# Extension publication policy

Content and tracker extensions are eligible for publication. Theme extensions are unsupported.

Publication requires:

- a valid manifest, implementation, content rating, and extension version;
- representative fixtures plus contract and behavior tests;
- an appropriate `LICENSE` and any required `NOTICE`;
- source and provenance links for imported extensions;
- DCO sign-off for contributed changes; and
- deterministic packaging and generated catalog artifacts.

Tracker extensions must implement authentication, search, progress retrieval, updates, collections, token refresh where applicable, and logout. Mark an extension `serviceUnavailable` when it requires a runtime surface manko does not expose, and `retired` when it is no longer distributed.
