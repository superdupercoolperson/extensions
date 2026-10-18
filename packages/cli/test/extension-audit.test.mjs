// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {readFile} from 'node:fs/promises';
import {resolve} from 'node:path';
test('all release WASM packages complete the shared offline audit', async()=>{
 const root=resolve(import.meta.dirname,'../../..');
 await promisify(execFile)(process.execPath,['scripts/audit-extensions.mjs'],{cwd:root});
 const results=JSON.parse(await readFile(resolve(root,'target/extension-audit/fixtures/results.json')));
 assert.equal(results.length,ids.length);
 assert.ok(results.every(result=>result.status==='pass'));
 for(const result of results){
  assert.ok(result.steps.some(step=>step.name==='details'||step.name==='progress'));
  assert.ok(result.steps.some(step=>step.name.startsWith('reader')||step.name==='acquisition'||step.name==='collections'));
 }
 for(const id of ['LNori','MangaFox']) {
  const steps=results.find(result=>result.id===id).steps;
  assert.equal(steps.find(step=>step.name==='warm catalog').requests,0,`${id} reuses its homepage`);
 }
 for(const id of ['MangaBat']) {
  const steps=results.find(result=>result.id===id).steps;
  assert.equal(steps.find(step=>step.name==='details').requests,1,`${id} fetches one series document`);
  assert.equal(steps.find(step=>step.name==='installments').requests,0,`${id} reuses the series document`);
 }
});

import {ids,responder,connection} from '../../../scripts/audit-fixtures.mjs';
import {loadContentExtension,loadTrackerExtension,runtimeResponse,metadataCacheFixture} from '../../../extensions/content/test-runtime.mjs';
for(const id of ids) test(`${id}: outage is explicit and retry cannot reuse a poisoned cache`,async()=>{
 const tracker=['AniList','MyAnimeList'].includes(id),connector=['OPDS','Komga','Kavita','Suwayomi'].includes(id);
 let unavailable=true;
 const fixture=responder(id);
 const {extension}=await (tracker?loadTrackerExtension:loadContentExtension)(resolve(import.meta.dirname,`../../../extensions/${tracker?'tracker':'content'}/${id}/main.wasm`),
  request=>unavailable?runtimeResponse({url:request.url,status:503,text:'Unavailable'}):fixture(request),
  {cache:metadataCacheFixture(),secureState:tracker?{oauth_access_token:'fixture-token'}:{}});
 const sections=tracker||connector?null:await extension.discoverSections();
 const operation=()=>tracker?extension.search({title:'sample',kind:'manga'}):connector?extension.connector({...connection(id),operation:'browse',query:{text:'',page:1,pageSize:40,filters:{}}}):extension.discover({sectionId:sections[0].id});
 await assert.rejects(operation,{type:'serviceError'});
 unavailable=false;
 const recovered=await operation();
 assert.ok((tracker?recovered:recovered.items).length>0);
});
