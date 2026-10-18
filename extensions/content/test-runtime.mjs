/* Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0 */
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { instantiate } from "../../packages/cli/lib/wasm.mjs";

export const MINIMAL_PNG_BYTES = Object.freeze(Array.from(Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=",
  "base64"
)));
export const MINIMAL_WEBP_BYTES = Object.freeze(Array.from(Buffer.from(
  "UklGRhoAAABXRUJQVlA4TA4AAAAvAAAAAAcQEf0PRET/Aw==",
  "base64"
)));
export const MINIMAL_GIF_BYTES = Object.freeze(Array.from(Buffer.from(
  "R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==",
  "base64"
)));
export const MINIMAL_AVIF_BYTES = Object.freeze(Array.from(Buffer.from(
  "AAAAHGZ0eXBhdmlmAAAAAGF2aWZtaWYxbWlhZgAAAPBtZXRhAAAAAAAAAChoZGxyAAAAAAAAAABwaWN0AAAAAAAAAAAAAAAAbGliYXZpZgAAAAAOcGl0bQAAAAAAAQAAAB5pbG9jAAAAAEQAAAEAAQAAAAEAAAEUAAAAFQAAAChpaW5mAAAAAAABAAAAGmluZmUCAAAAAAEAAGF2MDFDb2xvcgAAAABoaXBycAAAAElpcGNvAAAAFGlzcGUAAAAAAAAAAQAAAAEAAAAOcGl4aQAAAAABCAAAAAxhdjFDgQAcAAAAABNjb2xybmNseAABAA0ABoAAAAAXaXBtYQAAAAAAAAABAAEEAQKDBAAAAB1tZGF0EgAKBxgABhgIaDUyCBAAABjhQnIQ",
  "base64"
)));

export function runtimeResponse({ url = "https://example.invalid/", status = 200, headers = {}, mimeType, cookies = [], text, bytes } = {}) {
  const data = bytes === undefined ? Buffer.from(text || "", "utf8") : Buffer.from(bytes);
  return { url, status, headers, mimeType, cookies, dataBase64: data.toString("base64") };
}

