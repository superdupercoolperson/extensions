/* Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0 */

import test from "node:test";
import assert from "node:assert/strict";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
import { MINIMAL_WEBP_BYTES, loadContentExtension, runtimeResponse, metadataCacheFixture } from "../../test-runtime.mjs";

const mainPath = resolve(dirname(fileURLToPath(import.meta.url)), "../main.wasm");
const manifest = JSON.parse(await readFile(resolve(dirname(mainPath), "extension.json"), "utf8"));
const expectedUserAgent = "manko NHentai Extension/2.0.0 (+https://github.com/superdupercoolperson/extensions)";
const listGallery = {
  id: 101,
  media_id: "9001",
  english_title: "Sanitized Sample Gallery",
  japanese_title: "Sample Alt",
  thumbnail: "/galleries/9001/thumb.webp",
  thumbnail_width: 150,
  thumbnail_height: 200,
  num_pages: 3,
  num_favorites: 12,
  tag_ids: [1, 2]
};
const detailGallery = {
  id: 101,
  media_id: "9001",
  title: { english: "Sanitized Sample Gallery", japanese: "Sample Alt", pretty: "Sanitized Sample Gallery" },
  cover: { path: "https://t.nhentai.net/galleries/9001/cover.webp.webp", width: 300, height: 400 },
  thumbnail: { path: "/galleries/9001/thumb.webp", width: 150, height: 200 },
  pages: [
    { number: 1, path: "/galleries/9001/1.jpg" },
    { number: 2, path: "galleries/9001/2.png" },
    { number: 3, path: "https://i.nhentai.net/galleries/9001/3.webp.webp" }
  ],
  tags: [
    { type: "language", name: "english" },
    { type: "artist", name: "Sample Creator" },
    { type: "tag", name: "landscape" },
    { type: "parody", name: "Sample Parody" },
    { type: "character", name: "Sample Character" },
    { type: "group", name: "Sample Group" },
    { type: "category", name: "doujinshi" }
  ],
  num_pages: 3,
  num_favorites: 12,
  upload_date: 1700000000
};

function jsonResponse(request, value, status = 200, headers = {}) {
  assert.equal(request.headers["User-Agent"], expectedUserAgent);
  assert.equal(request.headers.Referer, "https://nhentai.net/");
  return runtimeResponse({ url: request.url, status, headers, text: JSON.stringify(value) });
}

function assertDeclaredCover(item) {
  const url = new URL(item.imageUrl);
  assert.equal(url.protocol, "https:");
  assert.ok(manifest.allowedHTTPSHosts.includes(url.hostname), `undeclared cover host ${url.hostname}`);
}

test("NHentai maps v2 returned paths directly and brokers both covers and pages", async () => {
  const loaded = await loadContentExtension(mainPath, request => {
    const url = new URL(request.url);
    assert.notEqual(url.pathname, "/api/v2/cdn", "v0.3 must not depend on the retired CDN-discovery endpoint");
    if (url.pathname === "/api/v2/search") {
      assert.equal(url.searchParams.get("query"), "sample");
      assert.equal(url.searchParams.get("sort"), "date");
      assert.equal(url.searchParams.get("page"), "2");
      return jsonResponse(request, { result: [listGallery], num_pages: 3, per_page: 25, total: 60 });
    }
    if (url.pathname === "/api/v2/galleries/101") return jsonResponse(request, detailGallery);
    if (url.hostname === "i.nhentai.net" || url.hostname === "t.nhentai.net") {
      return runtimeResponse({ url: request.url, mimeType: "image/webp", bytes: MINIMAL_WEBP_BYTES });
    }
    throw new Error(`Unexpected request ${request.url}`);
  });

  const page = await loaded.extension.search({ query: "sample", metadata: { page: 2 } });
  assert.equal(page.items[0].type, "work");
  assert.equal(page.items[0].workId, "101");
  assert.equal(page.items[0].imageUrl, "https://t.nhentai.net/galleries/9001/thumb.webp");
  page.items.forEach(assertDeclaredCover);
  assert.equal(page.metadata.page, 3);
  const cover = await loaded.extension.imagePageContent({ url: page.items[0].imageUrl });
  assert.equal(cover.mimeType, "image/webp");

  const work = await loaded.extension.details("101");
  assert.equal(work.imageUrl, "https://t.nhentai.net/galleries/9001/cover.webp.webp");
  assert.equal(work.workInfo.shareUrl, "https://nhentai.net/g/101/");
  assert.equal(work.workInfo.artist, "Sample Creator");
  assert.equal(work.workInfo.author, "Sample Creator, Sample Group");
  assert.ok(work.workInfo.searchFacets.some(facet => facet.fieldID === "artist" && facet.presentation === "creator"));
  assert.ok(work.workInfo.searchFacets.some(facet => facet.fieldID === "tag" && facet.presentation === "tag"));
  assert.deepEqual(
    Array.from(new Set(work.workInfo.searchFacets.map(facet => facet.groupTitle))),
    ["Parodies", "Characters", "Tags", "Artists", "Groups", "Languages", "Categories"]
  );
  const [installment] = await loaded.extension.installments(work);
  const sequence = await loaded.extension.imagePages(installment);
  assert.deepEqual(Array.from(sequence.pages), [
    "https://i.nhentai.net/galleries/9001/1.jpg",
    "https://i.nhentai.net/galleries/9001/2.png",
    "https://i.nhentai.net/galleries/9001/3.webp.webp"
  ]);
  const image = await loaded.extension.imagePageContent({ url: sequence.pages[2] });
  assert.equal(image.mimeType, "image/webp");
  assert.equal(image.dataBase64, Buffer.from(MINIMAL_WEBP_BYTES).toString("base64"));
});

