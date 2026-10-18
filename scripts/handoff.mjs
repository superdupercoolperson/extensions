// SPDX-License-Identifier: Apache-2.0
import {readFile,writeFile,copyFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
import {ratingLabels,languageLabel,filterSources} from './handoff-page.mjs';
const root=resolve(import.meta.dirname,'..');
const config=JSON.parse(await readFile(resolve(root,'repository.config.json'),'utf8'));
const pack=config.packs.community;
const expectedIDs=['HitomiLA','LNori','MangaBat','MangaFire','MangaFox','MangaKatana','NHentai','OmegaScans'];
const catalog=JSON.parse(await readFile(resolve(root,'dist/community/v1/stable/catalog.json'),'utf8'));
const sources=filterSources(catalog.sources);
if(sources.length!==expectedIDs.length||sources.some(source=>!expectedIDs.includes(source.id)))throw new Error('Unexpected catalog membership');
const esc=value=>String(value).replace(/[&<>"']/g,char=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
await mkdir(resolve(root,'site'),{recursive:true});
await copyFile(resolve(root,'scripts/handoff-page.mjs'),resolve(root,'site/handoff-page.mjs'));
await writeFile(resolve(root,'site/handoff-config.json'),JSON.stringify({expectedIDs:expectedIDs,repositoryURL:pack.repositoryURL,sources:sources.map(({id,name,languages,contentRating,availability})=>({id,name,languages,contentRating,availability}))})+'\n');
await writeFile(resolve(root,'site/boot.mjs'),`import {mount} from './handoff-page.mjs';
try {
 const response=await fetch('./handoff-config.json');
 if(!response.ok)throw new Error('Configuration unavailable');
 mount(document,location,navigator,await response.json());
} catch {
 document.getElementById('status').textContent='Filters are unavailable. Use the install buttons or select and copy the repository URL.';
}
`);
const githubMark=await readFile(resolve(root,'assets/github.svg'),'utf8');
const iconLicense=await readFile(resolve(root,'licenses/Octicons-LICENSE.txt'),'utf8');
const url=pack.repositoryURL,title="Source Extensions";
const languages=[...new Set(sources.flatMap(source=>source.languages))].sort((a,b)=>languageLabel(a).localeCompare(languageLabel(b),'en'));
const rows=sources.map(source=>{
 if(!/^icons\/[A-Za-z0-9_.-]+$/.test(source.icon))throw new Error('Invalid local icon');
 const install=`manko://repository/install?${new URLSearchParams({url,source:source.id})}`;
 return `<li id="source-${esc(source.id)}" class="source"><img src="./dist/community/v1/stable/${esc(source.icon)}" width="44" height="44" alt=""><div class="info"><h2>${esc(source.name)}</h2><div class="meta"><span>v${esc(source.version)}</span><span>${esc(source.languages.length>1?'Multilingual':languageLabel(source.languages[0]))}</span><span class="rating">${esc(ratingLabels[source.contentRating]||source.contentRating)}</span></div><p>${esc(source.description)}</p></div><a class="install" ${["retired","serviceunavailable"].includes(source.availability.toLowerCase())?'aria-disabled="true"':`href="${esc(install)}"`} aria-label="Install ${esc(source.name)} in Manko" title="Install ${esc(source.name)} in Manko"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" aria-hidden="true"><path d="M12 3v12m-5-5 5 5 5-5M5 16v4h14v-4"/></svg></a></li>`;
}).join('\n');
await writeFile(resolve(root,'site/index.html'),`<!doctype html>
<!-- GitHub mark from primer/octicons.\n${iconLicense} -->
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light dark"><meta name="referrer" content="no-referrer"><title>${title}</title>
<style>
:root{--accent:#ff9500;--ink:#7a3000;--text:#1c1c1e;--muted:#636366;--background:#f2f2f7;--surface:#fff;--line:#d1d1d6;--soft:#fff0dc;font-family:ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;color:var(--text);background:var(--background);line-height:1.5;color-scheme:light dark}*{box-sizing:border-box}[hidden]{display:none!important}body{margin:0;padding:76px 24px 32px}main{max-width:700px;margin:auto}.github{position:absolute;top:18px;right:20px;width:44px;height:44px;display:grid;place-items:center;color:var(--text);border-radius:50%}.github:hover{background:var(--line)}.github svg{width:24px;height:24px;fill:currentColor}.wordmark{font-size:12px;font-weight:750;letter-spacing:.18em;color:var(--ink);margin-bottom:18px}h1{font-size:clamp(30px,7vw,40px);line-height:1.15;letter-spacing:-.035em;margin:0 0 14px}p{color:var(--muted);margin:0 0 22px}.primary{display:inline-block;border-radius:100px;background:var(--accent);color:#000;padding:12px 22px;text-decoration:none;font-weight:650}.hint{font-size:13px;margin:10px 0 24px}.row{display:flex;gap:8px}.manual label{display:block;margin-bottom:8px;font-size:13px;font-weight:600}input,select{min-width:0;background:var(--surface);color:inherit;border:1px solid var(--line);border-radius:10px;padding:12px;font:inherit;min-height:46px}#repository{flex:1;font:12px ui-monospace,monospace}button,.secondary{background:var(--soft);color:var(--ink);border:0;border-radius:10px;padding:12px 16px;min-height:44px;font:600 14px system-ui;cursor:pointer}#status{min-height:40px;font-size:13px;margin:10px 0 12px}#selected{padding:12px 0 24px}#selected a{color:var(--ink);font-weight:650;text-underline-offset:4px}.filters{display:grid;grid-template-columns:minmax(150px,1fr) 170px 150px;gap:12px;border-top:1px solid var(--line);padding-top:24px}.filters label{display:block;font-size:12px;font-weight:600;margin-bottom:6px}.filters input,.filters select{width:100%}.count{font-size:13px;margin:18px 0 4px}ul{padding:0;margin:0;list-style:none}.source{display:flex;gap:16px;align-items:center;padding:22px 8px;border-bottom:1px solid var(--line);border-radius:8px}.source.selected{background:var(--soft);box-shadow:inset 3px 0 var(--accent)}.source img{flex:none;border-radius:10px;object-fit:contain;background:#fff;padding:4px}.info{flex:1;min-width:0}h2{font-size:17px;line-height:1.3;margin:0 0 5px;overflow-wrap:anywhere}.meta{display:flex;gap:6px 12px;flex-wrap:wrap;color:var(--muted);font-size:12px;align-items:center}.rating{color:var(--ink);background:var(--soft);padding:1px 7px;border-radius:5px}.info p{font-size:13px;margin:8px 0 0}.install{display:grid;place-items:center;width:44px;height:44px;flex:none;color:var(--ink);border-radius:50%;background:var(--soft)}#empty{text-align:center;padding:32px 0}#empty p{margin-bottom:12px}footer{margin-top:28px;font-size:13px}footer a{color:var(--muted);text-underline-offset:3px}[aria-disabled="true"]{opacity:.4;pointer-events:none}:focus-visible{outline:3px solid var(--ink);outline-offset:3px}
@media(max-width:540px){body{padding:72px 18px 28px}.filters{grid-template-columns:1fr}.source{gap:12px;padding:20px 4px}.row{flex-wrap:wrap}#repository{flex-basis:100%}.row button{margin-left:auto}.primary{display:block;text-align:center}}
@media(prefers-color-scheme:dark){:root{--accent:#ff9f0a;--ink:#ff9f0a;--text:#f2f2f7;--muted:#aeaeb2;--background:#000;--surface:#1c1c1e;--line:#38383a;--soft:#322617}}
@media(prefers-contrast:more){:root{--accent:#ffad33;--muted:#3a3a3c;--line:#8e8e93}}@media(prefers-contrast:more) and (prefers-color-scheme:dark){:root{--accent:#ffc478;--ink:#ffd18a;--muted:#e5e5ea;--line:#aeaeb2}}
</style></head><body><a class="github" href="https://github.com/superdupercoolperson/extensions" aria-label="View source on GitHub" title="View source on GitHub">${githubMark}</a><main><div class="wordmark">MANKO</div><h1 id="title">${title}</h1><p>Browse content sources for Manko. Add the repository, then choose your extensions.</p>
<a id="open" class="primary" href="manko://repository/add?url=${encodeURIComponent(url)}">Add to Manko</a><p class="hint">Requires Manko. You review and confirm in the app.</p>
<div class="manual"><label for="repository">Repository URL</label><div class="row"><input id="repository" readonly value="${url}" spellcheck="false"><button id="copy" type="button">Copy</button></div><p id="status" role="status" aria-live="polite">Paste it into Extensions → Add Repository.</p></div>
<div id="selected" hidden><a id="install-selected">Open in Manko</a></div>
<div id="filters" class="filters" hidden><div><label for="search">Search extensions</label><input id="search" type="search" placeholder="Search by name" autocomplete="off"></div><div><label for="language">Language</label><select id="language"><option value="">All languages</option>${languages.map(code=>`<option value="${esc(code)}">${esc(languageLabel(code))}</option>`).join('')}</select></div><div><label for="rating">Content rating</label><select id="rating"><option value="">All ratings</option>${Object.entries(ratingLabels).map(([value,label])=>`<option value="${value}">${label}</option>`).join('')}</select></div></div>
<p class="count" id="count" role="status" aria-live="polite">${sources.length} extensions</p><ul aria-label="Extensions">${rows}</ul><div id="empty" hidden><p>No extensions match your filters.</p><button id="reset" type="button">Reset filters</button></div>
<noscript><p>Filters require JavaScript. Install buttons and the repository URL work without it.</p></noscript><footer>Maintained by superdupercoolperson · <a href="${pack.privacyURL}">Privacy</a></footer></main><script type="module" src="./boot.mjs"></script></body></html>\n`);
await writeFile(resolve(root,'site/.nojekyll'),'');
console.log('Prepared extension catalog. Nothing deployed.');
