#!/usr/bin/env node
/*
 * Copyright 2026 manko Extension Contributors
 * SPDX-License-Identifier: Apache-2.0
 */
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { validateModule } from "../lib/wasm.mjs";
import { createServer } from "node:http";
import { homedir } from "node:os";
import { mkdir, readFile, readdir, rm, unlink, writeFile } from "node:fs/promises";
import { dirname, extname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { API_VERSION, assertExtensionPackage } from "../lib/contracts.mjs";

const ROOT = resolve(process.env.MANKO_EXTENSION_ROOT || resolve(dirname(fileURLToPath(import.meta.url)), "../../.."));
const EXTENSION_ROOTS = {
  content: join(ROOT, "extensions", "content"),
  tracker: join(ROOT, "extensions", "tracker")
};
const APACHE_HEADER = "/* Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0 */";

const args = process.argv.slice(2);
const command = args.shift();
const option = name => {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
};
const json = value => `${JSON.stringify(value, null, 2)}\n`;
const sha256 = value => createHash("sha256").update(value).digest("hex");
const safeID = value => /^[A-Za-z0-9._-]{1,128}$/.test(value);
const extensionDirectory = (kind, id) => join(EXTENSION_ROOTS[kind], id);

async function writeJSON(path, value) {
  await mkdir(dirname(path), { recursive: true });
  await writeFile(path, json(value));
}

function neutralSVG(id, rating) {
  const palette = rating === "ADULT" ? ["#8f2d56", "#d81159"] : rating === "MATURE" ? ["#6c4ab6", "#b185db"] : ["#16697a", "#489fb5"];
  const initials = id.replace(/[^A-Za-z0-9]/g, "").slice(0, 2).toUpperCase() || "MR";
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" role="img" aria-label="${id} placeholder"><defs><linearGradient id="g" x2="1" y2="1"><stop stop-color="${palette[0]}"/><stop offset="1" stop-color="${palette[1]}"/></linearGradient></defs><rect width="256" height="256" rx="48" fill="url(#g)"/><path d="M59 52h138v152H59z" fill="#fff" opacity=".16"/><text x="128" y="148" text-anchor="middle" font-family="system-ui,sans-serif" font-size="68" font-weight="700" fill="#fff">${initials}</text></svg>\n`;
}

async function createScaffold(entry) {
  if (!safeID(entry.id)) throw new Error("Unsafe extension id");
  const kind = entry.kind || "content";
  if (!EXTENSION_ROOTS[kind]) throw new Error("Unsupported extension kind");
  const directory = extensionDirectory(kind, entry.id);
  try { await readFile(join(directory, "extension.json")); throw new Error("Extension already exists"); } catch (error) { if (error.code !== "ENOENT") throw error; }
  const crate = "manko-" + entry.id.toLowerCase().replace(/[^a-z0-9]+/g,"-");
  await mkdir(join(directory, "src"), {recursive:true});
  await writeJSON(join(directory,"extension.json"), {
    id:entry.id,name:entry.name || entry.id,description:"Open-source manko extension.",kind,
    apiVersion:API_VERSION,runtime:"wasm",crate,version:"0.1.0",language:"en",languages:["en"],
    contentRating:"SAFE",availability:"serviceUnavailable",capabilities:[],permissions:["network"],
    allowedHTTPSHosts:["blocked.manko.invalid"],authenticationModes:["none"],
    ...(kind === "tracker" ? { accountPolicy: {
      authentication: { mode:"oauth2PKCE", authorizationURL:"https://blocked.manko.invalid/authorize",
        tokenURL:"https://blocked.manko.invalid/token", callbackScheme:"manko", callbackURL:"manko://oauth/fixture",
        clientID:"configure-before-publishing", responseType:"code", scopes:[], pkceMethod:"S256" },
      requests:[{origin:"https://blocked.manko.invalid",pathPrefix:"/api/",methods:["GET"],operations:["trackerSearch"]}]
    }} : {}),
    developers:[{name:"superdupercoolperson"}],iconFile:"icon.svg",license:"Apache-2.0"
  });
  await writeFile(join(directory,"Cargo.toml"), `[package]
name = "${crate}"
version = "0.1.0"
edition.workspace = true
license.workspace = true
[lib]
crate-type = ["cdylib", "rlib"]
[dependencies]
manko-sdk.workspace = true
serde_json.workspace = true
`);
  await writeFile(join(directory,"src","lib.rs"), `// SPDX-License-Identifier: Apache-2.0
use manko_sdk::{Context, Operation, error};
use serde_json::Value;
fn run(_: Context, _: String, _: Value) -> Operation { Box::pin(async { Err(error("Not implemented", "unsupportedOperation")) }) }
manko_sdk::export_extension!("${entry.id}", "${kind}", run);
`);
  await writeFile(join(directory,"icon.svg"),neutralSVG(entry.id,"SAFE"));
  await writeFile(join(directory,"LICENSE"),await readFile(join(ROOT,"LICENSE")));
  const workspace = await readFile(join(ROOT,"Cargo.toml"),"utf8");
  await writeFile(join(ROOT,"Cargo.toml"),workspace.replace("members = [",'members = [\n  "extensions/'+kind+'/'+entry.id+'",'));
}

function compile() {
  const flags = [
    "-C", "link-arg=--max-memory=134217728",
    `--remap-path-prefix=${ROOT}=/manko-extensions`,
    `--remap-path-prefix=${resolve(process.env.CARGO_HOME || join(homedir(), ".cargo"))}=/cargo`
  ];
  // Build each extension with its own dependency features. Adding an HTML
  // provider must not silently change unrelated released module bytes.
  const entries = readdirSync(join(ROOT, "extensions"), {withFileTypes:true})
    .filter(entry => entry.isDirectory()).flatMap(kind =>
      readdirSync(join(ROOT, "extensions", kind.name), {withFileTypes:true})
        .filter(entry => entry.isDirectory()).map(entry =>
          JSON.parse(readFileSync(join(ROOT, "extensions", kind.name, entry.name, "extension.json"))).crate));
  for (const crate of entries.sort()) {
    execFileSync(process.env.CARGO || "cargo", ["build","-p",crate,"--lib","--examples","--release","--locked","--target","wasm32-unknown-unknown"], {
      cwd:ROOT, stdio:"inherit",
      env:{...process.env,CARGO_ENCODED_RUSTFLAGS:flags.join("\u001f")}
    });
  }
}

async function seedCatalog() {
  const input = option("--input");
  if (!input) throw new Error("seed-catalog requires --input");
  const raw = JSON.parse(await readFile(resolve(input)));
  if (!Array.isArray(raw.sources)) throw new Error("Catalog seed must contain a sources array");
  const entries = raw.sources.map(source => ({
    id: source.id,
    name: source.name,
    kind: source.kind || "content",
    language: source.language || source.languages?.[0] || "en",
    contentRating: source.contentRating || "SAFE"
  }));
  const ids = new Set(entries.map(entry => entry.id));
  if (ids.size !== entries.length) throw new Error("Catalog seed contains duplicate extension IDs");
  for (const entry of entries) await createScaffold(entry);
  console.log(`Seeded ${entries.length} extension scaffold${entries.length === 1 ? "" : "s"}.`);
}

function selectedKinds() {
  const kind = option("--kind");
  if (!kind) return Object.keys(EXTENSION_ROOTS);
  if (!EXTENSION_ROOTS[kind]) throw new Error(`Unsupported extension kind: ${kind}`);
  return [kind];
}

async function extensionEntries(kinds = Object.keys(EXTENSION_ROOTS)) {
  const result = [];
  for (const kind of kinds) {
    await mkdir(EXTENSION_ROOTS[kind], { recursive: true });
    const entries = await readdir(EXTENSION_ROOTS[kind], { withFileTypes: true });
    for (const entry of entries.filter(entry => entry.isDirectory())) result.push({ kind, id: entry.name });
  }
  return result.sort((a, b) => a.id.localeCompare(b.id) || a.kind.localeCompare(b.kind));
}

async function assertSupportedLayout() {
  const entries = await readdir(join(ROOT, "extensions"), { withFileTypes: true });
  const unsupported = entries.filter(entry => entry.isDirectory() && !EXTENSION_ROOTS[entry.name]);
  if (unsupported.length) throw new Error(`Unsupported extension directories: ${unsupported.map(entry => entry.name).join(", ")}`);
}

async function check() {
  await assertSupportedLayout();
  const entries = await extensionEntries(selectedKinds());
  const ids = entries.map(entry => entry.id);
  if (new Set(ids).size !== ids.length) throw new Error("Extension IDs must be unique");
  for (const entry of entries) await assertExtensionPackage(extensionDirectory(entry.kind, entry.id), entry.kind, entry.id);
  const packs = await catalogPacks(entries);
  for (const [name, packEntries] of Object.entries(packs)) {
    try {
      const manifest = JSON.parse(await readFile(join(ROOT, "dist", name, "v1", "stable", "versioning.json"), "utf8"));
      if (manifest.schemaVersion !== 3 || JSON.stringify(manifest.sources.map(source => source.id)) !== JSON.stringify(packEntries.map(entry => entry.id))) throw new Error(`Generated ${name} manifest does not match its packages`);
      if (manifest.sources.some(source => !EXTENSION_ROOTS[source.kind] || source.entryType !== "extension" || !source.extension)) throw new Error(`Invalid ${name} manifest`);
    } catch (error) { if (error.code !== "ENOENT") throw error; }
  }
  const trackers = entries.filter(entry => entry.kind === "tracker").length;
  console.log(`check: ${entries.length} extensions (${entries.length - trackers} content, ${trackers} tracker)`);
}

async function pruneGeneratedFiles(directory, expected, matchesGeneratedName) {
  await mkdir(directory, { recursive: true });
  for (const filename of await readdir(directory)) {
    if (matchesGeneratedName(filename) && !expected.has(filename)) await unlink(join(directory, filename));
  }
}

async function catalogPacks(entries) {
  const packs = { community: [] };
  for (const entry of entries) {
    const metadata = await assertExtensionPackage(extensionDirectory(entry.kind, entry.id), entry.kind, entry.id);
    if (entry.kind !== "content" || metadata.connectorKind) throw new Error(`Only content sources can be published: ${entry.id}`);
    packs.community.push(entry);
  }
  return packs;
}

async function buildArtifacts() {
  await assertSupportedLayout();
  const entries = await extensionEntries(selectedKinds());
  const packs = await catalogPacks(entries);
  const configuration = JSON.parse(await readFile(join(ROOT, "repository.config.json"), "utf8"));
  const manifests = [];
  for (const [packName, packEntries] of Object.entries(packs)) {
    manifests.push(await buildCatalog(packName, packEntries, configuration.packs[packName]));
  }
  const sources = manifests.flatMap(manifest => manifest.sources);
  const counts = Object.fromEntries(Object.keys(EXTENSION_ROOTS).map(kind => [kind, entries.filter(entry => entry.kind === kind).length]));
  await writeJSON(join(ROOT, "inventory", "registry.json"), { entries });
  await writeJSON(join(ROOT, "inventory", "status.json"), { counts, packs: Object.fromEntries(Object.entries(packs).map(([name, entries]) => [name, entries.map(entry => entry.id)])), entries: sources.map(source => ({ id: source.id, kind: source.kind, availability: source.availability })) });
  return manifests;
}

async function buildCatalog(packName, entries, config, writeOutput = true) {
  const sourceRevision = process.env.MANKO_SOURCE_REVISION?.trim() || "main";
  if (!/^(?:main|[0-9a-f]{40})$/.test(sourceRevision)) {
    throw new Error("MANKO_SOURCE_REVISION must be main or a full Git commit SHA");
  }
  const output = join(ROOT, "dist", packName, "v1", "stable");
  const sourcesDirectory = join(output, "sources");
  const iconsDirectory = join(output, "icons");
  if (writeOutput) await rm(sourcesDirectory, { recursive: true, force: true });
  if (writeOutput) await rm(join(output, "packages"), { recursive: true, force: true });
  if (writeOutput) await mkdir(sourcesDirectory, { recursive: true });
  if (writeOutput) await mkdir(iconsDirectory, { recursive: true });
  const sources = [];
  const catalog = [];
  const iconNames = new Set();
  for (const { kind, id } of entries) {
    const directory = extensionDirectory(kind, id);
    const metadata = await assertExtensionPackage(directory, kind, id);
    const script = await readFile(join(ROOT,"target","wasm32-unknown-unknown","release",`${metadata.crate.replaceAll("-","_")}.wasm`));
    validateModule(script,metadata);
    const moduleURL = `sources/${id}/${metadata.version}/main.wasm`;
    if (writeOutput) await mkdir(dirname(join(output, moduleURL)), { recursive: true });
    if (writeOutput) await writeFile(join(output, moduleURL), script);
    const iconExtension = extname(metadata.iconFile).toLowerCase();
    const iconName = `${id}${iconExtension}`;
    iconNames.add(iconName);
    const iconBytes = await readFile(join(directory, metadata.iconFile));
    if (writeOutput) await writeFile(join(iconsDirectory, iconName), iconBytes);
    const source = {
      id,
      name: metadata.name,
      description: metadata.description,
      version: metadata.version,
      icon: `icons/${iconName}`,
      languages: metadata.languages,
      contentRating: metadata.contentRating,
      capabilities: metadata.capabilities,
      developers: metadata.developers.map(developer => ({ name: developer.name })),
      rightsDeclaration: `Licensed under ${metadata.license || "Apache-2.0"}.`,
      rightsURL: `https://github.com/superdupercoolperson/extensions/blob/${sourceRevision}/extensions/${kind}/${id}/LICENSE`,
      reportURL: "https://github.com/superdupercoolperson/extensions/issues/new/choose",
      sourceURL: `https://github.com/superdupercoolperson/extensions/tree/${sourceRevision}/extensions/${kind}/${id}`,
      sourceRevision,
      entryType: "extension",
      kind,
      availability: metadata.availability,
      permissions: { values: metadata.permissions },
      extension: {
        apiVersion: metadata.apiVersion,
        runtime: "wasm",
        moduleURL,
        sha256: sha256(script),
        size: script.length,
        allowedHTTPSHosts: metadata.allowedHTTPSHosts,
        authenticationModes: metadata.authenticationModes,
        ...(metadata.accountPolicy ? { accountPolicy: metadata.accountPolicy } : {}),
        ...(metadata.connectorKind ? { connectorKind: metadata.connectorKind } : {})
      }
    };
    sources.push(source);
    catalog.push({
      ...source,
      developers: metadata.developers,
      moduleSHA256: source.extension.sha256,
      license: metadata.license || "Apache-2.0",
      compatibility: metadata.compatibility || null,
      upstream: metadata.upstream || null
    });
  }
  if (writeOutput) await pruneGeneratedFiles(iconsDirectory, iconNames, filename => /\.(?:svg|png|jpe?g|webp|ico)$/i.test(filename));
  const manifest = {
    schemaVersion: 3,
    repository: {
      id: config.id,
      name: config.name,
      description: config.description,
      homepage: config.homepage,
      privacyURL: config.privacyURL,
      publisher: { name: config.publisherName }
    },
    sources
  };
  if (writeOutput) await writeJSON(join(output, "versioning.json"), manifest);
  if (writeOutput) await rm(join(output, "versioning.unsigned.json"), { force: true });
  if (writeOutput) await writeJSON(join(output, "catalog.json"), { repositoryURL: config.repositoryURL, repository: { name: config.name }, sources: catalog });
  if (writeOutput) console.log(`Bundled ${sources.length} deterministic WASM module${sources.length === 1 ? "" : "s"}.`);
  return manifest;
}

async function publish() {
  const configs = JSON.parse(await readFile(join(ROOT, "repository.config.json"), "utf8")).packs;
  const packs = await catalogPacks(await extensionEntries());
  for (const packName of Object.keys(configs)) {
    const output = join(ROOT, "dist", packName, "v1", "stable");
    const current = JSON.parse(await readFile(join(output, "versioning.json")));
    if (current.schemaVersion !== 3) throw new Error("A schema-3 catalog is required");
    const rebuilt = await buildCatalog(packName, packs[packName], configs[packName], false);
    if (JSON.stringify(current) !== JSON.stringify(rebuilt)) throw new Error("Catalog does not match the current build");
    for (const source of current.sources) {
      const bytes = await readFile(join(output, source.extension.moduleURL));
      if (bytes.length !== source.extension.size || sha256(bytes) !== source.extension.sha256) throw new Error(`Script hash mismatch: ${source.id}`);
      const official = source.kind === "tracker" || Boolean(source.extension.connectorKind);
      if (official !== (packName === "official")) throw new Error(`Wrong catalog membership: ${source.id}`);
      validateModule(bytes, source);
    }
    console.log(`publish dry-run: ${packName}: ${current.sources.length} packages validated`);
  }
}

async function createNew() {
  const id = option("--id");
  const name = option("--name") || id;
  const kind = option("--kind") || "content";
  if (!id || !safeID(id)) throw new Error("new requires a safe --id");
  if (!EXTENSION_ROOTS[kind]) throw new Error("--kind must be content or tracker");
  await createScaffold({ id, name, kind, language: "en", contentRating: "SAFE" });
  console.log(`Created ${relative(ROOT, extensionDirectory(kind, id))}`);
}

async function serve() {
  const port = Number(option("--port") || 4173);
  const contentTypes = { ".html": "text/html; charset=utf-8", ".css": "text/css; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".mjs": "text/javascript; charset=utf-8", ".json": "application/json", ".wasm": "application/wasm", ".xml": "application/xml", ".woff2": "font/woff2", ".svg": "image/svg+xml", ".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".webp": "image/webp", ".ico": "image/x-icon" };
  const server = createServer(async (request, response) => {
    try {
      let pathname = decodeURIComponent(new URL(request.url, `http://localhost:${port}`).pathname);
      if (pathname === "/extensions") pathname = "/";
      else if (pathname.startsWith("/extensions/")) pathname = pathname.slice("/extensions".length);
      if (pathname === "/") pathname = "/index.html";
      const base = pathname.startsWith("/dist/") ? ROOT : join(ROOT, "site");
      const path = resolve(base, `.${pathname}`);
      if (!path.startsWith(`${base}${sep}`)) throw new Error("unsafe path");
      const candidates = extname(path) ? [path] : [path, `${path}.html`, join(path, "index.html")];
      let resolvedPath;
      let bytes;
      for (const candidate of candidates) {
        try {
          bytes = await readFile(candidate);
          resolvedPath = candidate;
          break;
        } catch (error) {
          if (error.code !== "ENOENT" && error.code !== "EISDIR") throw error;
        }
      }
      if (!resolvedPath) throw new Error("not found");
      response.writeHead(200, { "Content-Type": contentTypes[extname(resolvedPath)] || "application/octet-stream", "Cache-Control": "no-store" });
      response.end(bytes);
    } catch {
      response.writeHead(404);
      response.end("Not found");
    }
  });
  server.listen(port, "127.0.0.1", () => console.log(`manko catalog: http://127.0.0.1:${port}`));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    switch (command) {
    case "seed-catalog": await seedCatalog(); break;
    case "new": await createNew(); break;
    case "check": await check(); break;
    case "test": await check(); break;
    case "compile": compile(); break;
    case "bundle": compile(); await buildArtifacts(); break;
    case "publish": await publish(); break;
    case "serve": await serve(); break;
    default:
      console.log("manko-ext new|seed-catalog|check|test|bundle|serve|publish");
      process.exitCode = command ? 1 : 0;
    }
  } catch (error) {
    console.error(error.stack || error.message);
    process.exitCode = 1;
  }
}