test("NHentai uses live typed suggestions, caches them, and falls back to observed facets", async () => {
  let suggestionRequests = 0;
  const loaded = await loadContentExtension(mainPath, request => {
    const url = new URL(request.url);
    if (url.pathname === "/api/v2/tags/search") {
      suggestionRequests++;
      assert.equal(request.method, "POST");
      assert.equal(request.headers["Content-Type"], "application/json");
      const body = request.body;
      assert.equal(body.query, "big breasts");
      assert.equal(body.limit, 30);
      assert.equal(body.type, suggestionRequests === 1 ? null : "tag");
      return jsonResponse(request, [
        { id: 1, type: "tag", name: "big breasts", slug: "big-breasts", count: 231086 },
        { id: 2, type: "unknown", name: "ignored", slug: "ignored", count: 1 }
      ]);
    }
    if (url.pathname === "/api/v2/galleries/101") return jsonResponse(request, detailGallery);
    throw new Error(`Unexpected request ${request.url}`);
  });

  const crossField = await loaded.extension.searchSuggestions({ query: "big breasts", limit: 5 });
  assert.deepEqual(Array.from(crossField, suggestion => [suggestion.fieldID, suggestion.value, suggestion.subtitle]), [
    ["tag", "big breasts", "231086 galleries"]
  ]);
  await loaded.extension.searchSuggestions({ query: "big breasts", limit: 5 });
  assert.equal(suggestionRequests, 1, "identical cross-field suggestions are cached");

  const typed = await loaded.extension.searchSuggestions({ fieldID: "tag", query: "big breasts", limit: 5 });
  assert.deepEqual(Array.from(typed, suggestion => suggestion.value), ["big breasts"]);
  assert.equal(suggestionRequests, 1, "a canonical tag learned from cross-field lookup needs no second request");

  await loaded.extension.details("101");
  const fallback = await loaded.extension.searchSuggestions({ fieldID: "artist", query: "sample", limit: 5 });
  assert.deepEqual(Array.from(fallback, suggestion => suggestion.value), ["Sample Creator"]);
});

test("NHentai composes structured selections and forwards sort", async () => {
  const loaded = await loadContentExtension(mainPath, request => {
    const url = new URL(request.url);
    if (url.pathname === "/api/v2/galleries/101") return jsonResponse(request, detailGallery);
    if (url.pathname === "/api/v2/search") {
      assert.equal(url.searchParams.get("query"), 'sample artist:"Sample Creator" -tag:blocked');
      assert.equal(url.searchParams.get("sort"), "popular-week");
      return jsonResponse(request, { result: [], num_pages: 1, per_page: 25, total: 0 });
    }
    throw new Error(`Unexpected request ${request.url}`);
  });
  const configuration = loaded.extension.searchFilters();
  assert.ok(configuration.fields.some(field => field.id === "parody" && field.supportsExclusion));
  assert.ok(configuration.fields.some(field => field.id === "uploaded" && !field.supportsExclusion));
  await loaded.extension.search({
    query: "sample",
    sort: "popular-week",
    selections: [
      { fieldID: "artist", value: "Sample Creator", title: "Sample Creator", polarity: "include" },
      { fieldID: "tag", value: "blocked", title: "blocked", polarity: "exclude" }
    ]
  });
});

