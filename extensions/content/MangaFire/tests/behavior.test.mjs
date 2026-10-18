import test from "node:test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {resolve} from "node:path";
import {loadContentExtension, runtimeResponse, metadataCacheFixture, MINIMAL_WEBP_BYTES} from "../../test-runtime.mjs";
const main=resolve(import.meta.dirname,"../main.wasm");
import { fixture, vectors, meta, reply, responder } from "./fixtures.mjs";
test("MangaFire signs Unicode searches and repeated filters with the upstream vector", async () => {
  const loaded = await loadContentExtension(main, responder);
  const result = await loaded.extension.search({title: "海 & + story", selections: [
    {fieldID: "genre", value: "1"}, {fieldID: "genre", value: "78"}, {fieldID: "language", value: "en"},
  ]});
  const url = new URL(loaded.calls[0].url);
  assert.equal(url.searchParams.get("vrf"), vectors[3].token);
  assert.equal(url.searchParams.get("keyword"), "海 & + story");
  assert.deepEqual(url.searchParams.getAll("genres_in[]"), ["1", "78"]);
  assert.equal(result.items.length, 1);
  assert.equal(result.items[0].workId, "demo1");
  assert.equal(result.items[0].title, "Sample & Story");
  assert.deepEqual(result.metadata, {page: 2});
  const last = await loaded.extension.search({title: "海 & + story", metadata: result.metadata});
  assert.deepEqual(last, {items: [], metadata: null});
});
test("MangaFire uses native discovery shelves and reuses metadata across runtime restarts", async () => {
  const cache = metadataCacheFixture(), loaded = await loadContentExtension(main, responder, {cache});
  const sections = await loaded.extension.discoverSections();
  assert.deepEqual(sections.map(s => s.presentation), ["carousel", "carousel", "carousel", "latestGallery"]);
  await loaded.extension.discover({sectionId: "trending"});
  const url = new URL(loaded.calls[0].url);
  assert.equal(url.pathname, "/api/top-titles");
  assert.equal(url.searchParams.get("type"), "trending");
  assert.equal(url.searchParams.get("days"), "7");
  const calls = loaded.calls.length;
  assert.deepEqual(await loaded.extension.discover({sectionId: "trending", metadata: {page: 2}}), {items: [], metadata: null});
  assert.equal(loaded.calls.length, calls);
  for (const [sectionId, days] of [["trending-day", "1"], ["trending-month", "30"]]) {
    await loaded.extension.discover({sectionId});
    assert.equal(new URL(loaded.calls.at(-1).url).searchParams.get("days"), days);
    const count = loaded.calls.length;
    assert.deepEqual(await loaded.extension.discover({sectionId, metadata: {page: 2}}), {items: [], metadata: null});
    assert.equal(loaded.calls.length, count);
  }
  await loaded.extension.discover({sectionId: "latest"});
  assert.equal(new URL(loaded.calls.at(-1).url).searchParams.get("order[chapter_updated_at]"), "desc");
  const restarted = await loadContentExtension(main, responder, {cache});
  await restarted.extension.discover({sectionId: "latest"});
  assert.equal(restarted.calls.length, 0);
  assert.ok([...cache.entries.keys()].every(k => !k.includes("vrf")));
});
test("MangaFire preserves multilingual releases, chapter pagination and reader identity", async () => {
  const loaded = await loadContentExtension(main, responder, {cache: metadataCacheFixture()});
  const detail = await loaded.extension.details({workID: "demo1"});
  assert.equal(detail.workInfo.synopsis, "An original sample synopsis & adventure.");
  assert.equal(detail.workInfo.author, "Sample Author");
  assert.equal(detail.workInfo.artist, "Sample Artist");
  assert.equal(detail.workInfo.status, "hiatus");
  assert.deepEqual(detail.workInfo.secondaryTitles, ["Sample Adventure"]);
  const chapters = await loaded.extension.installments({sourceWork: detail});
  assert.equal(chapters.length, 3);
  assert.deepEqual(chapters.map(c => c.langCode), ["en", "fr", "ja"]);
  assert.equal(chapters[0].number, 2.5);
  assert.equal(chapters[0].publishDate, "2026-10-08T16:00:00.000Z");
  const calls = loaded.calls.length;
  await loaded.extension.installments(detail);
  assert.equal(loaded.calls.length, calls);
  const pages = await loaded.extension.imagePages({extensionInstallment: chapters[0]});
  assert.equal(pages.pages.length, 2);
  assert.equal(pages.id, "demo1/701");
  await loaded.extension.imagePages(chapters[0]);
  assert.equal(loaded.calls.length, calls + 2, "page URLs are refreshed at every reader open");
  const image = await loaded.extension.imagePageContent({url: pages.pages[0]});
  assert.equal(image.mimeType, "image/webp");
  assert.equal(loaded.calls.at(-1).headers.Referer, "https://mangafire.to/");
  assert.equal(loaded.calls.at(-1).imageResource, true);
  await assert.rejects(loaded.extension.imagePages({...chapters[0], workId: "other"}), {type: "invalidIdentifier"});
});
test("MangaFire validates filters, pagers and path identifiers before requesting", async () => {
  const loaded = await loadContentExtension(main, responder);
  for (const input of [
    {sort: "bad"}, {selections: [{fieldID: "type", value: "evil"}]},
    {selections: [{fieldID: "status", value: "finished", polarity: "exclude"}]},
    {selections: [{fieldID: "language", value: "invalid"}]}, {query: "x".repeat(501)},
  ]) await assert.rejects(loaded.extension.search(input), {type: "invalidSearchTerm"});
  await assert.rejects(loaded.extension.search({metadata: {page: 0}}), {type: "invalidCursor"});
  await assert.rejects(loaded.extension.discover({sectionId: "missing"}), {type: "invalidSection"});
  await assert.rejects(loaded.extension.details("../demo1"), {type: "invalidIdentifier"});
  await assert.rejects(loaded.extension.imagePages({workId: "demo1", installmentId: "demo1/0701"}), {type: "invalidIdentifier"});
  assert.equal(loaded.calls.length, 0);
});
test("MangaFire rejects foreign reader data and unsafe CDN URLs", async () => {
  for (const data of [
    {...fixture.pages, title: {hid: "other"}}, {...fixture.pages, id: 702}, {...fixture.pages, pages: []},
    {...fixture.pages, pages: [{url: "https://mfcdn3.xyz.evil.test/page.jpg"}]},
  ]) {
    const loaded = await loadContentExtension(main, r => reply(r, {data}));
    await assert.rejects(loaded.extension.imagePages({workId: "demo1", installmentId: "demo1/701"}));
  }
  const loaded = await loadContentExtension(main, responder);
  for (const url of ["https://evil.test/page.jpg", "https://mfcdn3.xyz.evil.test/a", "https://evilmfcdn3.xyz/a", "https://l1n.mfcdn1.xyz.evil.test/a", "https://evilmfcdn1.xyz/a", "https://mfcdn1.xyz/a", "https://nw8.mfcdn2.xyz.evil.test/a", "https://evilmfcdn2.xyz/a", "https://mfcdn2.xyz/a", "http://l1n.mfcdn1.xyz/a", "http://m3z.mfcdn3.xyz/a", "https://user@m3z.mfcdn3.xyz/a", "https://m3z.mfcdn3.xyz:444/a"]) {
    await assert.rejects(loaded.extension.imagePageContent({url}), {type: "hostNotAllowed"});
  }
  assert.equal(loaded.calls.length, 0);
});
for (const host of ["l1n.mfcdn1.xyz", "nw8.mfcdn2.xyz"]) test(`MangaFire accepts the observed ${host} CDN without loosening DNS boundaries`, async () => {
  const pages = {...fixture.pages, pages: [{url: `https://${host}/chapter/page.webp`}]};
  const loaded = await loadContentExtension(main, request => request.imageResource
    ? runtimeResponse({url: request.url, mimeType: "image/webp", bytes: MINIMAL_WEBP_BYTES})
    : reply(request, {data: pages}));
  const sequence = await loaded.extension.imagePages({workId: "demo1", installmentId: "demo1/701"});
  assert.deepEqual(sequence.pages, [`https://${host}/chapter/page.webp`]);
  await loaded.extension.imagePageContent({url: sequence.pages[0]});
  assert.equal(loaded.calls.at(-1).headers.Referer, "https://mangafire.to/");
});
test("MangaFire reports challenges, cooldowns, changed signing and malformed responses", async () => {
  const challenged = await loadContentExtension(main, r => runtimeResponse({url: r.url, status: 403, headers: {"cf-mitigated": "challenge"}, text: "<html>Just a moment...</html>"}));
  await assert.rejects(challenged.extension.search({query: "sample"}), {type: "challengeRequired"});
  assert.equal(challenged.challenges.length, 1);
  const limited = await loadContentExtension(main, r => runtimeResponse({url: r.url, status: 429, headers: {"Retry-After": "30"}}));
  await assert.rejects(limited.extension.search({query: "sample"}), e => e.type === "rateLimited" && e.retryAfterSeconds === 30);
  const rejected = await loadContentExtension(main, r => runtimeResponse({url: r.url, status: 403, text: '{"message":"Invalid token."}'}));
  await assert.rejects(rejected.extension.search({query: "sample"}), {type: "serviceUnavailable"});
  const foreign = await loadContentExtension(main, () => runtimeResponse({url: "https://evil.test/login", text: "{}"}));
  await assert.rejects(foreign.extension.search({query: "sample"}), {type: "hostNotAllowed"});
  for (const data of [{items: [fixture.series], meta: {page: 1, lastPage: 2, hasNext: false}}, {items: [fixture.series]}, {}]) {
    const loaded = await loadContentExtension(main, r => reply(r, data));
    await assert.rejects(loaded.extension.search({query: "sample"}), {type: "invalidResponse"});
  }
});
test("MangaFire genre suggestions cache options and encode excluded genres", async () => {
  const loaded = await loadContentExtension(main, responder, {cache: metadataCacheFixture()});
  const suggestions = await loaded.extension.searchSuggestions({fieldID: "genre", query: "act"});
  assert.deepEqual(suggestions, [{fieldID: "genre", value: "1", title: "Action"}]);
  await loaded.extension.searchSuggestions({fieldID: "genre", query: "78"});
  assert.equal(loaded.calls.length, 1);
  await loaded.extension.search({selections: [{fieldID: "genre", value: "1", polarity: "exclude"}]});
  assert.equal(new URL(loaded.calls.at(-1).url).searchParams.get("genres_ex[]"), "1");
});

for (const mimeType of ["image/jpg", "IMAGE/JPG; charset=binary"]) test(`MangaFire normalizes ${mimeType} for live covers and preserves native handles`, async () => {
 const loaded = await loadContentExtension(main, request => ({...runtimeResponse({url:request.url,mimeType}),resourceID:"native-cover"}));
 const image = await loaded.extension.imagePageContent({url:"https://static.mfcdn.nl/cover.jpg"});
 assert.equal(image.mimeType,"image/jpeg"); assert.equal(image.resourceID,"native-cover"); assert.equal(image.dataBase64,undefined);
 const invalid=await loadContentExtension(main,request=>runtimeResponse({url:request.url,mimeType:"text/html",text:"not an image"}));
 await assert.rejects(invalid.extension.imagePageContent({url:"https://static.mfcdn.nl/cover.jpg"}),{type:"invalidResponse"});
});
