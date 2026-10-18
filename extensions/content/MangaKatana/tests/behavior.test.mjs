// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {resolve} from 'node:path';
import {loadContentExtension,runtimeResponse,metadataCacheFixture} from '../../test-runtime.mjs';
import {id,responder,reader,series,chapterRow} from './fixtures.mjs';
const main=resolve(import.meta.dirname,'../main.wasm');
const load=(r=responder,cache=metadataCacheFixture())=>loadContentExtension(main,r,{cache});
const select=(fieldID,value,polarity='include')=>({fieldID,value,polarity});
const html=(r,text,status=200,headers={})=>runtimeResponse({url:r.url,text,status,headers});
test('MangaKatana discovery, pagination, identity and cache reuse',async()=>{
 const cache=metadataCacheFixture(),a=await load(responder,cache),e=a.extension;
 assert.deepEqual((await e.discoverSections()).map(s=>s.id),['hot','latest','new','directory']);
 for(const sectionId of ['hot','latest','new','directory']){
  const p=await e.discover({sectionId});assert.equal(p.items.length,1);assert.equal(p.items[0].workId,id);assert.equal(p.items[0].title,'The Quiet Observatory & Beyond');
  assert.equal(p.metadata?.page,sectionId==='hot'?undefined:2);
  if(p.metadata){const end=await e.discover({sectionId,metadata:p.metadata});assert.deepEqual(end.items,[]);assert.equal(end.metadata,null);}
 }
 const restarted=await load(responder,cache);await restarted.extension.discover({sectionId:'hot'});assert.equal(restarted.calls.length,0);
 await assert.rejects(e.discover({sectionId:'unknown'}),{type:'invalidSection'});
});
test('MangaKatana title/author queries and all supported filters encode site parameters',async()=>{
 const {extension:e,calls}=await load();
 await e.search({query:'sample & 星'});assert.equal(new URL(calls.at(-1).url).searchParams.get('search'),'sample & 星');
 await e.search({selections:[select('author','Jane & Doe')]});assert.equal(new URL(calls.at(-1).url).searchParams.get('search_by'),'author');
 assert.deepEqual((await e.search({query:'missing'})).items,[]);
 const config=await e.searchFilters();assert.equal(config.sortedBrowseSectionID,'directory');
 for(const sort of config.sortOptions)await e.search({sort:sort.id});
 for(const field of config.fields.filter(f=>f.inputKind==='choice'))for(const o of field.options)await e.search({selections:[select(field.id,o.id)]});
 await e.search({sort:'numc',selections:[select('genre','action'),select('genre','fantasy'),select('genre','horror','exclude'),select('include_mode','or'),select('status','2'),select('chapters','50')]});
 const q=new URL(calls.at(-1).url).searchParams;assert.equal(q.get('include'),'action_fantasy');assert.equal(q.get('exclude'),'horror');assert.equal(q.get('include_mode'),'or');assert.equal(q.get('status'),'2');assert.equal(q.get('chapters'),'50');assert.equal(q.get('order'),'numc');
 assert.ok((await e.searchSuggestions({fieldID:'genre',query:'fant'})).some(x=>x.value==='fantasy'));
 for(const args of [{query:'x',selections:[select('genre','action')]},{query:'x',sort:'latest'},{query:'x',selections:[select('author','Jane')]},{selections:[select('genre','made-up')]},{selections:[select('status','9')]},{selections:[select('genre','action'),select('genre','action','exclude')]},{selections:[select('status','1','exclude')]}])await assert.rejects(e.search(args),{type:'invalidSearchTerm'});
});
test('MangaKatana details and chapters preserve metadata with a single series request',async()=>{
 const {extension:e,calls}=await load();const d=await e.details(id),cs=await e.installments(d);
 assert.deepEqual(d.workInfo.secondaryTitles,['Observatory','星']);assert.equal(d.workInfo.author,'Manko Fixtures, Another Artist');assert.equal(d.workInfo.status,'ongoing');assert.equal(d.workInfo.searchFacets[0].value,'fantasy');assert.ok(!('chapters'in d));
 assert.deepEqual(cs.map(c=>c.number),[2.5,1]);assert.equal(cs[0].title,'Chapter 2.5: First & Last Light');assert.equal(cs[0].publishDate,'2026-10-17T00:00:00Z');assert.equal(calls.length,1);
});
test('MangaKatana long indexes remain complete and ordered',async()=>{
 const body=series.slice(0,series.indexOf('<div class="chapters">'))+'<div class="chapters"><table>'+Array.from({length:1500},(_,i)=>chapterRow(1500-i).replace('href="/manga/','href="https://mangakatana.com/manga/')).join('')+'</table></div>';
 const {extension:e}=await load(r=>html(r,body));const cs=await e.installments({workId:id});assert.equal(cs.length,1500);assert.equal(cs[0].number,1500);assert.equal(cs.at(-1).number,1);
});
test('MangaKatana lightweight chapter parsing preserves markup and validates links',async()=>{
 const markup=series.replace('Chapter 1: First &amp; Last Light','Chapter <b>1</b>: First &amp; Last Light').replace('class="chapter"','class="chapter current"');
 const {extension:e}=await load(r=>html(r,markup));
 const chapters=await e.installments({workId:id});
 assert.equal(chapters[1].title,'Chapter 1 : First & Last Light');
 assert.equal(chapters[1].publishDate,'2026-10-17T00:00:00Z');
 for(const href of ['https://evil.test/manga/'+id+'/c1','/manga/'+id+'/c1?unsafe=1','/manga/'+id+'/c1#fragment']){
  const bad=await load(r=>html(r,series.replaceAll('/manga/'+id+'/c1',href)));
  await assert.rejects(bad.extension.installments({workId:id}));
 }
});
test('MangaKatana stable page references resolve only requested images using chapter Referer',async()=>{
 const {extension:e,calls}=await load();const ps=await e.imagePages({installmentId:`${id}/c1`,workId:id});assert.equal(calls.length,1);assert.equal(ps.pages.length,2);assert.ok(ps.pages[0].endsWith('#page-1'));
 await e.imagePageContent({url:ps.pages[1]});assert.equal(calls.length,2);assert.ok(calls[1].url.endsWith('/2.png'));assert.equal(calls[1].headers.Referer,`https://mangakatana.com/manga/${id}/c1`);
 await assert.rejects(e.imagePages({installmentId:`${id}/c1`,workId:'other.2'}),{type:'invalidIdentifier'});
});
test('MangaKatana refreshes expired images once and never retries cooldowns',async()=>{
 let reads=0,images=0;const {extension:e}=await load(r=>{if(r.imageResource){images++;return r.url.includes('/old/')?html(r,'expired',403):responder(r);}reads++;return html(r,reader(reads===1?'old':'new'));});
 const p=await e.imagePages({installmentId:`${id}/c1`});await e.imagePageContent({url:p.pages[0]});assert.equal(reads,2);assert.equal(images,2);
 let n=0;const bad=await load(r=>r.imageResource?(n++,html(r,'gone',404)):html(r,reader()));await assert.rejects(bad.extension.imagePageContent({url:p.pages[0]}),{type:'notFound'});assert.equal(n,2);
 const busy=await load(r=>r.imageResource?html(r,'busy',429,{'Retry-After':'10'}):html(r,reader()));await assert.rejects(busy.extension.imagePageContent({url:p.pages[0]}),{type:'rateLimited'});assert.equal(busy.calls.length,2);
});
test('MangaKatana parses string escapes and rejects executable or malformed arrays',async()=>{
 const escaped=await load(r=>html(r,String.raw`<script>var thzq=["https:\/\/i1.mangakatana.com/token/\u0061/1.png",];</script>`));assert.equal((await escaped.extension.imagePages({installmentId:`${id}/c1`})).pages.length,1);
 for(const code of ['[]',"['https://evil.test/a.png']","[fetch('https://evil.test')]","['https://i1.mangakatana.com/a.png'","['https://i1.mangakatana.com/a.png' + 'x']"]){const {extension:e}=await load(r=>html(r,`var thzq=${code};`));await assert.rejects(e.imagePages({installmentId:`${id}/c1`}));}
});
test('MangaKatana rejects unsafe references and distinguishes malformed pages, errors and challenges',async()=>{
 const a=await load();for(const url of ['https://evil.test/page.png',`https://mangakatana.com/manga/${id}/c1#page-0`,`https://mangakatana.com/manga/${id}/c1?evil=1#page-1`,'https://i1.mangakatana.com/arbitrary.png'])await assert.rejects(a.extension.imagePageContent({url}));assert.equal(a.calls.length,0);
 for(const [status,type]of [[404,'notFound'],[503,'serviceError'],[429,'rateLimited']]){const {extension:e}=await load(r=>html(r,'unavailable',status));await assert.rejects(e.discover({sectionId:'latest'}),{type});}
 const malformed=await load(r=>html(r,'<html>layout changed</html>'));await assert.rejects(malformed.extension.search({query:'x'}),{type:'invalidResponse'});
 const challenge=await load(r=>html(r,'<title>Just a moment...</title>',403));await assert.rejects(challenge.extension.discover({sectionId:'hot'}),{type:'challengeRequired'});assert.equal(challenge.challenges.length,1);
});
test('MangaKatana identifies CDN octet-stream images by bytes and rejects disguised HTML',async()=>{
 const good=await load(r=>r.imageResource?{...responder(r),mimeType:'application/octet-stream'}:responder(r));
 const url=`https://mangakatana.com/manga/${id}/c1#page-1`;
 assert.equal((await good.extension.imagePageContent({url})).mimeType,'image/png');
 const bad=await load(r=>r.imageResource?runtimeResponse({url:r.url,mimeType:'application/octet-stream',text:'<html>not an image</html>'}):responder(r));
 await assert.rejects(bad.extension.imagePageContent({url}),{type:'invalidResponse'});
});
