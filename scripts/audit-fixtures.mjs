// SPDX-License-Identifier: Apache-2.0
// Original, sanitized response fixtures. Endpoint shapes checked 2026-10-13.
import {runtimeResponse, MINIMAL_PNG_BYTES} from '../extensions/content/test-runtime.mjs';
import {responder as lnori} from '../extensions/content/LNori/tests/fixtures.mjs';
import {responder as katana} from '../extensions/content/MangaKatana/tests/fixtures.mjs';
import {responder as fox} from '../extensions/content/MangaFox/tests/fixtures.mjs';
import {responder as fire, fixture as fireFixture} from '../extensions/content/MangaFire/tests/fixtures.mjs';
const json=(r,value)=>runtimeResponse({url:r.url,mimeType:'application/json',text:JSON.stringify(value)});
const html=(r,text)=>runtimeResponse({url:r.url,mimeType:'text/html',text});
export const ids=['HitomiLA','NHentai','LNori','MangaFox','MangaKatana','MangaFire','MangaBat','OmegaScans'];
export const connection=id=>({baseURL:`https://server.test/${id.toLowerCase()}/`,options:id==='Suwayomi'?{sourceID:'42'}:{}});
const gallery={id:101,media_id:'9001',title:{english:'The Quiet Observatory',pretty:'The Quiet Observatory'},cover:{path:'/galleries/9001/cover.png',width:600,height:900},thumbnail:{path:'/galleries/9001/thumb.png',width:300,height:450},pages:[1,2,3].map(number=>({number,path:`/galleries/9001/${number}.png`})),tags:[{type:'language',name:'english'},{type:'artist',name:'Manko Fixtures'},{type:'tag',name:'landscape'}],num_pages:3,num_favorites:12,upload_date:1700000000};
const omega={id:7,series_slug:'sample-comic',title:'The Quiet Observatory',series_type:'Comic',thumbnail:'https://media.omegascans.org/cover.png',author:'Manko Fixtures',description:'<p>An original journey through the night sky.</p>',status:'Ongoing',tags:'$b'};
const chapter={chapter_slug:'chapter-1',chapter_name:'Chapter 1',price:0,series:{id:7,series_slug:'sample-comic'},created_at:'2026-10-01T00:00:00.000Z'};
const flight=values=>values.map((v,i)=>`<script>self.__next_f.push(${JSON.stringify([1,`${i===0?'a':'b'}:${JSON.stringify(v)}\n`])})</script>`).join('');
const mangaInfo='<div class="panel-story-info"><h1>The Quiet Observatory</h1><span class="info-image"><img src="https://img-r2.2xstorage.com/cover.png"></span><table class="variations-tableInfo"><tr><td>Author:</td><td><a>Manko Fixtures</a></td></tr><tr><td>Status:</td><td>Ongoing</td></tr><tr><td>Genres:</td><td><a>Adventure</a></td></tr></table></div><div id="panel-story-info-description">An original journey through the night sky.</div><div class="row-content-chapter"><a href="/manga/sample/chapter-1">Chapter 1</a><a href="/manga/sample/chapter-2">Chapter 2</a></div>';
const hitomiGallery=id=>({id:String(id),title:id===1?'The Quiet Observatory and the Cartographer Who Followed the Last Light Beyond the Northern Mountains':`Observatory Journey ${id}`,language:'english',language_localname:'English',type:'manga',date:'2026-01-01 00:00:00+00',artists:[{artist:'Manko Fixtures'}],files:[1,2,3].map(n=>({hash:(id*10+n).toString(16).padStart(64,'0'),name:`${n}.png`,hasavif:1,haswebp:1,width:600,height:900}))});
export function responder(id) {
 return r=>{
  const u=new URL(r.url),p=u.pathname;
  if(r.imageResource || /\.(png|jpg|webp|avif)$/.test(p))return runtimeResponse({url:r.url,mimeType:'image/png',bytes:MINIMAL_PNG_BYTES});
  if(id==='LNori') {
   const response=lnori(r);
   if(p.startsWith('/book/')) {
    const paragraphs=Array.from({length:30},(_,i)=>`<p>${i+1}. Mira opened the observatory before sunrise. Beyond the window, the last stars faded into a blue sky. She recorded their positions in a notebook, leaving room for tomorrow's discoveries.</p>`).join('');
    return html(r,'<html><body>'+Buffer.from(response.dataBase64,'base64').toString().replace('<p onclick="bad()">A &amp; B<script>bad()</script></p>',paragraphs)+'</body></html>');
   }
   return response;
  }
  if(id==='MangaKatana')return katana(r);
  if(id==='MangaFox'){
   const response=fox(r);
   return p.includes('/c002.5/')?html(r,Buffer.from(response.dataBase64,'base64').toString().replaceAll('/v01/c001/','/c002.5/')):response;
  }
  if(id==='MangaFire'){
   if(['/api/chapters/702','/api/chapters/703'].includes(p))return json(r,{data:{...fireFixture.pages,id:Number(p.split('/').at(-1))}});
   return fire(r);
  }
  if(id==='NHentai') {
   if(p==='/api/v2/galleries/popular')return json(r,[gallery]);
   if(['/api/v2/search','/api/v2/galleries'].includes(p))return json(r,{result:[{...gallery,english_title:gallery.title.english,thumbnail:'/galleries/9001/thumb.png'}],num_pages:1,per_page:25,total:1});
   if(p==='/api/v2/galleries/101')return json(r,gallery);
  }
  if(id==='HitomiLA') {
   if(p.endsWith('.nozomi')) {const all=Buffer.alloc(30*4);for(let i=0;i<30;i++)all.writeInt32BE(i+1,i*4);const match=/bytes=(\d+)-(\d+)/.exec(r.headers.Range||'');const start=match?+match[1]:0,end=match?Math.min(+match[2],all.length-1):all.length-1;return runtimeResponse({url:r.url,status:match?206:200,mimeType:'application/octet-stream',headers:{'Content-Range':`bytes ${start}-${end}/${all.length}`},bytes:all.subarray(start,end+1)});}
   const block=/\/galleryblock\/(\d+)\.html/.exec(p),detail=/\/galleries\/(\d+)\.js/.exec(p);
   if(block){const g=hitomiGallery(+block[1]),h=g.files[0].hash;return html(r,`<div class="manga"><div class="dj-img1"><picture><source type="image/avif" data-srcset="//tn.gold-usergeneratedcontent.net/avifbigtn/${h.slice(-1)}/${h.slice(-3,-1)}/${h}.avif 2x"></picture></div><h1><a href="/galleries/${g.id}.html">${g.title}</a></h1><a href="/index-english.html">english</a></div>`);}
   if(detail)return html(r,`var galleryinfo = ${JSON.stringify(hitomiGallery(+detail[1]))};`);
   if(p.endsWith('/gg.js'))return html(r,"gg = {m:function(g){var o=1;switch(g){case 256:o=0;break;}return o;},s:function(h){var m=/(..)(.)$/.exec(h);return parseInt(m[2]+m[1],16).toString(10);},b:'123/'};");
  }
  if(['MangaBat'].includes(id)) {
   if(p.startsWith('/genre/')||p.startsWith('/search/')||p.startsWith('/author/'))return html(r,'<div class="panel_story_list"><div class="story_item"><a href="/manga/sample" title="The Quiet Observatory"><img src="https://img-r2.2xstorage.com/cover.png"></a></div></div>');
   if(p.includes('chapter-'))return html(r,'<div class="container-chapter-reader">'+[1,2,3].map(n=>`<img src="https://img-r2.2xstorage.com/page-${n}.png">`).join('')+'</div>');
   if(p==='/manga/sample')return html(r,mangaInfo);
  }
  if(id==='OmegaScans') {
   if(p==='/query')return json(r,{meta:{current_page:1,last_page:1},data:[omega]});
   if(p==='/tags')return json(r,[{id:1,name:'Adventure'}]);
   if(p==='/chapter/query')return json(r,{meta:{last_page:1},data:[chapter]});
   if(p==='/series/sample-comic')return html(r,flight([omega,[{id:1,name:'Adventure'}]]));
   if(p==='/series/sample-comic/chapter-1')return html(r,flight([{...chapter,chapter_data:{images:[1,2,3].map(n=>`https://media.omegascans.org/page-${n}.png`)}}]));
  }
  if(id==='Komga') {
   const series={id:'series-1',name:'The Quiet Observatory',metadata:{title:'The Quiet Observatory',language:'en',status:'ONGOING',authors:[{name:'Manko Fixtures',role:'writer'}]}};
   if(p.endsWith('/libraries'))return json(r,[]);
   if(p.endsWith('/books'))return json(r,{content:[{id:'book-1',name:'First Light',metadata:{numberSort:1},media:{mediaType:'application/epub+zip'}}],last:true});
   if(p.endsWith('/series/series-1'))return json(r,series);
   if(p.endsWith('/series'))return json(r,{content:[series],last:true});
  }
  if(id==='Kavita') {
   const series={id:1,name:'The Quiet Observatory',format:3,writers:[{name:'Manko Fixtures'}],publicationStatus:2};
   if(p.endsWith('libraries'))return json(r,[]);
   if(p.endsWith('all-v2'))return json(r,[series]);
   if(p.endsWith('volumes'))return json(r,[{number:1,chapters:[{id:8,title:'First Light',number:'1',files:[{filePath:'book.epub'}]}]}]);
   if(p.includes('/api/Series/'))return json(r,series);
  }
  if(id==='OPDS')return html(r,'<feed xmlns="http://www.w3.org/2005/Atom"><entry><id>one</id><title>The Quiet Observatory</title><author><name>Manko Fixtures</name></author><link rel="http://opds-spec.org/image" href="cover.png" type="image/png"/><link rel="http://opds-spec.org/acquisition" href="book.epub" type="application/epub+zip"/></entry></feed>');
  if(id==='Suwayomi') {
   const {query}=r.body,manga={id:3,title:'The Quiet Observatory',author:'Manko Fixtures',genre:['manga']};
   return json(r,{data:query.includes('MankoDiscovery')?{serverSettings:{ip:'localhost'}}:query.includes('MankoChapters')?{chapters:{nodes:[{id:8,name:'First Light',sourceOrder:1,pageCount:3}]}}:query.includes('MankoSearch')?{searchSourceManga:{mangas:[manga],hasNextPage:false}}:query.includes('MankoBrowse')?{fetchSourceManga:{mangas:[manga],hasNextPage:false}}:{manga}});
  }
  if(id==='AniList') {
   const {query}=JSON.parse(r.body),media={id:1,format:'MANGA',title:{userPreferred:'The Quiet Observatory'},coverImage:{large:'https://s4.anilist.co/fixture.png'}};
   return json(r,{data:query.includes('Page(page')?{Page:{media:[media]}}:query.includes('Media(id')?{Media:{mediaListEntry:{mediaId:1,status:'CURRENT',score:80,progress:4,progressVolumes:1,updatedAt:1700000000}}}:query.includes('Viewer')?{Viewer:{id:42}}:query.includes('SaveMediaListEntry')?{SaveMediaListEntry:{mediaId:1}}:{MediaListCollection:{lists:[{name:'Reading',entries:[{mediaId:1,media}]}]}}});
  }
  if(id==='MyAnimeList') {
   const node={id:2,title:'The Quiet Observatory',media_type:'manga',main_picture:{large:'https://cdn.myanimelist.net/fixture.png'}};
   if(p==='/v2/manga')return json(r,{data:[{node}]});
   if(p==='/v2/manga/2')return json(r,{id:2,my_list_status:{status:'reading',score:8,num_volumes_read:1,num_chapters_read:4,updated_at:'2026-01-01T00:00:00Z'}});
   if(p.endsWith('/my_list_status'))return json(r,{status:'reading',num_chapters_read:5});
   if(p.includes('/mangalist'))return json(r,{data:[{node,list_status:{status:'reading'}}],paging:{}});
  }
  throw new Error(`Unrecorded ${id} fixture request: ${r.method} ${r.url}`);
 };
}
