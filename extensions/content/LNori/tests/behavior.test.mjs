import test from "node:test";
import assert from "node:assert/strict";
import {resolve} from "node:path";
import {loadContentExtension,runtimeResponse,metadataCacheFixture,MINIMAL_WEBP_BYTES} from "../../test-runtime.mjs";
const main=resolve(import.meta.dirname,"../main.wasm");
import { workID, bookID, card, library, homepage, schema, series, bookSchema, book, responder } from "./fixtures.mjs";
test("LNORI caches bounded pages, preserves unique series and filters locally",async()=>{
 const cache=metadataCacheFixture();const loaded=await loadContentExtension(main,responder,{cache});
 const first=await loaded.extension.discover({sectionId:"library"});
 assert.equal(first.items.length,25);assert.equal(first.metadata.page,2);assert.equal(first.items[0].workId,workID);assert.equal(first.items[0].mediaKind,"lightNovel");
 const last=await loaded.extension.discover({sectionId:"library",metadata:first.metadata});assert.equal(last.items.length,1);assert.equal(last.metadata,null);assert.equal(loaded.calls.length,2);
 const search=await loaded.extension.search({query:"sample 3",selections:[{fieldID:"genre",value:"fantasy"}]});assert.equal(search.items.length,1);
 const excluded=await loaded.extension.search({selections:[{fieldID:"genre",value:"fantasy",polarity:"exclude"}]});assert.equal(excluded.items.length,0);
 const suggestions=await loaded.extension.searchSuggestions({fieldID:"genre",query:"fan"});assert.deepEqual(suggestions.map(s=>s.value),["fantasy"]);
 const restarted=await loadContentExtension(main,responder,{cache});await restarted.extension.search({query:"sample 3",selections:[{fieldID:"genre",value:"fantasy"}]});assert.equal(restarted.calls.length,0);
 cache.setReload(true);await restarted.extension.search({query:"sample 3",selections:[{fieldID:"genre",value:"fantasy"}]});assert.equal(restarted.calls.length,1);
});
test("LNORI volumes use the app's XHTML contract and remove executable markup",async()=>{
 const loaded=await loadContentExtension(main,responder,{cache:metadataCacheFixture()});
 const detail=await loaded.extension.details(workID);assert.equal(detail.workInfo.primaryTitle,"Sample & Story");assert.equal(detail.workInfo.mediaKind,"lightNovel");
 const volumes=await loaded.extension.installments(detail);assert.equal(volumes.length,1);assert.equal(volumes[0].format,"xhtml");
 const publication=await loaded.extension.publicationContent(volumes[0]);assert.equal(publication.mimeType,"application/xhtml+xml");assert.ok(publication.text.includes('<section id="chapter1">'));assert.ok(publication.text.includes("A &amp; B"));assert.ok(publication.text.includes('src="https://img.lnori.com/11-01.jpg"'));assert.ok(!publication.text.includes("bad()"));assert.ok(!publication.text.includes("srcset"));
 await loaded.extension.publicationContent(volumes[0]);assert.equal(loaded.calls.length,2);
 await loaded.extension.imagePageContent({url:"https://img.lnori.com/11-01.jpg"});assert.equal(loaded.calls.at(-1).imageResource,true);
 await assert.rejects(loaded.extension.publicationContent({...volumes[0],installmentId:"12/unrelated"}),{type:"invalidIdentifier"});
});
test("LNORI rejects bad paths and doesn't cache failed catalog responses",async()=>{
 let attempts=0;const loaded=await loadContentExtension(main,request=>runtimeResponse({url:request.url,text:++attempts===1?"<html>Broken library</html>":library}),{cache:metadataCacheFixture()});
 await assert.rejects(loaded.extension.search({query:""}),{type:"invalidResponse"});await loaded.extension.search({query:""});assert.equal(attempts,2);
 for(const id of ["1","../secret","1/%2e%2e","1/name?x=y"])await assert.rejects(loaded.extension.details(id),{type:"invalidIdentifier"});
 await assert.rejects(loaded.extension.search({metadata:{page:0}}),{type:"invalidCursor"});
 await assert.rejects(loaded.extension.imagePageContent({url:"https://evil.test/cover.jpg"}),{type:"hostNotAllowed"});
});

