/* SPDX-License-Identifier: Apache-2.0 */
import test from "node:test";
import assert from "node:assert/strict";
import {createHash} from "node:crypto";
import {execFileSync} from "node:child_process";
import {readFile, readdir, mkdir, mkdtemp, writeFile, rm, chmod} from "node:fs/promises";
import {resolve} from "node:path";
import {validateModule, instantiate} from "../lib/wasm.mjs";
import {assertExtensionPackage} from "../lib/contracts.mjs";
const root=resolve(import.meta.dirname,"../../..");
const packs={community:["HitomiLA","LNori","MangaBat","MangaFire","MangaFox","MangaKatana","NHentai","OmegaScans"]};

test("Independent catalogs contain only their packages with matching digests",async()=>{
  for(const [pack,ids] of Object.entries(packs)) {
    const directory=resolve(root,`dist/${pack}/v1/stable`);
    const manifest=JSON.parse(await readFile(resolve(directory,"versioning.json")));
    assert.equal(manifest.schemaVersion,3);
    assert.deepEqual(manifest.sources.map(entry=>entry.id),ids);
    assert.deepEqual((await readdir(resolve(directory,"sources"))).sort(),ids);
    for(const entry of manifest.sources){
      const metadata=JSON.parse(await readFile(resolve(root,"extensions",entry.kind,entry.id,"extension.json")));
      assert.equal(entry.version,metadata.version);
      assert.equal(entry.extension.runtime,"wasm");
      assert.equal(entry.extension.apiVersion,"3.0");
      assert.equal(entry.extension.scriptURL,undefined);
      assert.equal(Boolean(metadata.connectorKind || entry.kind === "tracker"),pack === "official");
      const bytes=await readFile(resolve(directory,entry.extension.moduleURL));
      assert.equal(bytes.length,entry.extension.size);
      assert.equal(createHash("sha256").update(bytes).digest("hex"),entry.extension.sha256);
      for(const path of [root,"/Users/","/home/"]) assert.ok(!bytes.includes(Buffer.from(path)),"module must not embed checkout or home paths");
      validateModule(bytes,entry);
    }
  }
});

test("Validator rejects unsupported metadata, corrupt binaries and oversized modules",async()=>{
  const bytes=await readFile(resolve(root,"target/wasm32-unknown-unknown/release/manko_hitomi.wasm"));
  validateModule(bytes,{id:"HitomiLA",kind:"content"});
  assert.throws(()=>validateModule(bytes,{id:"not-HitomiLA",kind:"content"}));
  assert.throws(()=>validateModule(bytes,{id:"HitomiLA",kind:"tracker"}));
  assert.throws(()=>instantiate(Buffer.from("defineContentExtension({})")));
  assert.throws(()=>instantiate(bytes.subarray(0,bytes.length/2)));
  assert.throws(()=>instantiate(Buffer.alloc(10*1024*1024+1)));
  assert.throws(()=>instantiate(Buffer.from([0,97,115,109,1,0,0,0])));
});

