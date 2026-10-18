/* Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0 */
import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { assertExtensionPackage } from "../../../../packages/cli/lib/contracts.mjs";
import { validateModule } from "../../../../packages/cli/lib/wasm.mjs";
const directory=resolve(dirname(fileURLToPath(import.meta.url)),"..");
test("NHentai is a native WASM package with retained provenance",async()=>{
  const metadata=await assertExtensionPackage(directory,"content","NHentai");
  assert.equal(metadata.contentRating,"ADULT");
  assert.equal(metadata.availability,"approvalRequired");
  const provenance=JSON.parse(await readFile(resolve(directory,"provenance.json")));
  assert.ok(provenance);
  validateModule(await readFile(resolve(directory,"../../../target/wasm32-unknown-unknown/release",metadata.crate.replaceAll("-","_")+".wasm")),metadata);
});