test("NHentai keeps its homepage and opts into a catalog only after sorting", async () => {
  const loaded = await loadContentExtension(mainPath, request => {
    const url = new URL(request.url);
    if (url.pathname === "/api/v2/galleries/popular") return jsonResponse(request, [listGallery]);
    assert.equal(url.pathname, "/api/v2/galleries");
    assert.equal(url.searchParams.get("page"), "1");
    return jsonResponse(request, { result: [listGallery], num_pages: 2 });
  });
  const configuration = loaded.extension.searchFilters();
  assert.equal(configuration.sortedBrowseSectionID, "latest");
  assert.equal(configuration.defaultSortID, "date");
  assert.deepEqual(Array.from(configuration.sortOptions, option => option.id), ["date", "popular-today", "popular-week", "popular"]);
  assert.deepEqual(Array.from(loaded.extension.discoverSections(), section => [section.id, section.title, section.type]), [
    ["latest", "Latest", 0], ["popular", "Popular Today", 0]
  ]);
  await loaded.extension.discover({ sectionId: "latest" });
  await loaded.extension.discover({ sectionId: "popular" });
  assert.equal(loaded.calls.length, 2);
});

test("NHentai all four catalog orders preserve ranking, filters and page cursors", async () => {
  const orders = { date: [303, 301, 302], "popular-today": [302, 303, 301], "popular-week": [301, 303, 302], popular: [302, 301, 303] };
  const loaded = await loadContentExtension(mainPath, request => {
    const url = new URL(request.url);
    assert.equal(url.pathname, "/api/v2/search");
    // Reproduce the live API's rejection instead of accepting empty test queries.
    if (url.searchParams.get("query") === "") return jsonResponse(request, { error: "query is required" }, 400);
    const page = Number(url.searchParams.get("page"));
    const ids = orders[url.searchParams.get("sort")].map(id => id + (page - 1) * 10);
    return jsonResponse(request, { result: ids.map(id => ({ ...listGallery, id })), num_pages: 2 });
  });
  const contexts = [
    { query: "", expected: " " },
    { query: "   ", expected: " " },
    { selections: [{ fieldID: "language", value: "english", polarity: "include" }], expected: "language:english" },
    { query: "landscape", selections: [{ fieldID: "tag", value: "scenery", polarity: "include" }, { fieldID: "artist", value: "Sample Creator", polarity: "exclude" }], expected: 'landscape tag:scenery -artist:"Sample Creator"' }
  ];
  for (const [sort, ids] of Object.entries(orders)) {
    for (const { expected, ...context } of contexts) {
      for (const method of ["discover", "search"]) {
        const first = await loaded.extension[method]({ sectionId: "latest", sort, ...context });
        assert.equal(new URL(loaded.calls.at(-1).url).searchParams.get("query"), expected);
        assert.deepEqual(Array.from(first.items, item => item.workId), ids.map(String));
        assert.equal(first.metadata.page, 2);
        const second = await loaded.extension[method]({ sectionId: "latest", sort, ...context, cursor: first.metadata });
        assert.equal(new URL(loaded.calls.at(-1).url).searchParams.get("page"), "2");
        assert.deepEqual(Array.from(second.items, item => item.workId), ids.map(id => String(id + 10)));
        assert.equal(second.metadata, null);
      }
    }
  }
});

test("NHentai empty sorted rankings finish without falling back to newest", async () => {
  const loaded = await loadContentExtension(mainPath, request => {
    const url = new URL(request.url);
    assert.equal(url.pathname, "/api/v2/search");
    assert.equal(url.searchParams.get("query"), " ");
    return jsonResponse(request, { result: [], num_pages: 0 });
  });
  for (const sort of ["date", "popular-today", "popular-week", "popular"]) {
    for (const method of ["discover", "search"]) {
      const page = await loaded.extension[method]({ sectionId: "latest", sort });
      assert.deepEqual(Array.from(page.items), []);
      assert.equal(page.metadata, null);
    }
  }
  assert.equal(loaded.calls.length, 8);
});

test("NHentai uses the unpaged v2 popular endpoint", async () => {
  const loaded = await loadContentExtension(mainPath, request => {
    const url = new URL(request.url);
    if (url.pathname === "/api/v2/galleries/popular") return jsonResponse(request, [listGallery]);
    throw new Error(`Unexpected request ${request.url}`);
  });
  const first = await loaded.extension.discover({ sectionId: "popular" });
  assert.equal(first.items.length, 1);
  first.items.forEach(assertDeclaredCover);
  assert.equal(first.metadata, null);
  const callsBefore = loaded.calls.length;
  const second = await loaded.extension.discover({ sectionId: "popular", metadata: { page: 2 } });
  assert.deepEqual(Array.from(second.items), []);
  assert.equal(second.metadata, null);
  assert.equal(loaded.calls.length, callsBefore);
});