test("LNORI accepts legitimate long title slugs and binds books to the series metadata",async()=>{
 const titleSlug='a'.repeat(205);const source=card(1).replace('/1/sample-novel',`/1/${titleSlug}`);
 const loaded=await loadContentExtension(main,r=>runtimeResponse({url:r.url,text:source}));assert.equal((await loaded.extension.search({})).items[0].workId,`1/${titleSlug}`);
 const mismatch=await loadContentExtension(main,r=>runtimeResponse({url:r.url,text:new URL(r.url).pathname.startsWith('/series/')?series:book.replace('"value":"1"','"value":"2"')}));
 await assert.rejects(mismatch.extension.publicationContent({installmentId:bookID,workId:workID}),{type:'invalidResponse'});
});
test("LNORI large catalogs and long volumes retain all identities and reading text",async()=>{
 const largeLibrary=Array.from({length:884},(_,i)=>card(i+1)).join('');
 const paragraphs=Array.from({length:4000},(_,i)=>`<p>Original sample paragraph ${i} &amp; illustration notes.</p>`).join('');
 const largeBook=`<script type="application/ld+json">${JSON.stringify(bookSchema)}</script><h1 id="book-title">Original sample volume</h1><article class="content-body"><section id="chapter1">${paragraphs}</section></article>`;
 const loaded=await loadContentExtension(main,r=>runtimeResponse({url:r.url,text:new URL(r.url).pathname==='/library'?largeLibrary:new URL(r.url).pathname.startsWith('/series/')?series:largeBook}),{cache:metadataCacheFixture()});
 const last=await loaded.extension.search({metadata:{page:36}});assert.equal(last.items.length,9);assert.equal(last.metadata,null);
 const found=await loaded.extension.search({query:'Sample 884'});assert.equal(found.items[0].workId,'884/sample-884');assert.equal(loaded.calls.length,2);
 for(const {value} of loaded.context.cache.entries.values()) {
  if(value.items) assert.ok(value.items.length<=25,'cache stores only bounded result pages');
 }
 const volume=await loaded.extension.publicationContent({installmentId:bookID,workId:workID});
 assert.equal((volume.text.match(/Original sample paragraph/g)||[]).length,4000);assert.ok(volume.text.includes('paragraph 3999 &amp;'));assert.ok(volume.text.includes('<section id="chapter1">'));
});

test("LNORI streams nested sections exactly once and strips unsafe markup",async()=>{
 const nested=book.replace('<h2>Chapter One</h2>', '<section id="nested"><h2>Chapter One</h2><p>Original nested paragraph &copy; &lt;value&gt;</p></section><!-- Ignore > --><STYLE>unsafe-style</STYLE><SCRIPT>unsafe-script</SCRIPT>');
 const loaded=await loadContentExtension(main,r=>runtimeResponse({url:r.url,text:new URL(r.url).pathname.startsWith('/series/')?series:nested}),{cache:metadataCacheFixture()});
 const content=await loaded.extension.publicationContent({installmentId:bookID,workId:workID});
 assert.equal((content.text.match(/Original nested paragraph/g)||[]).length,1);
 assert.ok(content.text.includes('© &lt;value&gt;'));assert.ok(content.text.includes('<section id="nested">'));assert.ok(!content.text.includes('unsafe-'));
});


test("LNORI streaming descriptor does not fetch the full volume", async () => {
 const loaded = await loadContentExtension(main, () => { throw new Error("No HTTP should be needed"); });
 const stream = await loaded.extension.publicationStream({installmentId: bookID, workId: workID});
 assert.equal(stream.id, bookID); assert.equal(stream.workId, workID);
 assert.equal(stream.url, `https://lnori.com/book/${bookID}`);
 assert.equal(stream.headers.Referer, "https://lnori.com/"); assert.equal(loaded.calls.length, 0);
 await assert.rejects(loaded.extension.publicationStream({installmentId: "../secret", workId: workID}), {type:"invalidIdentifier"});
});