async function loadExtension(mainPath, responder, kind, options = {}) {
  const metadata = JSON.parse(await readFile(resolve(dirname(mainPath),"extension.json"),"utf8"));
  const root = resolve(dirname(mainPath),"../../..");
  const {dispatch} = instantiate(await readFile(resolve(root,"target/wasm32-unknown-unknown/release",metadata.crate.replaceAll("-","_")+".wasm")));
  const calls=[], sleeps=[], challenges=[], state=new Map(Object.entries(options.initialState || {})), secureState=new Map(Object.entries(options.secureState || {}));
  const cache=options.cache || metadataCacheFixture(100);
  const imageOverrides=new Map();
  let sequence=0, cookies=structuredClone(options.initialCookies || []);
  const context={cache,http:{request:responder},secureState:{set:(key,value)=>secureState.set(key,value),remove:key=>secureState.delete(key)}};
  async function service({service,arguments:args},owner) {
    switch(service) {
    case "http": {
      const request={...args, headers:{...args.headers}};
      if (args.authenticated) {
        const token=secureState.get("oauth_access_token");
        if (!token) throw new Error("Tracker authentication required");
        request.headers.Authorization="Bearer "+token;
      }
      if (args.bodyBase64) {
        const bytes=Buffer.from(args.bodyBase64,"base64");
        if(kind==="tracker") request.body=bytes.toString();
        else {try {request.body=JSON.parse(bytes.toString());} catch {request.body=bytes.toString();}}
      }
      calls.push(request);
      const loader=imageOverrides.get(owner) || context.http.imageResource;
      const response=await (args.imageResource && loader ? loader(request) : responder(request,calls.length-1));
      if (response.cookies?.length) cookies=structuredClone(response.cookies);
      return response;
    }
    case "state": case "secureState": {
      if (args.key.startsWith("oauth_") || args.key.startsWith("__native_") || args.key === "cookie_store_cookies") throw new Error("Reserved native state");
      const store=service==="state"?state:secureState;
      if(args.operation==="get") return store.get(args.key) ?? null;
      if(args.operation==="set") store.set(args.key,args.value);
      else if(args.operation==="remove") store.delete(args.key);
      else if(args.operation==="reset") store.clear();
      return null;
    }
    case "clock": return {now:options.now || "2026-01-01T00:00:00.000Z",milliseconds:Date.parse(options.now || "2026-01-01T00:00:00.000Z"),random:Math.floor(Math.random()*0xfffffffe)+1};
    case "sleep": sleeps.push(args.milliseconds);return null;
    case "challenge": challenges.push(args.url);throw Object.assign(new Error("Visible verification required"),{name:"ChallengeRequiredError",type:"challengeRequired",url:args.url});
    case "cache": {
      if(options.globals?.Date) cache.setTime(options.globals.Date.now()/1000);
      const value=await cache.dispatch({...args,owner});
      return args.operation==="read" ? {...value,available:Boolean(options.cache)} : value;
    }
    case "log": return null;
    default: throw new Error("Unsupported host service: "+service);
    }
  }
  function unwrap(result) {
    if (!result.ok) throw Object.assign(new Error(result.error?.message || "Extension failed"),result.error);
    return result.value;
  }
  function invoke(method,args={}) {
    if(method==="invalidateCache") cache.clear();
    const owner=String(++sequence);
    if(args?.http) imageOverrides.set(owner,args.http.imageResource || args.http.request);
    const result=dispatch({action:"invoke",invocationID:owner,method:kind==="tracker" && method==="search"?"trackerSearch":method,arguments:args});
    if(!result.pending) {try {return unwrap(result);} catch(error) {return Promise.reject(error);}}
    return (async()=>{
      let output=result;
      for(let step=0;step<10000;step++) {
        if(!output.pending) return unwrap(output);
        const responses=await Promise.all(output.pending.map(async request=>{
          try {return {id:request.id,payload:await service(request,owner),error:null};}
          catch(error) {return {id:request.id,payload:null,error:{name:error.name,message:error.message,type:error.type,url:error.url}};}
        }));
        output=dispatch({action:"resume",invocationID:owner,responses});
      }
      throw new Error("Continuation limit exceeded");
    })();
  }
  const extension=new Proxy({}, {get:(_,method)=>(args)=>{
    if(kind==="tracker" && method==="progress" && typeof args==="string") args={remoteWorkID:args};
    if(kind==="tracker" && method==="update" && !args?.progress) args={progress:args};
    return invoke(method,args);
  }});
  await extension.initialize({});
  return {extension,calls,sleeps,challenges,state,secureState,webCalls:[],context,get cookies(){return structuredClone(cookies);},dispatch};
}

export function loadContentExtension(mainPath, responder, options = {}) {
  return loadExtension(mainPath, responder, "content", options);
}

export function loadTrackerExtension(mainPath, responder, options = {}) {
  return loadExtension(mainPath, responder, "tracker", options);
}


// A host cache fixture retained across VM restarts. Swift tests cover the native
// disk broker and fencing; source tests use this to assert request reuse.
export function metadataCacheFixture(maximumEntries=1000) {
  const entries=new Map();
  const leases=new Map();
  let now=0,reload=false,generation=0;
  return {
    entries,
    advance(seconds){now+=seconds;},
    setTime(value){now=value;},
    setReload(value){reload=value;},
    clear(){generation++;entries.clear();},
    async remove(key){generation++;entries.delete(key);},
    dispatch(args) {
      if(args.operation==="acquire") {
        if(!leases.has(args.key)) {leases.set(args.key,[]);return {};}
        return new Promise(resolve=>leases.get(args.key).push(resolve));
      }
      if(args.operation==="release") {
        const waiters=leases.get(args.key);
        if(waiters?.length) waiters.shift()({}); else leases.delete(args.key);
        return {};
      }
      if(args.operation==="read") {
        const entry=entries.get(args.key);
        if(entry) {entries.delete(args.key);entries.set(args.key,entry);}
        const hit=entry && !(reload && args.ttlSeconds!==null) && (args.ttlSeconds===null || now-entry.time<args.ttlSeconds);
        return {hit:Boolean(hit),value:hit?structuredClone(entry.value):null,permit:{generation}};
      }
      if(args.operation==="write" && args.permit?.generation===generation && Buffer.byteLength(JSON.stringify(args.value))<=16777216) {
        entries.delete(args.key);entries.set(args.key,{value:structuredClone(args.value),time:now});
        while(entries.size>maximumEntries) entries.delete(entries.keys().next().value);
      }
      if(args.operation==="remove") {generation++;entries.delete(args.key);}
      return {};
    }
  };
}
