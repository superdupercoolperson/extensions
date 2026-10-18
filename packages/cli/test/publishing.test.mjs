import test from "node:test";
import {createHash} from "node:crypto";
import assert from "node:assert/strict";
import {readFile, readdir} from "node:fs/promises";
const root = new URL("../../../", import.meta.url);
test("published distribution is source-only and uses the new publisher", async () => {
  assert.deepEqual(await readdir(new URL("dist/", root)), ["community"]);
  const manifest = JSON.parse(await readFile(new URL("dist/community/v1/stable/versioning.json", root)));
  const catalog = JSON.parse(await readFile(new URL("dist/community/v1/stable/catalog.json", root)));
  assert.equal(manifest.repository.publisher.name, "superdupercoolperson");
  assert.equal(catalog.repositoryURL, "https://superdupercoolperson.github.io/extensions/");
  assert.equal(manifest.sources.length, 8);
  for (const source of manifest.sources) {
    assert.equal(source.kind, "content");
    assert.equal(source.extension.connectorKind, undefined);
    assert.deepEqual(source.developers, [{name:"superdupercoolperson"}]);
    for (const key of ["rightsURL", "sourceURL", "reportURL"]) assert.ok(source[key].startsWith("https://github.com/superdupercoolperson/extensions/"));
  }
});

test("root and legacy distributions resolve identical manifests, icons and packages", async () => {
  const legacy = new URL("dist/community/v1/stable/", root);
  const site = new URL("site/", root);
  for (const file of ["versioning.json", "catalog.json"]) {
    assert.deepEqual(await readFile(new URL(file, site)), await readFile(new URL(file, legacy)));
  }
  const manifest = JSON.parse(await readFile(new URL("versioning.json", site)));
  for (const source of manifest.sources) {
    for (const path of [source.icon, source.extension.moduleURL]) {
      assert.deepEqual(await readFile(new URL(path, site)), await readFile(new URL(path, legacy)));
    }
    const bytes = await readFile(new URL(source.extension.moduleURL, site));
    assert.equal(bytes.length, source.extension.size);
    assert.equal(createHash("sha256").update(bytes).digest("hex"), source.extension.sha256);
  }
  const config = JSON.parse(await readFile(new URL("site/handoff-config.json", root)));
  const url = "https://superdupercoolperson.github.io/extensions/";
  assert.equal(config.repositoryURL, url);
  const html = await readFile(new URL("site/index.html", root), "utf8");
  assert.ok(html.includes(`value="${url}"`));
  assert.ok(html.includes(`manko://repository/add?url=${encodeURIComponent(url)}`));
  const installLinks = [...html.matchAll(/href="(manko:\/\/repository\/install[^" ]+)"/g)];
  assert.equal(installLinks.length, manifest.sources.filter(source => !["retired", "serviceunavailable"].includes(source.availability.toLowerCase())).length);
  for (const href of installLinks) {
    assert.equal(new URL(href[1].replaceAll("&amp;", "&")).searchParams.get("url"), url);
  }
});