test("CLI scaffolds a compilable Rust package without modifying the published catalog",async()=>{
  await mkdir(resolve(root,"target/cli-tests"),{recursive:true});
  const directory=await mkdtemp(resolve(root,"target/cli-tests/scaffold-"));
  try{
    await writeFile(resolve(directory,"Cargo.toml"),'[workspace]\nmembers = []\n[workspace.package]\nedition="2021"\nlicense="Apache-2.0"\n');
    await writeFile(resolve(directory,"LICENSE"),await readFile(resolve(root,"LICENSE")));
    execFileSync(process.execPath,[resolve(root,"packages/cli/bin/manko-ext.mjs"),"new","--id","Fixture","--kind","tracker"],{env:{...process.env,MANKO_EXTENSION_ROOT:directory}});
    const metadata=await assertExtensionPackage(resolve(directory,"extensions/tracker/Fixture"),"tracker","Fixture");
    assert.equal(metadata.runtime,"wasm");
    assert.match(await readFile(resolve(directory,"Cargo.toml"),"utf8"),/members = \[\n  "extensions\/tracker\/Fixture",/);
    assert.match(await readFile(resolve(directory,"extensions/tracker/Fixture/src/lib.rs"),"utf8"),/export_extension!/);
    const cargo=resolve(directory,"fake-cargo.mjs"), log=resolve(directory,"cargo-args.json");
    await writeFile(cargo,`#!/usr/bin/env node\nimport {writeFileSync} from 'node:fs';\nwriteFileSync(${JSON.stringify(log)},JSON.stringify(process.argv.slice(2)));\n`);
    await chmod(cargo,0o755);
    execFileSync(process.execPath,[resolve(root,"packages/cli/bin/manko-ext.mjs"),"compile"],{env:{...process.env,MANKO_EXTENSION_ROOT:directory,CARGO:cargo}});
    const args=JSON.parse(await readFile(log,"utf8"));
    assert.equal(args.includes("--workspace"),false,"release builds must isolate extension dependency features");
    assert.deepEqual(args.slice(0,3),["build","-p","manko-fixture"]);
  } finally {await rm(directory,{recursive:true,force:true});}
});

test("Tracker metadata rejects disguised writes and ambiguous account paths", async () => {
  await mkdir(resolve(root, "target/cli-tests"), {recursive: true});
  const directory = await mkdtemp(resolve(root, "target/cli-tests/account-policy-"));
  try {
    await writeFile(resolve(directory, "Cargo.toml"), '[workspace]\nmembers=[]\n[workspace.package]\nedition="2021"\nlicense="Apache-2.0"\n');
    await writeFile(resolve(directory, "LICENSE"), await readFile(resolve(root, "LICENSE")));
    execFileSync(process.execPath, [resolve(root, "packages/cli/bin/manko-ext.mjs"), "new", "--id", "Fixture", "--kind", "tracker"], {env: {...process.env, MANKO_EXTENSION_ROOT: directory}});
    const pack = resolve(directory, "extensions/tracker/Fixture");
    const metadata = JSON.parse(await readFile(resolve(pack, "extension.json")));
    for (const method of ["POST", "PUT", "PATCH", "DELETE"]) {
      metadata.accountPolicy.requests[0].methods = [method];
      metadata.accountPolicy.requests[0].operations = ["progress"];
      delete metadata.accountPolicy.requests[0].bodyKind;
      await writeFile(resolve(pack, "extension.json"), JSON.stringify(metadata));
      await assert.rejects(assertExtensionPackage(pack, "tracker", "Fixture"), /Mutating requests must declare update access/);
    }
    metadata.accountPolicy.requests[0].methods = ["GET"];
    for (const path of ["/api/../", "/api/./", "/api/%2e%2e/", "/api/..;/", "/api/\\token", "/api/\u0000token"]) {
      metadata.accountPolicy.requests[0].pathPrefix = path;
      await writeFile(resolve(pack, "extension.json"), JSON.stringify(metadata));
      await assert.rejects(assertExtensionPackage(pack, "tracker", "Fixture"));
    }
  } finally { await rm(directory, {recursive: true, force: true}); }
});

test("SDK dispatch suspends source requests and releases cancelled invocations",async()=>{
  const bytes=await readFile(resolve(root,"target/wasm32-unknown-unknown/release/manko_mangafox.wasm"));
  const {dispatch}=instantiate(bytes);
  const request=dispatch({action:"invoke",invocationID:"first",method:"search",arguments:{query:"fixture"}});
  assert.ok(request.pending.length);
  assert.equal(dispatch({action:"invoke",invocationID:"second",method:"searchFilters",arguments:{}}).ok,true);
  assert.equal(dispatch({action:"cancel",invocationID:"first"}).ok,true);
  assert.equal(dispatch({action:"resume",invocationID:"first",responses:[]}).ok,false);
  assert.equal(dispatch({action:"invoke",invocationID:"first",method:"searchFilters",arguments:{}}).ok,true);
});
