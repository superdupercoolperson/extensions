import test from "node:test";
import assert from "node:assert/strict";
import {readFile, readdir} from "node:fs/promises";
const root = new URL("../../../", import.meta.url);
test("published distribution is source-only and uses the new publisher", async () => {
  assert.deepEqual(await readdir(new URL("dist/", root)), ["community"]);
  const manifest = JSON.parse(await readFile(new URL("dist/community/v1/stable/versioning.json", root)));
  const catalog = JSON.parse(await readFile(new URL("dist/community/v1/stable/catalog.json", root)));
  assert.equal(manifest.repository.publisher.name, "superdupercoolperson");
  assert.equal(catalog.repositoryURL, "https://superdupercoolperson.github.io/extensions/dist/community/v1/stable/");
  assert.equal(manifest.sources.length, 8);
  for (const source of manifest.sources) {
    assert.equal(source.kind, "content");
    assert.equal(source.extension.connectorKind, undefined);
    assert.deepEqual(source.developers, [{name:"superdupercoolperson"}]);
    for (const key of ["rightsURL", "sourceURL", "reportURL"]) assert.ok(source[key].startsWith("https://github.com/superdupercoolperson/extensions/"));
  }
});
