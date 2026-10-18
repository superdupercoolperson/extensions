/* SPDX-License-Identifier: Apache-2.0 */
import assert from "node:assert/strict";
import {resolve} from "node:path";
import {loadContentExtension,runtimeResponse,metadataCacheFixture,MINIMAL_WEBP_BYTES} from "../../test-runtime.mjs";
const main=resolve(import.meta.dirname,"../main.wasm");
const row='<li><a href="/manga/sample_manga/" title="Sample &amp; Story"><img src="//fmcdn.mfcdn.net/store/cover.jpg?token=public&amp;ttl=1799999999"></a></li>';
function catalog(search,page){return `<ul class="manga-list-${search?4:1}-list line">${page===1?row+row:''}</ul><div class="pager-list-left">${page===1?`<a href="${search?'/search?page=2&amp;title=sample':'/directory/2.html'}">2</a>`:''}</div>`;}
const series=`<img class="detail-info-cover-img" src="//fmcdn.mfcdn.net/store/cover.jpg"><span class="detail-info-right-title-font">Sample Story</span><span class="detail-info-right-title-tip">Ongoing</span><p class="detail-info-right-say"><a>Sample Author</a></p><p class="fullcontent">Full synopsis</p><div id="chapterlist"><a href="/manga/sample_manga/v01/c001/1.html" title="Chapter 1">Chapter 1</a><a href="/manga/sample_manga/v01/c001/1.html">Chapter 1 repeated</a><a href="/manga/sample_manga/c002.5/1.html">Chapter 2.5</a></div><a href="/manga/recommended/c001/1.html">Recommended Comic</a>`;
function reader(page){return `<div id="viewer"><a><img id="image" src="//zjcdn.mangafox.me/store/${page}.webp?token=public&amp;ttl=1799999999"></a><img style="display:none" src="//evil.test/preload.jpg"></div><select class="mangaread-page">${[1,2,3].map(i=>`<option value="//m.fanfox.net/manga/sample_manga/v01/c001/${i}.html">${i}</option>`).join('')}</select>`;}
export const homepage = ['Hot Manga Releases','Being Read Right Now','Popular Manga Ranking','Recommended','New Manga Release','Lastest Updates','Trending Manga'].map((title,index)=>`<div class="manga-list-${index===5?4:1}-title">${title}</div><ul class="manga-list-1-list">${row}${row}</ul>`).join('');
function responder(request){const url=new URL(request.url);let html;
 if(url.pathname==='/')html=homepage;
 else if(url.pathname.startsWith('/directory'))html=catalog(false,url.pathname==='/directory/'?1:2);
 else if(url.pathname==='/search')html=catalog(true,Number(url.searchParams.get('page')));
 else if(url.pathname==='/manga/sample_manga/')html=series;
 else if(url.hostname==='m.fanfox.net')html=reader(Number(url.pathname.split('/').at(-1).replace('.html','')));
 else if(request.imageResource)return runtimeResponse({url:request.url,mimeType:'image/webp',bytes:MINIMAL_WEBP_BYTES});
 else throw new Error(`Unexpected ${request.url}`);
 return runtimeResponse({url:request.url,text:html});
}

export { row, catalog, series, reader, responder };
