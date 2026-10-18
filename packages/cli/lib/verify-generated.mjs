// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {createHash} from "node:crypto";
import {readFileSync} from "node:fs";
import {validateModule} from "./wasm.mjs";

const root = new URL("../../../", import.meta.url);
const prefixes = ["dist/community/v1/stable/"];
const committed = path => execFileSync("git", ["show", `HEAD:${path}`], {cwd:root, maxBuffer:12 * 1024 * 1024});
const rebuilt = path => readFileSync(new URL(path, root));
const digest = bytes => createHash("sha256").update(bytes).digest("hex");

function verify(read, prefix) {
  const manifest = JSON.parse(read(prefix + "versioning.json"));
  const catalog = JSON.parse(read(prefix + "catalog.json"));
  for (const source of manifest.sources) {
    const bytes = read(prefix + source.extension.moduleURL);
    assert.equal(bytes.length, source.extension.size, `${source.id}: size`);
    assert.equal(digest(bytes), source.extension.sha256, `${source.id}: hash`);
    validateModule(bytes, source);
    const item = catalog.sources.find(item => item.id === source.id);
    assert.deepEqual(item.extension, source.extension, `${source.id}: catalog artifact`);
    assert.equal(item.moduleSHA256, source.extension.sha256);
  }
  assert.equal(catalog.sources.length, manifest.sources.length);
  // Compare everything except compiler-dependent artifact fingerprints.
  for (const source of [...manifest.sources, ...catalog.sources]) {
    delete source.extension.sha256;
    delete source.extension.size;
    delete source.moduleSHA256;
  }
  return {manifest, catalog};
}

const modulePaths = new Set();
for (const prefix of prefixes) {
  assert.deepEqual(verify(rebuilt, prefix), verify(committed, prefix), "Generated catalog metadata is stale");
  for (const source of JSON.parse(rebuilt(prefix + "versioning.json")).sources) modulePaths.add(prefix + source.extension.moduleURL);
}
const changes = execFileSync("git", ["status", "--porcelain", "--untracked-files=all", "--", "dist", "inventory", "repository.config.json"], {cwd:root, encoding:"utf8"});
for (const line of changes.trimEnd().split("\n").filter(Boolean)) {
  const path = line.slice(3);
  assert.ok(line.startsWith(" M ") && (prefixes.some(prefix => path === prefix + "catalog.json" || path === prefix + "versioning.json") || modulePaths.has(path)), `Unexpected generated change: ${line}`);
}
console.log("Committed and rebuilt content-source catalogs are valid and current.");
