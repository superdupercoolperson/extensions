/* SPDX-License-Identifier: Apache-2.0 */
import test from "node:test";
import assert from "node:assert/strict";
import {resolve} from "node:path";
import {loadContentExtension,runtimeResponse,metadataCacheFixture} from "../../test-runtime.mjs";
for(const id of ['LNori','OmegaScans','MangaFox']){
 const main=resolve(import.meta.dirname,`../../${id}/main.wasm`);
 test(`${id} hands off visible challenges and preserves rate-limit errors`,async()=>{
  const challenged=await loadContentExtension(main,r=>runtimeResponse({url:r.url,status:403,headers:{'cf-mitigated':'challenge'},text:'<html>Just a moment...</html>'}),{cache:metadataCacheFixture()});
  await assert.rejects(challenged.extension.search({query:'sample'}),{type:'challengeRequired'});assert.equal(challenged.challenges.length,1);
  const limited=await loadContentExtension(main,r=>runtimeResponse({url:r.url,status:429,headers:{'Retry-After':'30'}}),{cache:metadataCacheFixture()});
  await assert.rejects(limited.extension.search({query:'sample'}),e=>e.type==='rateLimited'&&e.retryAfterSeconds===30);
  assert.equal(limited.calls.length,1);
 });
 test(`${id} refuses a redirect to an undeclared host`,async()=>{
  const loaded=await loadContentExtension(main,()=>runtimeResponse({url:'https://evil.test/login',text:'<html>Redirected</html>'}));
  await assert.rejects(loaded.extension.search({query:'sample'}),{type:'hostNotAllowed'});
 });
}