test("LNORI prefers displayed book artwork over promotional schema cards",async()=>{
 const html=series.replace('https://cdn.lnori.com/cover/1.webp','https://cdn.lnori.com/volume/1.jpg')+'<img src="https://cdn.lnori.com/cover/1.webp" alt="Sample Cover">';
 const loaded=await loadContentExtension(main,r=>runtimeResponse({url:r.url,text:html}),{cache:metadataCacheFixture()});
 const detail=await loaded.extension.details(workID);
 assert.equal(detail.workInfo.thumbnailUrl,'https://cdn.lnori.com/cover/1.webp');
});

test("LNORI homepage shelves preserve source order, paginate, and share one cached request", async () => {
 const loaded=await loadContentExtension(main,responder,{cache:metadataCacheFixture()});
 const sections=await loaded.extension.discoverSections();
 assert.deepEqual(sections.map(s=>s.id),['featured','seasonal','popular','library']);
 assert.deepEqual((await loaded.extension.discover({sectionId:'featured'})).items.map(v=>v.workId),['1/sample-novel','2/sample-2','3/sample-3','4/sample-4','5/sample-5']);
 assert.deepEqual((await loaded.extension.discover({sectionId:'seasonal'})).items.map(v=>v.workId),['3/sample-3','1/sample-novel']);
 const popular=await loaded.extension.discover({sectionId:'popular'});
 assert.equal(popular.items.length,25);assert.deepEqual(popular.metadata,{page:2});
 assert.equal((await loaded.extension.discover({sectionId:'popular',metadata:popular.metadata})).items.length,1);
 assert.equal(loaded.calls.length,1,'shelves must reuse the homepage response');
 await assert.rejects(loaded.extension.discover({sectionId:'unknown'}),{type:'invalidSection'});
});

test("LNORI missing homepage sections fail explicitly and failed pages are retryable", async () => {
 let failed=true;const loaded=await loadContentExtension(main,r=>failed?runtimeResponse({url:r.url,text:'<html>Unavailable</html>'}):responder(r),{cache:metadataCacheFixture()});
 await assert.rejects(loaded.extension.discover({sectionId:'featured'}),{type:'invalidResponse'});
 failed=false;assert.equal((await loaded.extension.discover({sectionId:'featured'})).items.length,5);
});


test("LNORI homepage streaming preserves nested cards, entities, and text fallback", async () => {
 const html=homepage.replace('data-title="Original Journey 1"','data-title="Original &amp; Journey > 1"')
  .replace('></div>','><div><span>Decorative content</span></div></div>')
  .replace('alt="Original Journey 3"','alt=""').replace('</a>','<h3>Third &amp; Journey</h3></a>');
 const loaded=await loadContentExtension(main,r=>runtimeResponse({url:r.url,text:html}),{cache:metadataCacheFixture()});
 const featured=await loaded.extension.discover({sectionId:'featured'});
 assert.equal(featured.items.length,5);assert.equal(featured.items[0].title,'Original & Journey > 1');
 const seasonal=await loaded.extension.discover({sectionId:'seasonal'});
 assert.equal(seasonal.items[0].title,'Third & Journey');assert.equal(loaded.calls.length,1);
});


test("LNORI cold searches cache only matching cards and reuse them after restart",async()=>{
 const cache=metadataCacheFixture();
 const html=Array.from({length:884},(_,i)=>card(i+1)).join('');
 const respond=r=>runtimeResponse({url:r.url,text:html});
 const loaded=await loadContentExtension(main,respond,{cache});
 const found=await loaded.extension.search({query:'sample 884'});
 assert.equal(found.items[0].workId,'884/sample-884');
 assert.equal(cache.entries.size,1);
 assert.equal([...cache.entries.values()][0].value.items.length,1);
 const restarted=await loadContentExtension(main,respond,{cache});
 assert.deepEqual(await restarted.extension.search({query:'sample 884'}),found);
 assert.equal(restarted.calls.length,0);
 assert.equal((await restarted.extension.search({query:'absent title'})).items.length,0);
 assert.equal((await restarted.extension.search({query:'absent title'})).items.length,0);
 assert.equal(restarted.calls.length,1);
});
