/* SPDX-License-Identifier: Apache-2.0 */
import test from "node:test";
import assert from "node:assert/strict";
import {resolve} from "node:path";
import {loadContentExtension,runtimeResponse,metadataCacheFixture,MINIMAL_WEBP_BYTES} from "../../test-runtime.mjs";
const main=resolve(import.meta.dirname,"../main.wasm");
test("Omega normalizes the live JPEG MIME alias while preserving native image handles", async () => {
 for (const mimeType of ["image/jpg", "IMAGE/JPG; charset=binary"]) {
  const loaded = await loadContentExtension(main, request => ({
   ...runtimeResponse({url: request.url, mimeType}), resourceID: "native-jpeg-fixture"
  }));
  const image = await loaded.extension.imagePageContent({url: "https://media.omegascans.org/page.jpg"});
  assert.equal(image.mimeType, "image/jpeg");
  assert.equal(image.resourceID, "native-jpeg-fixture");
  assert.equal(image.dataBase64, undefined);
 }
 const invalid = await loadContentExtension(main, request => runtimeResponse({url: request.url, mimeType: "text/html", text: "not an image"}));
 await assert.rejects(invalid.extension.imagePageContent({url: "https://media.omegascans.org/page.jpg"}), {type: "invalidResponse"});
});
const series={id:7,series_slug:"sample-comic",title:"Sample Comic",series_type:"Comic",thumbnail:"https://media.omegascans.org/file/zFSsXt/cover.webp",author:"Sample Creator",description:"<p>Sample &amp; <strong>formatted</strong> synopsis.</p><p>Second paragraph.</p>",status:"Ongoing",tags:"$b"};
const chapter=(slug,price=0)=>({chapter_slug:slug,chapter_name:slug.replace('chapter-','Chapter '),price,series:{id:7,series_slug:"sample-comic"},created_at:"2026-10-01T00:00:00.000Z"});
function flight(records){return records.map((r,i)=>`<script>self.__next_f.push(${JSON.stringify([1,`${i===0?'a':'b'}:${JSON.stringify(r)}\n`])})</script>`).join('');}
function responder(request){
 const url=new URL(request.url);let value;
 if(url.pathname==="/query"){const page=Number(url.searchParams.get('page'));value={meta:{current_page:page,last_page:2},data:page===1?[series,series]:[]};}
 else if(url.pathname==="/tags")value=[{id:1,name:"Adventure"}];
 else if(url.pathname==="/chapter/query"){assert.equal(url.searchParams.get("series_id"),"7");value={meta:{last_page:1},data:[chapter('chapter-2',10),chapter('chapter-1'),chapter('chapter-1')]};}
 else if(url.pathname==="/series/sample-comic")return runtimeResponse({url:request.url,text:flight([series,[{id:1,name:"Adventure"}]])});
 else if(url.pathname.startsWith("/series/sample-comic/chapter-"))return runtimeResponse({url:request.url,text:flight([{...chapter(url.pathname.split('/').at(-1),url.pathname.endsWith('chapter-2')?10:0),chapter_data:{images:["https://media.omegascans.org/file/zFSsXt/01.webp","https://media.omegascans.org/file/zFSsXt/02.webp"]}}])});
 else if(request.imageResource)return runtimeResponse({url:request.url,mimeType:"image/webp",bytes:MINIMAL_WEBP_BYTES});
 else throw new Error(`Unexpected ${request.url}`);
 return runtimeResponse({url:request.url,text:JSON.stringify(value)});
}
test("Omega maps catalog pagination, duplicates, structured genres and cache reuse",async()=>{
 const cache=metadataCacheFixture();const loaded=await loadContentExtension(main,responder,{cache});
 const first=await loaded.extension.search({query:"sample & comic",sort:"total_views",selections:[{fieldID:"genre",value:"1"}]});assert.equal(first.items.length,1);assert.equal(first.metadata.page,2);
 const url=new URL(loaded.calls[0].url);assert.equal(url.searchParams.get("query_string"),"sample & comic");assert.equal(url.searchParams.get("tags_ids"),"[1]");assert.equal(url.searchParams.get("orderBy"),"total_views");assert.equal(url.searchParams.get("series_type"),"Comic");
 const last=await loaded.extension.search({query:"sample & comic",metadata:first.metadata});assert.equal(last.items.length,0);assert.equal(last.metadata,null);
 const suggested=await loaded.extension.searchSuggestions({fieldID:"genre",query:"adv"});assert.equal(suggested[0].title,"Adventure");
 await loaded.extension.search({query:"sample & comic",sort:"total_views",selections:[{fieldID:"genre",value:"1"}]});assert.equal(loaded.calls.length,3);
 const restarted=await loadContentExtension(main,responder,{cache});await restarted.extension.search({query:"sample & comic",sort:"total_views",selections:[{fieldID:"genre",value:"1"}]});assert.equal(restarted.calls.length,0);
});
test("Omega decodes public Flight data, lists free chapters and brokers their images",async()=>{
 const loaded=await loadContentExtension(main,responder,{cache:metadataCacheFixture()});const detail=await loaded.extension.details("sample-comic");assert.equal(detail.workInfo.author,"Sample Creator");assert.equal(detail.workInfo.synopsis,"Sample & formatted synopsis. Second paragraph.");
 const chapters=await loaded.extension.installments(detail);assert.equal(chapters.length,1);assert.equal(chapters[0].installmentId,"sample-comic/chapter-1");assert.equal(chapters[0].number,1);
 const sequence=await loaded.extension.imagePages(chapters[0]);assert.equal(sequence.pages.length,2);
 await loaded.extension.imagePages(chapters[0]);assert.equal(loaded.calls.length,3);
 await loaded.extension.imagePageContent({url:sequence.pages[0]});assert.equal(loaded.calls.at(-1).imageResource,true);
 await assert.rejects(loaded.extension.imagePages({...chapters[0],installmentId:"sample-comic/chapter-2"}),{type:"authenticationRequired"});
 await assert.rejects(loaded.extension.imagePages({...chapters[0],installmentId:"another/chapter-1"}),{type:"invalidIdentifier"});
});
test("Omega rejects malformed payloads, unsafe media and unrecognized filters",async()=>{
 const loaded=await loadContentExtension(main,r=>runtimeResponse({url:r.url,text:'{}'}),{cache:metadataCacheFixture()});
 await assert.rejects(loaded.extension.search({query:"sample"}),{type:"invalidResponse"});
 await assert.rejects(loaded.extension.search({sort:"unrecognized"}),{type:"invalidSearchTerm"});
 await assert.rejects(loaded.extension.search({selections:[{fieldID:"genre",value:"1",polarity:"exclude"}]}),{type:"invalidSearchTerm"});
 await assert.rejects(loaded.extension.search({selections:[{fieldID:"genre",value:"../"}]}),{type:"invalidIdentifier"});
 await assert.rejects(loaded.extension.imagePageContent({url:"https://media.omegascans.org.evil.test/1.webp"}),{type:"hostNotAllowed"});
});

