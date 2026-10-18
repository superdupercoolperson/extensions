// SPDX-License-Identifier: Apache-2.0
import test from 'node:test';
import assert from 'node:assert/strict';
import {resolveAction,mount,filterSources,ratingLabels,languageLabel} from '../../../scripts/handoff-page.mjs';
const config={expectedIDs:['One','Two'],repositoryURL:'https://superdupercoolperson.github.io/extensions/dist/community/v1/stable/',sources:[{id:'One',name:'One',languages:['multi'],contentRating:'SAFE'},{id:'Two',name:'Two',languages:['en'],contentRating:'MATURE'}]};
test('handoff selects only known packages and fixes the repository destination',()=>{
 for(const query of ['', '?source=One','?source=Two']) {
  const action=resolveAction(query,config),url=new URL(action.href);
  assert.equal(url.searchParams.get('url'),config.repositoryURL);
  assert.equal(url.pathname,query?'/install':'/add');
 }
 for(const query of ['?source=','?source=Unknown','?source=One&source=Two','?url=https://evil.example','?source=One&redirect=https://evil.example','?source=%3Cscript%3E']) assert.ok(resolveAction(query,config).error);
});
function fixture(){
 const elements=new Map();
 const document={getElementById(id){if(!elements.has(id))elements.set(id,{textContent:'Manko Catalog',value:'',classList:{add(){}},handlers:{},addEventListener(name,fn){this.handlers[name]=fn},removeAttribute(name){delete this[name]},setAttribute(name,value){this[name]=value},focus(){this.focused=true},select(){this.selected=true}});return elements.get(id)}};
 return {document,elements};
}
test('clipboard success and denied access both provide usable feedback',async()=>{
 for(const permitted of [true,false]) {
  const {document}=fixture();let copied;
  mount(document,{search:'',hostname:'localhost'},{clipboard:{async writeText(value){if(!permitted)throw new Error('denied');copied=value}}},config);
  await document.getElementById('copy').handlers.click();
  if(permitted){assert.equal(copied,config.repositoryURL);assert.match(document.getElementById('status').textContent,/copied/)}
  else {assert.equal(document.getElementById('repository').selected,true);assert.match(document.getElementById('status').textContent,/Select and copy/)}
 }
});
test('invalid selection cannot launch, while valid production links attempt the handoff',()=>{
 const invalid=fixture();let launched;
 mount(invalid.document,{search:'?source=Unknown',hostname:'superdupercoolperson.github.io',assign(url){launched=url}},{},config);
 assert.equal(launched,undefined);assert.match(invalid.document.getElementById('status').textContent,/invalid/);
 const valid=fixture();mount(valid.document,{search:'?source=One',hostname:'superdupercoolperson.github.io',assign(url){launched=url}},{},config);
 assert.match(launched,/source=One/);assert.equal(valid.document.getElementById('title').textContent,'Manko Catalog');
});

test('a browser rejecting automatic app launch retains the explicit link',()=>{
 const {document}=fixture();
 mount(document,{search:'?source=One',hostname:'superdupercoolperson.github.io',assign(){throw new Error('gesture required')}},{},config);
 assert.match(document.getElementById('install-selected').href,/source=One/);
 assert.equal(document.getElementById('open')['aria-disabled'],undefined);
});

test('browsing does not launch the app and combined filters reset',()=>{
 const {document}=fixture();let launches=0;
 mount(document,{search:'',hostname:'superdupercoolperson.github.io',assign(){launches++}},{},config);
 assert.equal(launches,0);
 document.getElementById('search').value='two';document.getElementById('language').value='multi';
 document.getElementById('search').handlers.input();
 assert.equal(document.getElementById('empty').hidden,false);
 document.getElementById('reset').handlers.click();
 assert.equal(document.getElementById('empty').hidden,true);
 assert.equal(document.getElementById('count').textContent,'2 of 2 extensions');
});
test('language and rating metadata stay faithful and filter together',()=>{
 assert.deepEqual(ratingLabels,{SAFE:'Safe',MATURE:'Mature',ADULT:'NSFW'});
 assert.equal(languageLabel('multi'),'Multilingual');
 const sources=[...config.sources,{id:'Three',name:'Alpha',languages:['en','ja'],contentRating:'ADULT'}];
 assert.deepEqual(filterSources(sources,{query:'three',language:'ja',rating:'ADULT'}).map(x=>x.id),['Three']);
 assert.deepEqual(filterSources(sources,{language:'multi'}).map(x=>x.id),['Three','One']);
 assert.equal(languageLabel('es-la'),'Spanish (Latin America)');
 assert.deepEqual(filterSources(sources,{rating:'SAFE'}).map(x=>x.id),['One']);
 assert.deepEqual(filterSources(sources).map(x=>x.id),['Three','One','Two']);
});

test('unavailable sources cannot launch through selected links',()=>{
 const unavailable={...config,sources:config.sources.map(source=>({...source,availability:'serviceUnavailable'}))};
 assert.ok(resolveAction('?source=One',unavailable).error);
});
