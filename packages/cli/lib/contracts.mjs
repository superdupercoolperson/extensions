/* Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0 */
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { basename, extname, join } from "node:path";

export const API_VERSION = "3.0";
export const SUPPORTED_API_VERSIONS = new Set(["3.0"]);

export const repositoryCapabilities = new Set([
  "browse", "discover", "search", "filters", "details", "installments",
  "acquisition", "imageSequence", "updates", "managedCollections", "settings",
  "authentication", "interceptors", "cookies", "challengeHandoff",
  "progress", "trackerCollections", "trackerSearch"
]);

export const extensionPermissions = new Set([
  "network", "cookies", "state", "secureState", "rateLimiting", "redactedLogging",
  "challengeHandoff", "authenticationHandoff", "managedCollections", "trackerUpdates"
]);

export const authenticationModes = new Set([
  "none", "basic", "bearer", "apiKey", "oauth2Implicit", "oauth2PKCE", "visibleWebSession"
]);

const availabilities = new Set(["approvalRequired", "available", "serviceUnavailable", "retired"]);
const contentRatings = new Set(["SAFE", "MATURE", "ADULT"]);

function assertNonemptyString(value, field) {
  assert.equal(typeof value, "string", `${field} must be a string`);
  assert.ok(value.trim(), `${field} must not be empty`);
}

function assertStringArray(value, field, allowed, { required = false } = {}) {
  assert.ok(Array.isArray(value), `${field} must be an array`);
  if (required) assert.ok(value.length > 0, `${field} must not be empty`);
  value.forEach((item, index) => {
    assertNonemptyString(item, `${field}[${index}]`);
    if (allowed) assert.ok(allowed.has(item), `${field} contains unsupported value ${item}`);
  });
  assert.equal(new Set(value).size, value.length, `${field} must not contain duplicates`);
}

function assertIcon(bytes, extension, id) {
  if (extension === ".svg") {
    assert.match(bytes.toString("utf8", 0, Math.min(bytes.length, 512)), /<svg\b/i, `${id} icon is not SVG`);
    return;
  }
  if (extension === ".png") {
    assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", `${id} icon is not PNG`);
    return;
  }
  if ([".jpg", ".jpeg"].includes(extension)) {
    assert.equal(bytes.subarray(0, 2).toString("hex"), "ffd8", `${id} icon is not JPEG`);
    return;
  }
  if (extension === ".webp") {
    assert.equal(bytes.subarray(0, 4).toString("ascii"), "RIFF", `${id} icon is not WebP`);
    assert.equal(bytes.subarray(8, 12).toString("ascii"), "WEBP", `${id} icon is not WebP`);
    return;
  }
  assert.equal(extension, ".ico", `${id} iconFile has an unsupported extension`);
  assert.equal(bytes.subarray(0, 4).toString("hex"), "00000100", `${id} icon is not ICO`);
}

export function isAllowedHTTPSHost(value) {
  if (typeof value !== "string" || value !== value.toLowerCase()) return false;
  const host = value.startsWith("*.") ? value.slice(2) : value;
  return !host.includes("*")
    && /^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?)+$/.test(host)
    && host !== "localhost"
    && !host.endsWith(".local");
}