test("Omega reads decimal chapter numbers from names rather than hyphenated slugs",async()=>{
 const loaded=await loadContentExtension(main,r=>new URL(r.url).pathname==='/chapter/query'?runtimeResponse({url:r.url,text:JSON.stringify({meta:{last_page:1},data:[{...chapter('chapter-24-5'),chapter_name:'Chapter 24.5'}]})}):responder(r));
 const chapters=await loaded.extension.installments({workId:'sample-comic'});assert.equal(chapters[0].number,24.5);
});

test('Omega latest updates follow chapter updates, separately from new series',async()=>{
 const loaded=await loadContentExtension(main,responder,{cache:metadataCacheFixture()});
 for(const [section,sort] of [['latest','updated_at'],['new','created_at'],['popular','total_views']]){
  await loaded.extension.discover({sectionId:section});
  assert.equal(new URL(loaded.calls.at(-1).url).searchParams.get('orderBy'),sort);
 }
});

test('Omega skips unnumbered season markers without losing numbered free chapters',async()=>{
 const rows=[
  {...chapter('chapter-156'),index:156},
  {...chapter('season-2'),chapter_name:'Season 2',index:null},
  {...chapter('announcement'),chapter_name:'Announcement'},
  {...chapter('chapter-24-5'),chapter_name:'Chapter 24.5'},
  {...chapter('chapter-1'),index:'1'},
  chapter('chapter-2',10),
 ];
 const cache=metadataCacheFixture();
 const load=data=>loadContentExtension(main,r=>new URL(r.url).pathname==='/chapter/query'
  ?runtimeResponse({url:r.url,text:JSON.stringify({meta:{last_page:1},data})}):responder(r),{cache});
 const loaded=await load(rows);
 const chapters=await loaded.extension.installments({workId:'sample-comic'});
 assert.deepEqual(chapters.map(c=>c.number),[156,24.5,1]);
 assert.deepEqual(chapters.map(c=>c.installmentId),['sample-comic/chapter-156','sample-comic/chapter-24-5','sample-comic/chapter-1']);
 const restarted=await load(rows);
 assert.deepEqual(await restarted.extension.installments({workId:'sample-comic'}),chapters);
 assert.equal(restarted.calls.length,0);
 const sequence=await loaded.extension.imagePages(chapters[0]);
 assert.equal((await loaded.extension.imagePageContent({url:sequence.pages[0]})).mimeType,'image/webp');
});

test('Omega still rejects malformed chapter data instead of treating it as a season marker',async()=>{
 const marker={...chapter('season-2'),chapter_name:'Season 2',index:null};
 for(const row of [
  {...marker,series:{id:8,series_slug:'foreign'}},
  {...marker,price:null},
  {...marker,index:-1},
  {...marker,index:'invalid'},
  {...marker,chapter_name:null},
 ]){
  const loaded=await loadContentExtension(main,r=>new URL(r.url).pathname==='/chapter/query'
   ?runtimeResponse({url:r.url,text:JSON.stringify({meta:{last_page:1},data:[chapter('chapter-1'),row]})}):responder(r));
  await assert.rejects(loaded.extension.installments({workId:'sample-comic'}),{type:'invalidResponse'});
 }
});
