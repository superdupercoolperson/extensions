/* SPDX-License-Identifier: Apache-2.0 */
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {resolve} from "node:path";
import {loadContentExtension, runtimeResponse, metadataCacheFixture, MINIMAL_WEBP_BYTES} from "../../test-runtime.mjs";
const main = resolve(import.meta.dirname, "../main.wasm");
const fixture = JSON.parse(await readFile(resolve(import.meta.dirname, "fixtures/api.json")));
const vectors = JSON.parse(await readFile(resolve(import.meta.dirname, "fixtures/signing.json")));
const meta = (page = 1, lastPage = 1) => ({page, lastPage, hasNext: page < lastPage});
const reply = (request, data) => runtimeResponse({url: request.url, text: JSON.stringify(data)});
function responder(request) {
  const url = new URL(request.url), page = Number(url.searchParams.get("page") || 1);
  assert.ok(url.searchParams.has("vrf") || request.imageResource);
  if (url.pathname === "/api/titles") return reply(request, {items: page === 1 ? [fixture.series, fixture.series] : [], meta: meta(page, 2)});
  if (url.pathname === "/api/top-titles") return reply(request, {items: [fixture.series]});
  if (url.pathname === "/api/titles/demo1") return reply(request, {data: fixture.series});
  if (url.pathname === "/api/titles/demo1/chapters") return reply(request, {items: page === 1 ? fixture.chapters.slice(0, 2) : [fixture.chapters[0], fixture.chapters[2]], meta: meta(page, 2)});
  if (url.pathname === "/api/chapters/701") return reply(request, {data: fixture.pages});
  if (url.pathname === "/api/filter-options") return reply(request, {data: {genres: fixture.genres}});
  if (request.imageResource) return runtimeResponse({url: request.url, mimeType: "image/webp", bytes: MINIMAL_WEBP_BYTES});
  throw new Error(`Unexpected request ${url.pathname}`);
}

export { fixture, vectors, meta, reply, responder };