test("NHentai hands only definitive Cloudflare challenges to manko", async () => {
  const challenged = await loadContentExtension(mainPath, request => runtimeResponse({
    url: request.url,
    status: 403,
    headers: { server: "cloudflare", "cf-mitigated": "challenge" },
    text: "challenge page"
  }));
  await assert.rejects(
    () => challenged.extension.discover({ sectionId: "latest" }),
    error => error.name === "ChallengeRequiredError" && error.type === "challengeRequired"
  );
  assert.deepEqual(challenged.challenges, ["https://nhentai.net/"]);

  const ordinary = await loadContentExtension(mainPath, request => runtimeResponse({
    url: request.url,
    status: 403,
    headers: { server: "cloudflare", "cf-ray": "sanitized" },
    text: "access denied"
  }));
  await assert.rejects(
    () => ordinary.extension.discover({ sectionId: "latest" }),
    error => error.name === "ServiceError" && error.type === "serviceError"
  );
  assert.deepEqual(ordinary.challenges, []);
});

test("NHentai performs one bounded rate-limit retry", async () => {
  let attempts = 0;
  const loaded = await loadContentExtension(mainPath, request => {
    const url = new URL(request.url);
    if (url.pathname === "/api/v2/galleries") {
      attempts++;
      if (attempts === 1) return runtimeResponse({ url: request.url, status: 429, headers: { "Retry-After": "2" } });
      return jsonResponse(request, { result: [], num_pages: 1, per_page: 25, total: 0 });
    }
    throw new Error(`Unexpected request ${request.url}`);
  });
  const result = await loaded.extension.discover({ sectionId: "latest" });
  assert.deepEqual(Array.from(result.items), []);
  assert.equal(attempts, 2);
  assert.deepEqual(loaded.sleeps, [2000]);
});

test("NHentai fails closed on malformed JSON, traversal, and undeclared returned media hosts", async () => {
  let mode = "json";
  const loaded = await loadContentExtension(mainPath, request => {
    if (mode === "json") return runtimeResponse({ url: request.url, text: "{" });
    if (mode === "notFound") return runtimeResponse({ url: request.url, status: 404 });
    return jsonResponse(request, { ...detailGallery, pages: [{ number: 1, path: "../outside.webp" }] });
  });
  await assert.rejects(() => loaded.extension.details("not-a-number"), error => error.name === "InvalidIdentifierError");
  await assert.rejects(() => loaded.extension.details("101"), error => error.name === "InvalidResponseError");
  mode = "notFound";
  await assert.rejects(() => loaded.extension.details("101"), error => error.name === "NotFoundError");
  mode = "path";
  await assert.rejects(() => loaded.extension.details("101"), error => error.name === "InvalidResponseError");

  const badHostGallery = { ...listGallery, thumbnail: "https://outside.example/thumb.webp" };
  const badHost = await loadContentExtension(mainPath, request => jsonResponse(request, { result: [badHostGallery], num_pages: 1, per_page: 25, total: 1 }));
  await assert.rejects(() => badHost.extension.discover({ sectionId: "latest" }), error => error.name === "HostNotAllowedError");
  const callsBefore = badHost.calls.length;
  await assert.rejects(() => badHost.extension.imagePageContent({ url: "https://outside.example/page.webp" }), error => error.name === "HostNotAllowedError");
  assert.equal(badHost.calls.length, callsBefore);
});

test("every NHentai filter preserves canonical query syntax, sort and pagination",async()=>{
 const loaded=await loadContentExtension(mainPath,request=>jsonResponse(request,{result:[],num_pages:3,per_page:25,total:0}));
 const config=loaded.extension.searchFilters();
 for(const field of config.fields){
  const value=field.id==="language"?"english":field.inputKind==="number"?"20":field.id==="uploaded"?"7d":"blue sky";
  for(const polarity of field.supportsExclusion?["include","exclude"]:["include"]){
   for(const scope of ["search","discover"]){
    for(const page of [1,2]){
     await loaded.extension[scope]({sectionId:"latest",query:"titleword",selections:[{fieldID:field.id,value,polarity}],sort:"popular-week",metadata:{page}});
     const url=new URL(loaded.calls.at(-1).url);
     assert.equal(url.searchParams.get("query"),`titleword ${polarity==="exclude"?"-":""}${field.id}:${value.includes(" ")?JSON.stringify(value):value}`);
     assert.equal(url.searchParams.get("page"),String(page));assert.equal(url.searchParams.get("sort"),"popular-week");
    }
   }
  }
 }
 await assert.rejects(loaded.extension.search({selections:[{fieldID:"pages",value:"-2",polarity:"include"}]}),/whole number/);
});