/** Validate one open-source content or tracker extension package. */
export async function assertExtensionPackage(directory, expectedKind, expectedID) {
  const metadata = JSON.parse(await readFile(join(directory, "extension.json"), "utf8"));
  const source = await readFile(join(directory, "src", "lib.rs"), "utf8");
  assert.equal(metadata.runtime, "wasm");
  assert.match(metadata.crate, /^[a-z][a-z0-9-]+$/);
  await readFile(join(directory, "Cargo.toml"));

  assert.equal(metadata.id, expectedID);
  assert.match(metadata.id, /^[A-Za-z0-9._-]{1,128}$/);
  assert.ok(["content", "tracker"].includes(expectedKind), `unsupported extension kind ${expectedKind}`);
  assert.equal(metadata.kind, expectedKind);
  assert.ok(SUPPORTED_API_VERSIONS.has(metadata.apiVersion), `unsupported API version ${metadata.apiVersion}`);
  assertNonemptyString(metadata.name, "name");
  assertNonemptyString(metadata.description, "description");
  assert.match(metadata.version, /^[A-Za-z0-9._-]{1,128}$/, "version must be path-safe");
  assertNonemptyString(metadata.language, "language");
  assertStringArray(metadata.languages, "languages", undefined, { required: true });
  assert.ok(metadata.languages.includes(metadata.language), "languages must contain language");
  assert.ok(contentRatings.has(metadata.contentRating), `unsupported content rating ${metadata.contentRating}`);
  assert.ok(availabilities.has(metadata.availability), `unsupported availability ${metadata.availability}`);
  assertStringArray(metadata.capabilities, "capabilities", repositoryCapabilities);
  assertStringArray(metadata.permissions, "permissions", extensionPermissions, { required: true });
  assert.ok(metadata.permissions.includes("network"), "network permission is required by this catalog");
  if (metadata.connectorKind !== undefined) {
    assert.equal(expectedKind, "content", "connectors must be content extensions");
    assert.ok(["opds", "komga", "kavita", "suwayomi"].includes(metadata.connectorKind), "unsupported connector profile");
  }
  assertStringArray(metadata.allowedHTTPSHosts, "allowedHTTPSHosts", undefined, { required: metadata.connectorKind === undefined });
  assert.ok(metadata.allowedHTTPSHosts.every(isAllowedHTTPSHost), "allowedHTTPSHosts must contain exact public lower-case hostnames or explicit *. subdomain patterns");
  if (expectedKind === "tracker") {
    assert.ok(metadata.accountPolicy?.requests?.length, "A native account policy is required");
    const auth = metadata.accountPolicy.authentication;
    assert.ok(["oauth2PKCE", "oauth2Implicit"].includes(auth?.mode));
    assertNonemptyString(auth.authorizationURL, "authorizationURL");
    if (auth.mode === "oauth2PKCE") assertNonemptyString(auth.tokenURL, "tokenURL");
    for (const value of [auth.authorizationURL, auth.tokenURL].filter(Boolean)) {
      const url = new URL(value);
      assert.equal(url.protocol, "https:");
      assert.ok(metadata.allowedHTTPSHosts.includes(url.hostname));
      assert.ok(!url.username && !url.password && !url.search && !url.hash && (!url.port || url.port === "443"));
    }
    assert.equal(auth.callbackScheme, "manko");
    const callback = new URL(auth.callbackURL);
    assert.equal(callback.protocol, "manko:");
    assert.equal(callback.host, "oauth");
    assert.ok(!callback.username && !callback.password && !callback.search && !callback.hash);
    assert.ok(metadata.accountPolicy.requests.length <= 32);
    for (const rule of metadata.accountPolicy.requests) {
      const origin = new URL(rule.origin);
      assert.equal(origin.protocol, "https:");
      assert.equal(origin.pathname, "/");
      assert.ok(metadata.allowedHTTPSHosts.includes(origin.hostname));
      assert.ok(!origin.username && !origin.password && !origin.search && !origin.hash && !origin.port);
      assert.ok(rule.pathPrefix.startsWith("/") && !/[%;\\\x00-\x1f\x7f]/.test(rule.pathPrefix)
        && !rule.pathPrefix.split("/").some(part => part === "." || part === ".."));
      assertStringArray(rule.methods, "accountPolicy.methods", new Set(["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"]), {required:true});
      assertStringArray(rule.operations, "accountPolicy.operations", new Set(["trackerSearch", "progress", "progressBatch", "update", "collections"]), {required:true});
      assert.ok(rule.methods.every(method => ["GET", "HEAD"].includes(method)) || rule.bodyKind === "graphqlQuery" ||
        (rule.operations.length === 1 && rule.operations[0] === "update"), "Mutating requests must declare update access");
      if (rule.bodyKind !== undefined) {
        assert.ok(["graphqlQuery", "graphqlMutation"].includes(rule.bodyKind));
        assert.deepEqual(rule.methods, ["POST"]);
        if (rule.bodyKind === "graphqlMutation") assert.deepEqual(rule.operations, ["update"]);
      }
    }
  }
  assertStringArray(metadata.authenticationModes, "authenticationModes", authenticationModes, { required: true });
  assert.ok(Array.isArray(metadata.developers) && metadata.developers.length > 0, "developers must not be empty");
  metadata.developers.forEach((developer, index) => assertNonemptyString(developer?.name, `developers[${index}].name`));

  assert.equal(basename(metadata.iconFile), metadata.iconFile, "iconFile must be a package-local filename");
  const iconBytes = await readFile(join(directory, metadata.iconFile));
  assertIcon(iconBytes, extname(metadata.iconFile).toLowerCase(), expectedID);

  if (metadata.license?.startsWith("GPL-")) {
    const license = await readFile(join(directory, "LICENSE"), "utf8");
    assert.match(license, /GNU GENERAL PUBLIC LICENSE/, `${expectedID} must include its declared GPL license`);
  }

  assert.match(source, /export_extension!\s*\(/, "missing WASM ABI export");
  return metadata;
}

export const assertContentExtension = (directory, expectedID) => assertExtensionPackage(directory, "content", expectedID);
export const assertTrackerExtension = (directory, expectedID) => assertExtensionPackage(directory, "tracker", expectedID);
export const assertExtensionScaffold = (directory, expectedKind, expectedID) => {
  return assertExtensionPackage(directory, expectedKind, expectedID);
};
