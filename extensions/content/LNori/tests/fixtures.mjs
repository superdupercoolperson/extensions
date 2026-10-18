/* SPDX-License-Identifier: Apache-2.0 */
import assert from "node:assert/strict";
import {resolve} from "node:path";
import {loadContentExtension,runtimeResponse,metadataCacheFixture,MINIMAL_WEBP_BYTES} from "../../test-runtime.mjs";
const main=resolve(import.meta.dirname,"../main.wasm");
const workID="1/sample-novel",bookID="11/sample-novel-volume-1";
function card(id,title=`Sample ${id}`){return `<article class="card" data-id="${id}" data-t="${title}" data-a="Sample Author" data-tags="fantasy,adventure" data-d="2026" data-v="2" data-rel="${100-id}"><figure><a href="/series/${id}/${id===1?'sample-novel':`sample-${id}`}" title="${title}"><img src="https://cdn.lnori.com/cover/${id}.webp"></a></figure></article>`;}
const library=`<section id="grid">${Array.from({length:26},(_,i)=>card(i+1)).join('')}${card(1)}</section><script src="/cdn-cgi/challenge-platform/scripts/jsd/main.js"></script>`;
const schema={name:"Sample & Story",url:`https://lnori.com/series/${workID}`,image:"https://cdn.lnori.com/cover/1.webp",author:{name:"Sample Author"},description:"A sample synopsis",hasPart:[{name:"Volume 1",position:"1",url:`https://lnori.com/book/${bookID}`},{name:"Volume 1",position:"1",url:`https://lnori.com/book/${bookID}`}]};
const series=`<script type="application/ld+json">${JSON.stringify(schema)}</script>`;
const bookSchema={id:"11",additionalProperty:[{name:"series_id",value:"1"},{name:"series_slug",value:"sample-novel"}]};
const book=`<script type="application/ld+json">${JSON.stringify(bookSchema)}</script><h1 id="book-title">Sample &amp; Story Vol. 1</h1><a href="/series/${workID}">Series</a><article class="content-body"><section id="chapter1"><h2>Chapter One</h2><p onclick="bad()">A &amp; B<script>bad()</script></p><picture><source srcset="https://img.lnori.com/11-01.jxl"><img src="https://img.lnori.com/11-01.jpg" onerror="bad()"></picture></section></article>`;
export const homepage = `<section id="hero-section">${[1,2,3,4,5].map(id=>`<div data-link="/series/${id}/${id===1?'sample-novel':`sample-${id}`}" data-title="Original Journey ${id}" data-image="https://cdn.lnori.com/cover/${id}.webp"></div>`).join('')}</section><section id="summer"><h2>SUMMER 2026 ANIME</h2>${[3,1].map(id=>`<a href="/series/${id}/${id===1?'sample-novel':`sample-${id}`} "><img src="https://cdn.lnori.com/cover/${id}.webp" alt="Original Journey ${id}"></a>`).join('').replaceAll(' ">','">')}</section><section id="library">${Array.from({length:26},(_,i)=>`<a href="/series/${i+1}/${i===0?'sample-novel':`sample-${i+1}`}"><img src="https://cdn.lnori.com/cover/${i+1}.webp" alt="Original Journey ${i+1}"></a>`).join('')}</section>`;
function responder(request){
 const path=new URL(request.url).pathname;
 assert.equal(request.method,"GET");
 if(path==="/")return runtimeResponse({url:request.url,text:homepage});
 if(path==="/library")return runtimeResponse({url:request.url,text:library});
 if(path===`/series/${workID}`)return runtimeResponse({url:request.url,text:series});
 if(path===`/book/${bookID}`)return runtimeResponse({url:request.url,text:book});
 if(request.imageResource)return runtimeResponse({url:request.url,mimeType:"image/webp",bytes:MINIMAL_WEBP_BYTES});
 throw new Error(`Unexpected ${request.url}`);
}

export { workID, bookID, card, library, schema, series, bookSchema, book, responder };
