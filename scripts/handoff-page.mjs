// SPDX-License-Identifier: Apache-2.0
export const ratingLabels={SAFE:'Safe',MATURE:'Mature',ADULT:'NSFW'};
export function languageLabel(code) {
 if(code==='es-la')return 'Spanish (Latin America)';
 if(code==='multi')return 'Multilingual';
 try{return new Intl.DisplayNames(['en'],{type:'language'}).of(code)||code;}catch{return code;}
}
export function filterSources(sources,{query='',language='',rating=''}={}) {
 const term=query.trim().toLocaleLowerCase('en');
 return sources.filter(source=>(!term||`${source.name} ${source.id}`.toLocaleLowerCase('en').includes(term))&&(!language||(language==='multi'?(source.languages.length>1||source.languages.includes('multi')):source.languages.includes(language)))&&(!rating||source.contentRating===rating)).sort((a,b)=>a.name.localeCompare(b.name,'en'));
}
export function resolveAction(search, config) {
 const params=new URLSearchParams(search), sources=params.getAll('source');
 if([...params.keys()].some(key=>key!=='source')||sources.length>1||(sources.length&&!config.expectedIDs.includes(sources[0])))return {error:'This install link is invalid. Copy the repository URL to add it in Manko.'};
 if(sources.length&&['retired','serviceunavailable'].includes((config.sources.find(source=>source.id===sources[0])?.availability||'').toLowerCase()))return {error:'This extension is unavailable for installation.'};
 const query=new URLSearchParams({url:config.repositoryURL});
 if(sources.length)query.append('source',sources[0]);
 return {source:sources[0]||null,href:`manko://repository/${sources.length?'install':'add'}?${query}`};
}
export function mount(document,location,navigator,config) {
 const action=resolveAction(location.search,config),open=document.getElementById('open'),status=document.getElementById('status'),input=document.getElementById('repository');
 if(action.error){status.textContent=action.error;}
 else if(action.source){
  const row=document.getElementById(`source-${action.source}`);
  if(row){row.classList.add('selected');row.setAttribute('aria-label',`${action.source}, selected for installation`);}
  const selected=document.getElementById('selected');
  selected.hidden=false;
  const link=document.getElementById('install-selected');link.href=action.href;link.textContent=`Open ${action.source} in Manko`;
  if(location.hostname==='superdupercoolperson.github.io'){try{location.assign(action.href);}catch{/* Explicit link remains usable. */}}
 }
 open.href=resolveAction('',config).href;
 document.getElementById('copy').addEventListener('click',async()=>{
  try{await navigator.clipboard.writeText(config.repositoryURL);status.textContent='Repository URL copied.';}
  catch{input.focus();input.select();status.textContent='Select and copy this URL, then paste it into Manko.';}
 });
 const search=document.getElementById('search'),language=document.getElementById('language'),rating=document.getElementById('rating');
 function update(){
  const visible=new Set(filterSources(config.sources,{query:search.value,language:language.value,rating:rating.value}).map(source=>source.id));
  for(const source of config.sources)document.getElementById(`source-${source.id}`).hidden=!visible.has(source.id);
  document.getElementById('count').textContent=`${visible.size} of ${config.sources.length} extensions`;
  document.getElementById('empty').hidden=visible.size!==0;
 }
 for(const control of [search,language,rating])control.addEventListener('input',update);
 document.getElementById('reset').addEventListener('click',()=>{search.value='';language.value='';rating.value='';update();search.focus();});
 document.getElementById('filters').hidden=false;
 update();
}