test("NHentai coalesces chapter metadata, reuses full search payloads, and clears on refresh", async () => {
  const loaded = await loadContentExtension(mainPath, request => {
    if (new URL(request.url).pathname.endsWith("/search")) return jsonResponse(request, { result: [detailGallery], num_pages: 1 });
    return jsonResponse(request, detailGallery);
  });
  await loaded.extension.search({ query: "sample" });
  await loaded.extension.details("101");
  assert.equal(loaded.calls.length, 1, "complete search metadata should prepare the chapter without another request");
  await loaded.extension.invalidateCache();
  await Promise.all([loaded.extension.details("101"), loaded.extension.details("101")]);
  assert.equal(loaded.calls.length, 2, "concurrent details must share one request");
});

test("NHentai passes native resource references through without copying image bytes", async () => {
  const loaded = await loadContentExtension(mainPath, () => { throw new Error("unexpected legacy request"); });
  const result = await loaded.extension.imagePageContent({
    url: "https://i.nhentai.net/galleries/9001/1.jpg",
    http: { imageResource: async request => {
      assert.equal(request.headers.Referer, "https://nhentai.net/");
      assert.equal(request.headers["User-Agent"], expectedUserAgent);
      return { status: 200, resourceID: "owned-image", mimeType: "image/jpeg", headers: {} };
    } }
  });
  assert.equal(result.resourceID, "owned-image");
  assert.equal(result.dataBase64, undefined);
});


test("NHentai expires chapter metadata after thirty minutes and does not revive cache entries after refresh", async () => {
  let now = 0;
  let release;
  let ready;
  const started = new Promise(resolve => { ready = resolve; });
  let block = false;
  const loaded = await loadContentExtension(mainPath, async request => {
    if (block) await new Promise(resolve => { release = resolve; ready(); });
    return jsonResponse(request, detailGallery);
  }, { globals: { Date: class extends Date { static now() { return now; } } } });
  await loaded.extension.details("101");
  await loaded.extension.installments({ workId: "101" });
  assert.equal(loaded.calls.length, 1);
  now = 1800001;
  await loaded.extension.details("101");
  assert.equal(loaded.calls.length, 2);
  await loaded.extension.invalidateCache();
  block = true;
  const pending = loaded.extension.details("101");
  await started;
  await loaded.extension.invalidateCache();
  block = false;
  release();
  await pending;
  await loaded.extension.details("101");
  assert.equal(loaded.calls.length, 4);
});


test("NHentai complete search galleries persist across restart and targeted refresh keeps other galleries", async () => {
  const cache = metadataCacheFixture();
  const responder = request => jsonResponse(request, request.url.includes("/search") ? {result:[detailGallery], num_pages:1} : {...detailGallery,id:Number(/\/(\d+)$/.exec(request.url)?.[1] || 101)});
  const loaded = await loadContentExtension(mainPath, responder, {cache});
  await loaded.extension.search({query:"sample"});
  await loaded.extension.details("101");
  assert.equal(loaded.calls.length,1);
  const restarted = await loadContentExtension(mainPath, responder, {cache});
  const work = await restarted.extension.details("101");
  await restarted.extension.installments(work);
  assert.equal(restarted.calls.length,0);
  await restarted.extension.details("102");
  cache.setReload(true); await restarted.extension.details("101"); cache.setReload(false);
  await restarted.extension.details("102");
  assert.equal(restarted.calls.length,2);
});

test("NHentai partial cards never seed persisted chapter metadata", async () => {
  const cache = metadataCacheFixture();
  const partial = {...listGallery}; delete partial.pages;
  const loaded = await loadContentExtension(mainPath, request => jsonResponse(request, request.url.includes("/search") ? {result:[partial],num_pages:1} : detailGallery), {cache});
  await loaded.extension.search({query:"sample"});
  await loaded.extension.details("101");
  assert.equal(loaded.calls.length,2);
});
