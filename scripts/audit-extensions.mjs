// SPDX-License-Identifier: Apache-2.0
import {readFile,writeFile,mkdir,copyFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import {performance} from 'node:perf_hooks';
import {ids,responder,connection} from './audit-fixtures.mjs';
import {loadContentExtension,loadTrackerExtension,metadataCacheFixture,runtimeResponse} from '../extensions/content/test-runtime.mjs';
const root=resolve(import.meta.dirname,'..'),live=process.argv.includes('--live');
const selected=process.argv.find(v=>v.startsWith('--source='))?.split('=')[1];
const output=resolve(root,'target/extension-audit',live?'live':'fixtures');await mkdir(output,{recursive:true});
const results=[];
for(const id of ids.filter(id=>!selected||selected.split(',').includes(id))) {
 const kind=['AniList','MyAnimeList'].includes(id)?'tracker':'content';
 const dir=resolve(root,'extensions',kind,id),manifest=JSON.parse(await readFile(resolve(dir,'extension.json')));
 if(live&&(kind==='tracker'||manifest.connectorKind)){results.push({id,status:'blocked',reason:'No test account/server supplied; fixture checks are separate.'});continue;}
 const downloads=[],exchanges=[],steps=[],fixture=responder(id);
 const respond=async request=>{
  let response;
  if(live){const res=await fetch(request.url,{method:request.method,headers:request.headers,body:request.bodyBase64?Buffer.from(request.bodyBase64,'base64'):undefined,signal:AbortSignal.timeout(20000)});response=runtimeResponse({url:res.url,status:res.status,headers:Object.fromEntries(res.headers),mimeType:res.headers.get('content-type'),bytes:Buffer.from(await res.arrayBuffer())});}
  else response=fixture(request);
  exchanges.push({method:request.method,url:request.url,bodyBase64:request.bodyBase64||null,range:request.headers.Range||null,response,asset:request.imageResource?'image':null});return response;
 };
 let loaded;
 const step=async(name,operation)=>{const start=performance.now(),before=exchanges.length;try{const value=await operation();steps.push({name,status:'pass',milliseconds:Math.round(performance.now()-start),requests:exchanges.length-before});return value;}catch(e){steps.push({name,status:'fail',milliseconds:Math.round(performance.now()-start),error:e.message,type:e.type});return null;}};
 loaded=await (kind==='tracker'?loadTrackerExtension:loadContentExtension)(resolve(dir,'main.wasm'),respond,{cache:metadataCacheFixture(),secureState:kind==='tracker'?{oauth_access_token:'audit-fixture-token'}:{}});
 const ext=loaded.extension;
 if(kind==='tracker'){
  const matches=await step('search',()=>ext.search({title:'The Quiet Observatory',kind:'manga'}));
  for(const match of matches||[]) if(match.coverURL||match.coverUrl) await step('cover',()=>respond({method:'GET',url:match.coverURL||match.coverUrl,headers:{},imageResource:true}));
  const work=matches?.[0]?.id;
  if(work){await step('progress',()=>ext.progress(work));await step('update (fixture only)',()=>ext.update({remoteWorkID:work,chapter:5,status:'reading'}));}
  await step('collections',()=>ext.collections());
 } else if(manifest.connectorKind){
  const context=connection(id),call=(operation,args={})=>ext.connector({...context,operation,...args});
  await step('capabilities',()=>call('capabilities'));
  const page=await step('catalog',()=>call(id==='Suwayomi'?'search':'browse',{query:{text:'',page:1,pageSize:40,filters:{}}}));
  for(const card of page?.items||[])if(card.coverURL)await step('cover',()=>respond({method:'GET',url:card.coverURL,headers:{},imageResource:true}));
  const work=page?.items?.[0];
  if(work){await step('details',()=>call('details',{remoteID:work.remoteID}));const chapters=await step('installments',()=>call('installments',{remoteID:work.remoteID,page:1}));const chapter=chapters?.items?.[0];if(chapter){const acquisition=await step('acquisition',()=>call('acquisition',{remoteID:chapter.remoteID}));if(acquisition)downloads.push(acquisition);if(id==='Suwayomi'){const pages=await step('reader pages',()=>call('imagePages',{remoteID:chapter.remoteID,pageCount:chapter.pageCount,query:{page:1,pageSize:25}}));for(const page of pages?.items||[])await step('reader image',()=>respond({method:'GET',url:page.resourceURL,headers:{},imageResource:true}));}}}
 } else {
  const sections=await step('sections',()=>ext.discoverSections());let primary;
  for(const section of sections||[]){const page=await step(`catalog:${section.id}`,()=>ext.discover({sectionId:section.id}));primary??=page?.items?.find(v=>v.workId);if(page?.metadata&&!live)await step(`pagination:${section.id}`,()=>ext.discover({sectionId:section.id,metadata:page.metadata}));for(const card of (page?.items||[]).slice(0,live?5:undefined)) {
    if(card.previewImageUrl)await step(`preview:${section.id}:${card.workId}`,()=>ext.imagePageContent({url:card.previewImageUrl}));
    if(card.imageUrl)await step(`cover:${section.id}:${card.workId}`,()=>ext.imagePageContent({url:card.imageUrl}));
   }}
  if(primary){const detail=await step('details',()=>ext.details(primary.workId));if(detail){const chapters=await step('installments',()=>ext.installments(detail));for(const chapter of (live?chapters?.slice(0,1):chapters)||[]){if(id==='LNori')await step('reader text',()=>ext.publicationContent(chapter));else{const pages=await step('reader pages',()=>ext.imagePages({...chapter,sourceWork:detail}));for(const url of (pages?.pages||[]).slice(0,live?2:undefined))await step('reader image',()=>ext.imagePageContent({url}));}}}}
  if(!live){
   await step('warm catalog',()=>ext.discover({sectionId:sections[0].id}));
   const configuration=await step('search configuration',()=>ext.searchFilters());
   for(const sort of configuration?.sortOptions||[]) {
    const page=await step(`sort:${sort.id}`,()=>ext.search({query:'',sort:sort.id}));
    if(page?.metadata)await step(`sort pagination:${sort.id}`,()=>ext.search({query:'',sort:sort.id,metadata:page.metadata}));
   }
   await step('search text',()=>ext.search({query:id==='HitomiLA'?'1':'sample'}));
   if(id==='MangaKatana') {
    for(const query of ['sample','Sample','missing','Missing']) {
     const page=await step(`native search:${query}`,()=>ext.search({query}));
     if(page?.metadata)await step(`native search pagination:${query}`,()=>ext.search({query,metadata:page.metadata}));
    }
   }
   const values={genre:id==='OmegaScans'?'1':['MangaFox','MangaKatana'].includes(id)?'action':id==='MangaFire'?'1':id.startsWith('Manga')?'2':'fantasy',author:'sample',artist:'sample',tag:'landscape',female:'landscape',male:'landscape',group:'sample',series:'sample',character:'sample',type:'manga',parody:'sample',category:'manga',pages:'3',favorites:'12',uploaded:'>2020-01-01',title:'sample',jtitle:'sample'};
   for(const field of configuration?.fields||[]) {
    const value=field.options?.[0]?.id||values[field.id];
    const selection={fieldID:field.id,value,polarity:'include'};
    if(field.inputKind==='lookup'&&['LNori','MangaFox','MangaKatana','MangaFire','OmegaScans'].includes(id)) {
     await step(`suggestions:${field.id}`,()=>ext.searchSuggestions({fieldID:field.id,query:'',limit:20}));
     await step(`suggestions filtered:${field.id}`,()=>ext.searchSuggestions({fieldID:field.id,query:'fan',limit:20}));
    }
    const filtered=await step(`filter:${field.id}`,()=>ext.search({selections:[selection]}));
    if(id==='MangaKatana'&&filtered?.metadata)await step(`filter pagination:${field.id}`,()=>ext.search({selections:[selection],metadata:filtered.metadata}));
    if(field.supportsExclusion)await step(`exclude:${field.id}`,()=>ext.search({selections:[{...selection,polarity:'exclude'}]}));
   }
  }
 }
 const completed = kind === 'tracker' ? ['search', 'progress', 'collections'] : manifest.connectorKind ? ['catalog', 'details', 'installments', 'acquisition'] : ['sections', 'details', 'installments', id === 'LNori' ? 'reader text' : 'reader pages'];
 const missing = completed.filter(name => !steps.some(step => step.name === name && step.status === 'pass'));
 const report={id,status:steps.some(v=>v.status==='fail')?'fail':missing.length?'blocked':'pass',...(missing.length?{reason:`Incomplete flow: ${missing.join(', ')}`} : {}),mode:live?'live HTTP + Node WASM (not app visual verification)':'fixture Node WASM',steps};results.push(report);
 if(!live){await mkdir(resolve(output,id),{recursive:true});if(manifest.connectorKind)await copyFile(resolve(root,'scripts/fixtures/observatory.epub'),resolve(output,id,'publication.epub'));await writeFile(resolve(output,id,'fixture.json'),JSON.stringify({schemaVersion:1,downloads,capturedOn:'2026-10-13',manifest,connection:manifest.connectorKind?connection(id):null,exchanges},null,2));await copyFile(resolve(root,'target/wasm32-unknown-unknown/release',manifest.crate.replaceAll('-','_')+'.wasm'),resolve(output,id,'main.wasm'));}
 await writeFile(resolve(output,'results.json'),JSON.stringify(results,null,2));console.log(id,report.status,steps.filter(v=>v.status==='fail').map(v=>`${v.name}: ${v.error}`).join('; '));
}
if(!live&&results.some(v=>v.status!=='pass'))process.exitCode=1;
