# Source Extensions

[![Add to Manko](assets/add-to-manko.svg)](https://superdupercoolperson.github.io/extensions/)

Content sources for Manko, maintained by [superdupercoolperson](https://github.com/superdupercoolperson). Open the link on a device with Manko installed, then review and confirm in the app. Adding the repository installs nothing automatically.

Copy this repository URL into **Manko → Extensions → Add Repository**:

```text
https://superdupercoolperson.github.io/extensions/
```

The previous `/dist/community/v1/stable/` repository URL remains supported for existing installations.

| Extension | Install |
| --- | --- |
| HitomiLA | [Open in Manko](https://superdupercoolperson.github.io/extensions/?source=HitomiLA) |
| LNori | [Open in Manko](https://superdupercoolperson.github.io/extensions/?source=LNori) |
| MangaBat | [Open in Manko](https://superdupercoolperson.github.io/extensions/?source=MangaBat) |
| MangaFire | [Open in Manko](https://superdupercoolperson.github.io/extensions/?source=MangaFire) |
| MangaFox | [Open in Manko](https://superdupercoolperson.github.io/extensions/?source=MangaFox) |
| MangaKatana | [Open in Manko](https://superdupercoolperson.github.io/extensions/?source=MangaKatana) |
| NHentai | [Open in Manko](https://superdupercoolperson.github.io/extensions/?source=NHentai) |
| OmegaScans | [Open in Manko](https://superdupercoolperson.github.io/extensions/?source=OmegaScans) |

## Development

Use the pinned Rust toolchain with `wasm32-unknown-unknown` and Node.js 20 or newer.

```sh
npm ci
npm test
npm run check
npm run site:check
npm run publish:dry-run
```

Builds use `target/`. The bundle command generates the static catalog page in `site/`. GitHub Actions runs the tests and publishes the catalog and website together. See [LICENSES.md](LICENSES.md) for component licenses.
