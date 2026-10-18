import test from "node:test";
import assert from "node:assert/strict";
import {resolve} from "node:path";
import {loadContentExtension,runtimeResponse,metadataCacheFixture,MINIMAL_WEBP_BYTES} from "../../test-runtime.mjs";
const main=resolve(import.meta.dirname,"../main.wasm");
import { row, catalog, series, reader, responder } from "./fixtures.mjs";
test("MangaFox preserves catalog identities, pagination, entities and cache reuse",async()=>{
 const cache=metadataCacheFixture();const loaded=await loadContentExtension(main,responder,{cache});
 const first=await loaded.extension.discover({sectionId:'directory'});assert.equal(first.items.length,1);assert.equal(first.items[0].workId,'sample_manga');assert.equal(first.items[0].title,'Sample & Story');assert.equal(first.metadata.page,2);
 const last=await loaded.extension.discover({sectionId:'directory',metadata:first.metadata});assert.equal(last.items.length,0);assert.equal(last.metadata,null);
 const searched=await loaded.extension.search({query:'sample & story'});assert.equal(searched.items.length,1);assert.equal(new URL(loaded.calls.at(-1).url).searchParams.get('title'),'sample & story');
 const restarted=await loadContentExtension(main,responder,{cache});await restarted.extension.discover({sectionId:'directory'});assert.equal(restarted.calls.length,0);
});
test("MangaFox opens chapters with one HTML request and resolves images lazily",async()=>{
 const loaded=await loadContentExtension(main,responder,{cache:metadataCacheFixture()});const detail=await loaded.extension.details('sample_manga');assert.equal(detail.workInfo.synopsis,'Full synopsis');
 const chapters=await loaded.extension.installments(detail);assert.equal(chapters.length,2);assert.equal(chapters[0].volume,1);assert.equal(chapters[1].number,2.5);
 const list=await loaded.extension.imagePages(chapters[0]);assert.equal(list.pages.length,3);assert.equal(loaded.calls.length,2);
 await loaded.extension.imagePageContent({url:list.pages[0]});assert.equal(loaded.calls.length,3,'first page reuses the chapter HTML');assert.equal(loaded.calls.at(-1).headers.Referer,list.pages[0]);
 await loaded.extension.imagePageContent({url:list.pages[2]});assert.equal(loaded.calls.length,5,'only the requested page is resolved');assert.equal(loaded.calls.at(-1).imageResource,true);assert.equal(new URL(loaded.calls.at(-1).url).pathname,'/store/3.webp');
 assert.ok(loaded.calls.every(r=>!r.url.includes('evil.test')));
 await assert.rejects(loaded.extension.imagePages({...chapters[0],workId:'another'}),{type:'invalidIdentifier'});
});
test("MangaFox rejects unsafe page paths and malformed reader markup",async()=>{
 const loaded=await loadContentExtension(main,r=>runtimeResponse({url:r.url,text:'<div id="viewer"><img id="image" src="https://evil.test/page.jpg"></div>'}),{cache:metadataCacheFixture()});
 await assert.rejects(loaded.extension.imagePages({installmentId:'sample_manga/v01/c001',workId:'sample_manga'}),{type:'hostNotAllowed'});
 await assert.rejects(loaded.extension.imagePageContent({url:'https://m.fanfox.net/manga/sample_manga/v01/c001/0.html'}),{type:'invalidResponse'});
 await assert.rejects(loaded.extension.imagePageContent({url:'https://m.fanfox.net/manga/sample_manga/v01/c001/1.html?other=1'}),{type:'invalidResponse'});
 await assert.rejects(loaded.extension.imagePageContent({url:'https://evil.test/page.jpg'}),{type:'hostNotAllowed'});
});
test("MangaFox long chapter indexes retain order, metadata and decimal identities",async()=>{
 const largeSeries=series.slice(0,series.indexOf('<div id="chapterlist">'))+`<div id="chapterlist">${Array.from({length:1230},(_,i)=>`<a href="/manga/sample_manga/c${1230-i}/1.html" title="Chapter ${1230-i} &amp; title"><div><p>Chapter ${1230-i}</p><p>2026</p></div></a>`).join('')}</div>`;
 const loaded=await loadContentExtension(main,r=>runtimeResponse({url:r.url,text:largeSeries}),{cache:metadataCacheFixture()});
 const chapters=await loaded.extension.installments({workId:'sample_manga'});assert.equal(chapters.length,1230);assert.equal(chapters[0].number,1230);assert.equal(chapters.at(-1).number,1);assert.equal(chapters[0].title,'Chapter 1230 & title');
 await loaded.extension.installments({workId:'sample_manga'});assert.equal(loaded.calls.length,1);
});

test("MangaFox mirrors every homepage shelf without duplicate requests or duplicate cards",async()=>{
 const loaded=await loadContentExtension(main,responder,{cache:metadataCacheFixture()});
 const sections=await loaded.extension.discoverSections();
 assert.deepEqual(sections.map(s=>s.id),['hot','reading','ranking','recommended','new','trending','directory','latest']);
 for(const section of sections.filter(s=>s.id!=='directory')) {
  const page=await loaded.extension.discover({sectionId:section.id});
  assert.deepEqual(page.items.map(v=>v.workId),['sample_manga']);assert.equal(page.metadata,null);
 }
 assert.equal(loaded.calls.length,1);
 await assert.rejects(loaded.extension.discover({sectionId:'unknown'}),{type:'invalidSection'});
});

test("MangaFox genres use the public directory and reject unsupported combinations",async()=>{
 const loaded=await loadContentExtension(main,responder,{cache:metadataCacheFixture()});
 await loaded.extension.search({selections:[{fieldID:'genre',value:'action'}]});
 assert.equal(new URL(loaded.calls[0].url).pathname,'/directory/action/');
 await assert.rejects(loaded.extension.search({query:'sample',selections:[{fieldID:'genre',value:'action'}]}),{type:'invalidSearchTerm'});
 const suggestions=await loaded.extension.searchSuggestions({fieldID:'genre',query:'fan'});
 assert.deepEqual(suggestions.map(v=>v.value),['fantasy']);
});
