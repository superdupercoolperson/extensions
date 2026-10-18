// SPDX-License-Identifier: Apache-2.0
// Original synthetic markup matching the public site inspected 2026-10-18.
import {runtimeResponse,MINIMAL_PNG_BYTES} from '../../test-runtime.mjs';
export const id='quiet-observatory.123';
export const row=`<div class="item"><div class="wrap_img"><a href="/manga/${id}"><img lazy-src="/imgs/cover/sample.png" src="placeholder.png"></a></div><h3 class="title"><a href="/manga/${id}">The Quiet Observatory &amp; Beyond</a></h3></div>`;
export const catalog=(page=1)=>`<div id="book_list">${page===1?row+row:''}</div>${page===1?'<div class="pagination"><a href="/manga/page/2">Next</a></div>':''}`;
export const homepage=`<div id="hot_update">${row+row}</div>${catalog()}`;
export const chapterRow=(n)=>`<tr><td><div class="chapter"><a href="/manga/${id}/c${n}">Chapter ${n}: First &amp; Last Light</a></div></td><td><div class="update_time">Oct-17-2026</div></td></tr>`;
export const series=`<div id="single_book"><div class="cover"><img src="/imgs/cover/sample.png"></div><h1>The Quiet Observatory &amp; Beyond</h1><div class="alt_name">Observatory ; 星</div><div class="authors"><a>Manko Fixtures</a><a>Another Artist</a></div><div class="genres"><a href="/genre/fantasy">Fantasy</a></div><div class="status">Ongoing</div><div class="summary"><p>An original journey through the night sky.</p></div><div class="chapters"><table>${chapterRow('2.5')}${chapterRow('1')}${chapterRow('1')}</table></div></div>`;
export const reader=(token='fresh')=>`<div id="imgs"><img data-src="#"></div><script>var ytaw=['https://evil.test/advert.jpg'];var thzq=['https://i1.mangakatana.com/token/${token}/1.png','https://i1.mangakatana.com/token/${token}/2.png',];</script>`;
export function responder(r){
 const u=new URL(r.url),p=u.pathname;
 if(r.imageResource)return runtimeResponse({url:r.url,mimeType:'image/png',bytes:MINIMAL_PNG_BYTES});
 let body;
 if(p===`/manga/${id}`)body=series;
 else if(p.startsWith(`/manga/${id}/c`))body=reader();
 else if(u.searchParams.get('search')?.toLowerCase()==='missing')body='<div id="book_list">Not found any results.</div>';
 else if(p==='/'&&!u.searchParams.has('search'))body=homepage;
 else if(p.startsWith('/manga/')||p.startsWith('/latest')||p.startsWith('/new-manga')||p.startsWith('/page/')||u.searchParams.has('search'))body=catalog(p.includes('/page/')?2:1);
 else throw new Error(`Unexpected fixture request: ${r.url}`);
 return runtimeResponse({url:r.url,mimeType:'text/html',text:body});
}
