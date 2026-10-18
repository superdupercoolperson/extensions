// SPDX-License-Identifier: Apache-2.0
// Compiled contract fixture. Fixed-width labels are specialized by Swift tests.
use base64::{engine::general_purpose::STANDARD, Engine};
use manko_sdk::*;
use serde_json::{json, Value};
use std::cell::Cell;
const ID: &str = "MANKO_FIXTURE_ID_________________________________________________";
const KIND: &str = "MANKO_FIXTURE_KIND_______________________________________________";
const FLAVOR: &str = "MANKO_FIXTURE_FLAVOR_____________________________________________";
thread_local! {
    static LOADS: Cell<u32> = const {Cell::new(0)};
    static IMAGE_LOADS: Cell<u32> = const {Cell::new(0)};
    static ACTIVE_IMAGES: Cell<u32> = const {Cell::new(0)};
    static MAX_IMAGES: Cell<u32> = const {Cell::new(0)};
}
fn label(value: &'static str) -> &'static str {
    value.trim_end_matches('_')
}
fn card(title: Value) -> Value {
    json!({"items":[{"workId":"work-1","title":title,"subtitle":"Native","imageUrl":"https://fixture.example/cover.jpg","contentRating":"SAFE","mediaKind":"lightNovel"}],"metadata":null})
}
async fn state(
    context: &Context,
    storage: &str,
    operation: &str,
    key: &str,
    value: Value,
) -> Result<Value> {
    context
        .call(
            storage,
            json!({"operation":operation,"key":key,"value":value,"namespace":"victim"}),
        )
        .await
}
fn increment(image: bool) -> u32 {
    if image {
        IMAGE_LOADS.with(|v| {
            let next = v.get() + 1;
            v.set(next);
            next
        })
    } else {
        LOADS.with(|v| {
            let next = v.get() + 1;
            v.set(next);
            next
        })
    }
}
async fn cache(context: &Context, key: &str, image: bool, slow: bool) -> Result<Value> {
    context
        .remember(
            key,
            Some(1800.0),
            async {
                let serial = increment(image);
                context
                    .call("sleep", json!({"milliseconds":if slow {150} else {0}}))
                    .await?;
                Ok(json!({"serial":serial}))
            },
            |v| {
                if v["serial"].is_u64() {
                    Ok(())
                } else {
                    Err(error("Invalid cached fixture", "invalidResponse"))
                }
            },
        )
        .await
}
fn run(context: Context, method: String, args: Value) -> Operation {
    Box::pin(async move {
        let flavor = label(FLAVOR);
        let api = flavor == "API11Fixture";
        match method.as_str() {
        "initialize"=>{
            if flavor=="BlockedFetchFixture" {context.call("fetch",json!({})).await?;}
            if flavor=="NativeContentFixture" || flavor=="NativeTrackerFixture" {
                state(&context,"state","set","initialized",json!(true)).await?;
                if flavor=="NativeContentFixture" {state(&context,"secureState","set","token",json!("synthetic")).await?;}
            }
            Ok(Value::Null)
        },
        "invalidateCache"|"synchronizeManagedCollection"|"update"=>Ok(Value::Null),
        "settings"=>Ok(json!({"id":"settings","title":"Fixture Settings","fields":[{"id":"enabled","kind":"toggle","title":"Enabled","options":[],"isRequired":false}]})),
        "discoverSections"=>Ok(if api {json!([{"id":"latest","title":"Latest","type":3}])} else {json!([{"id":"latest","title":"Latest","subtitle":"Synthetic works","type":3,"presentation":"latestGallery"}])}),
        "searchFilters"=>Ok(if api {json!({"id":"fixture-search","title":"Fixture Search","fields":[{"id":"artist","inputKind":"lookup","queryPrefix":"artist:","title":"Artist","supportsExclusion":true,"options":[]}],"sortOptions":[{"id":"popular","title":"Popular"}],"defaultSortID":"popular","sortedBrowseSectionID":"latest"})} else {json!({"id":"search","title":"Search","fields":[{"id":"language","kind":"selection","title":"Language","options":[{"id":"en","title":"English"}],"isRequired":false}]})}),
        "searchSuggestions"=>{
            let query=text(&args["query"]);
            if query=="fail" {return Err(json!({"name":"Error","message":"suggestion failure"}));}
            if query=="malformed" {return Ok(json!({}));}
            Ok(json!((0..args["limit"].as_u64().unwrap_or(10).min(50)).map(|index|json!({"fieldID":args["fieldID"].as_str().unwrap_or(if index%2==0 {"artist"}else{"tag"}),"value":format!("{query}-{index}"),"title":format!("{query} {index}")})).collect::<Vec<_>>()))
        },
        "search"|"discover"=>{
            let query=text(&args["query"]);
            if flavor=="InfiniteLoopFixture" {loop {std::hint::black_box(1);}}
            if flavor=="DynamicCodeFixture" {return Ok(card(json!(((0..=8).sum::<u32>()+12).to_string())));}
            if flavor=="ChallengeFixture" {return context.call("challenge",json!({"url":"https://fixture.example/challenge"})).await;}
            if flavor=="ProtocolRelativeContentFixture" && method=="discover" {
                return Ok(json!({"items":[
                    {"workId":"protocol-relative","title":"Protocol relative","imageUrl":"//fixture.example/cover.jpg"},
                    {"workId":"root-relative","title":"Root relative","imageUrl":"/cover.jpg"},
                    {"workId":"insecure","title":"Insecure","imageUrl":"http://fixture.example/cover.jpg"},
                    {"workId":"undeclared","title":"Undeclared","imageUrl":"https://undeclared.example/cover.jpg"},
                    {"workId":"credentialed","title":"Credentialed","imageUrl":"https://user:secret@fixture.example/cover.jpg"},
                    {"workId":"private-host","title":"Private host","imageUrl":"https://127.0.0.1/cover.jpg"}],"metadata":null}));
            }
            if args["sort"]=="cancelled-refresh" {
                context.remember("cancelled-discovery-refresh",Some(60.0),async {
                    let response=context.request("https://fixture.example/http-cancellation","GET",json!({}),None,false).await?;
                    Ok(json!({"serial":status(&response)}))
                }, |_|Ok(())).await?;
            }
            match query {
                "structured-search"=>return Ok(card(json!(args.to_string()))),
                "typed-error"=>return Err(json!({"name":"TypeError","message":"Precise source failure"})),
                "string-error"=>return Err(json!({"message":"Plain thrown value"})),
                "named-error"=>return Err(json!({"name":"ServiceError","message":"HTTP 403"})),
                "large-error"=>return Err(json!({"name":"Error","message":"x".repeat(10240)})),
                "malicious-envelope"=>return Err(json!({"name":"N".repeat(512),"message":"m".repeat(12288),"code":"C".repeat(512),"type":"T".repeat(512),"stack":"s".repeat(12288)})),
                "isolation-state"=>{
                    let secret=state(&context,"secureState","get","audit-secret",Value::Null).await?;
                    let ordinary=state(&context,"state","get","audit-value",Value::Null).await?;
                    let cookies=state(&context,"secureState","get","cookie_store_cookies",Value::Null).await.unwrap_or(Value::Null);
                    for key in ["browser_session_profile","pending_browser_verification"] {let _=state(&context,"secureState","set",key,json!("forged")).await;}
                    state(&context,"state","set","audit-write",json!("owned")).await?;
                    state(&context,"secureState","set","audit-write",json!("owned")).await?;
                    return Ok(card(json!(json!({"secret":secret,"ordinary":ordinary,"cookies":cookies,"profile":null,"pending":null}).to_string())));
                },
                "http-concurrent-cookies"=>{
                    futures::try_join!(context.request("https://fixture.example/http-cookie-a","GET",json!({}),None,false),context.request("https://fixture.example/http-cookie-b","GET",json!({}),None,false))?;
                    return Ok(card(json!("concurrent-cookies")));
                },
                "cloudflare-fallback"|"cloudflare-delayed-fallback"=>{
                    return context.call("challenge",json!({"url":if query=="cloudflare-fallback" {"https://fixture.example/fallback-challenge"}else{"https://fixture.example/concurrent-challenge"}})).await;
                },
                "cloudflare-missing-url"=>return Err(json!({"name":"ChallengeRequiredError","message":"Browser verification required"})),
                "cloudflare-undeclared"=>return Err(json!({"name":"ChallengeRequiredError","message":"Browser verification required","url":"https://undeclared.example/cdn-cgi/challenge"})),
                "undeclared-request"=>{context.request("https://undeclared.example/data","GET",json!({}),None,false).await?;return Ok(card(json!("complete")));},
                _=>{}
            }
            if let Some(marker)=query.strip_prefix("isolation-cache:") {
                let value=context.remember("audit-shared-key",Some(60.0),async {Ok(json!({"marker":marker}))},|_|Ok(())).await?;
                return Ok(card(value["marker"].clone()));
            }
            if query.starts_with("http-") {
                let url=if query=="http-verified-session" {"https://fixture.example/reader/http-verified-session".to_owned()} else {format!("https://fixture.example/{query}")};
                let response=context.request(&url,"GET",if query=="http-verified-session" {json!({"User-Agent":"extension-user-agent"})}else{json!({})},None,false).await?;
                return Ok(card(json!(format!("status-{}",status(&response)))));
            }
            if api {return Ok(card(json!("Missing type")));}
            let mut result=card(json!(if query.is_empty() {"Fixture"}else{query}));
            result["items"][0]["mediaKind"]=json!("manga");
            if args["metadata"].is_null() {result["metadata"]=json!({"page":2,"opaque":"fixture"});}
            Ok(result)
        },
        "details"=>{
            let id=text(&args);
            if flavor=="ChapterIsolationVerification" {return Ok(json!({"workId":id,"workInfo":{"primaryTitle":if id=="title-a" {"Isolation Title A"}else{"Isolation Title B"},"secondaryTitles":[],"synopsis":"Original samples with the same remote chapter slug.","thumbnailUrl":format!("https://chapter-fixture.invalid/{id}/1.png"),"status":"ongoing"}}));}
            let mut title=if api {"Fixture Novel".to_owned()}else{"Fixture".to_owned()};
            if id=="image-cache-probe" {title=IMAGE_LOADS.with(|v|v.get()).to_string();}
            if let Some(operation)=id.strip_prefix("cache:") {
                if operation=="remove" {context.call("cache",json!({"operation":"remove","key":"fixture-gallery"})).await?;}
                let (a,b)=futures::try_join!(cache(&context,"fixture-gallery",false,false),cache(&context,"fixture-gallery",false,false))?;
                title=format!("Cache {}/{}",a["serial"],b["serial"]);
            }
            Ok(json!({"workId":if id=="mismatch-details" {"wrong-parent"}else{id},"workInfo":{
                "thumbnailUrl":if flavor=="ProtocolRelativeContentFixture" {"//fixture.example/cover.jpg"}else{"https://fixture.example/cover.jpg"},
                "synopsis":if api {"Synthetic novel"}else{"Synthetic fixture"},"primaryTitle":title,"secondaryTitles":["MR Fixture"],"contentRating":"SAFE","status":"ongoing","artist":"Fixture Artist","author":"Fixture Author","shareUrl":"https://fixture.example/work-1",
                "mediaKind":if api {"lightNovel"}else{"manga"},"searchFacets":[{"fieldID":"artist","value":"fixture-author","title":"Fixture Author","groupTitle":"Artists","presentation":"creator"}]}}))
        },
        "installments" if flavor=="ChapterIsolationVerification"=>Ok(json!((1..=2).map(|number|json!({"workId":args["workId"],"installmentId":format!("chapter-{number}"),"number":number,"title":format!("Chapter {number}"),"langCode":"en"})).collect::<Vec<_>>())),
        "installments"=>Ok(json!([{"workId":if args["workId"]=="mismatch-installments" {json!("wrong-parent")}else{args["workId"].clone()},"installmentId":"chapter-1","langCode":"en","number":1,"title":"Chapter 1","volume":1,"publishDate":"2026-01-01T00:00:00Z","format":if api {"xhtml"}else{"images"}}])),
        "imagePages"=>{
            let parent=text(&args["sourceWork"]["workId"]);
            if flavor=="ChapterIsolationVerification" {return Ok(json!({"id":args["installmentId"],"workId":parent,"pages":(1..=20).map(|number|format!("https://chapter-fixture.invalid/{parent}/{number}.png")).collect::<Vec<_>>()}));}
            let pages=if parent=="work-1" {vec!["https://fixture.example/1.jpg".into(),"https://fixture.example/2.jpg".into()]}else{vec![format!("https://fixture.example/{parent}/1.jpg"),format!("https://fixture.example/{parent}/2.jpg")]};
            Ok(json!({"id":args["installmentId"],"workId":if parent=="mismatch-pages" {"wrong-parent"}else{parent},"pages":pages}))
        },
        "imagePageContent"=>{
            let value=text(&args["url"]);
            if flavor=="ChapterIsolationVerification" {
                let url=url::Url::parse(value).map_err(|_|error("Invalid fixture image","invalidResponse"))?;
                let image=state(&context,"state","get",&format!("fixture-image:{}",url.path().trim_start_matches('/')),Value::Null).await?;
                let image=image.as_str().ok_or_else(||error("Unknown sample","notFound"))?;
                return Ok(json!({"dataBase64":image,"mimeType":"image/png"}));
            }
            if value.contains("wrong-mime") {return Ok(json!({"dataBase64":"AQI=","mimeType":"text/html"}));}
            if value.contains("invalid-base64") {return Ok(json!({"dataBase64":"%%%","mimeType":"image/png"}));}
            if value.contains("image-cache") {
                let url=url::Url::parse(value).unwrap();
                let resource=url.query_pairs().find(|(key,_)|key=="resource").map(|(_,v)|v.to_string()).unwrap_or("shared".into());
                let cached=cache(&context,&format!("image-gallery:{resource}"),true,value.contains("slow")).await?;
                return Ok(json!({"dataBase64":STANDARD.encode([cached["serial"].as_u64().unwrap() as u8]),"mimeType":"image/png"}));
            }
            if !api {return Ok(json!({"dataBase64":"AQI=","mimeType":"image/png"}));}
            let url=url::Url::parse(value).unwrap();
            if let Some((_,resource))=url.query_pairs().find(|(k,_)|k=="resourceID") {return Ok(json!({"resourceID":resource,"mimeType":"image/png"}));}
            if value.contains("isolation-capture") {
                let image=context.request(value,"GET",json!({}),None,true).await?;
                state(&context,"state","set","audit-resource",image["resourceID"].clone()).await?;
                context.request("https://fixture.example/http-cancellation","GET",json!({}),None,false).await?;
                return Ok(image);
            }
            if value.contains("/batch-image/") {
                ACTIVE_IMAGES.with(|v|v.set(v.get()+1));
                MAX_IMAGES.with(|v|v.set(v.get().max(ACTIVE_IMAGES.with(Cell::get))));
                context.call("sleep",json!({"milliseconds":0})).await?;
                ACTIVE_IMAGES.with(|v|v.set(v.get()-1));
                if value.ends_with("/3") {return Err(json!({"message":"Fixture image failure"}));}
                return Ok(json!({"dataBase64":STANDARD.encode(format!("{value}|{}",MAX_IMAGES.with(Cell::get))),"mimeType":"image/png"}));
            }
            context.request(value,"GET",json!({}),None,value.contains("native=1")).await
        },
        "publicationContent"=>Ok(json!({"id":args["installmentId"],"workId":args["workId"],"mimeType":"application/xhtml+xml; charset=utf-8","text":"<html><body onload='steal()'><h1>Chapter 1</h1><p>Hello <em>reader</em>.</p><script>steal()</script><form action='https://evil.example'><input /></form><iframe src='https://evil.example'></iframe><img src='https://evil.example/pixel.png' /></body></html>"})),
        "updates"=>Ok(json!({"items":[{"id":"update-1","remoteWorkID":"work-1","changedAt":null}],"nextCursor":null,"nextPage":null,"totalCount":1})),
        "managedCollections"=>Ok(json!({"items":[{"id":"favorites","title":"Favorites","workRemoteIDs":["work-1"],"modifiedAt":null}],"nextCursor":null,"nextPage":null,"totalCount":1})),
        "authentication"=>Ok(json!({"mode":"oauth2PKCE","authorizationURL":"https://fixture.example/oauth","tokenURL":"https://fixture.example/token","callbackScheme":"manko","callbackURL":"manko://oauth/fixture","clientID":"fixture-client","responseType":"code","scopes":["read","write"],"pkceMethod":"S256","form":null})),
        "trackerSearch"=>Ok(json!([{"id":"tracker-work","title":args["title"],"coverURL":"https://fixture.example/cover.jpg","mediaKind":"manga"}])),
        "progress"=>Ok(json!({"remoteWorkID":args["remoteWorkID"],"chapter":4,"volume":1,"status":"reading","score":8})),
        "progressBatch"=>Ok(json!(args["remoteWorkIDs"].as_array().unwrap().iter().map(|id|json!({"remoteWorkID":id,"chapter":4,"volume":1,"status":"reading","score":8})).collect::<Vec<_>>())),
        "collections"=>Ok(json!({"items":[{"id":"reading","title":"Reading","items":[{"id":"tracker-work","title":"Fixture","coverURL":null,"mediaKind":"manga"}]}],"nextCursor":null,"nextPage":null,"totalCount":1})),
        _=>Err(error("Unsupported operation","unsupportedOperation"))
    }
    })
}
manko_sdk::export_extension!(label(ID), label(KIND), run);
