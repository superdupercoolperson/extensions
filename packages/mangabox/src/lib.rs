// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
// Independent Manko implementation of the publicly documented MangaBox layouts.
use manko_sdk::{content, error, page, text, Context, Operation, Result};
use manko_site_support::{self as site, scraper::Html};
use serde_json::{json, Value};
use std::{cell::RefCell, collections::{HashMap, HashSet, VecDeque}, sync::LazyLock};

#[derive(Clone, Copy)]
pub struct Provider {
    pub name: &'static str,
    pub base: &'static str,
}
const AGENT: &str = "manko MangaBox Extension/0.1.0";
const SITE_HOSTS: &[&str] = &[
    "www.mangakakalot.gg",
    "mangakakalot.gg",
    "www.manganato.gg",
    "manganato.gg",
    "www.mangabats.com",
    "mangabats.com",
];
const MEDIA_SUFFIXES: &[&str] = &[
    ".mkklcdnv6temp.com",
    ".mkklcdnv5temp.com",
    ".mkklcdnv4temp.com",
    ".mkklcdnv3temp.com",
];
const IMAGE_MIRRORS: &[&str] = &["img-r1.2xstorage.com", "img-r2.2xstorage.com", "imgs-2.2xstorage.com"];
const MAX_IMAGE_BASES: usize = 16;
const MAX_REMEMBERED_PAGES: usize = 5000;
static PAGE_ARRAYS: LazyLock<regex::Regex> = LazyLock::new(||
    regex::Regex::new(r"(?s)\b(cdns|backupImage|chapterImages)\s*=\s*(\[[^;]*?\])").unwrap());
#[derive(Default)]
struct PageAlternatives {
    pages: HashMap<String, Vec<String>>,
    order: VecDeque<String>,
}
thread_local! {
    static PAGE_ALTERNATIVES: RefCell<PageAlternatives> = RefCell::new(PageAlternatives::default());
}
fn remember_alternatives(primary: &str, alternatives: Vec<String>) {
    PAGE_ALTERNATIVES.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.pages.contains_key(primary) { cache.order.push_back(primary.to_owned()); }
        cache.pages.insert(primary.to_owned(), alternatives);
        while cache.pages.len() > MAX_REMEMBERED_PAGES {
            if let Some(old) = cache.order.pop_front() { cache.pages.remove(&old); }
        }
    });
}
fn can_try_image_alternative(error: &Value) -> bool {
    matches!(text(&error["type"]), "notFound" | "serviceError" | "invalidResponse")
        || (error["code"] == "nativeRequestFailed" && error["name"] == "URLError")
}
async fn image_with_fallback(context: &Context, provider: Provider, requested: &str) -> Result<Value> {
    let primary = media(requested, provider.base)?;
    let mut candidates = vec![primary.clone()];
    candidates.extend(PAGE_ALTERNATIVES.with(|cache| cache.borrow().pages.get(&primary).cloned().unwrap_or_default()));
    let parsed = url::Url::parse(&primary).map_err(|_| site::invalid())?;
    if IMAGE_MIRRORS.contains(&parsed.host_str().unwrap_or_default()) {
        for host in IMAGE_MIRRORS {
            let mut mirror = parsed.clone();
            mirror.set_host(Some(host)).map_err(|_| site::invalid())?;
            candidates.push(media(mirror.as_str(), provider.base)?);
        }
    }
    let mut seen = HashSet::new();
    let mut failure = site::invalid();
    for candidate in candidates.into_iter().filter(|value| seen.insert(value.clone())).take(MAX_IMAGE_BASES + IMAGE_MIRRORS.len()) {
        let candidate = media(&candidate, provider.base)?;
        let parsed = url::Url::parse(&candidate).map_err(|_| site::invalid())?;
        match site::image(context, &candidate, provider.base, &[parsed.host_str().unwrap()], AGENT).await {
            Ok(image) => return Ok(image),
            Err(error) => {
                // Only transport/content failures can use another CDN. Preserve
                // verification, rate limits, authorization and cancellation.
                if !can_try_image_alternative(&error) { return Err(error); }
                failure = error;
            }
        }
    }
    Err(failure)
}

#[cfg(test)]
mod image_fallback_tests {
    use super::*;

    #[test]
    fn native_transport_errors_retry_but_cancellation_and_policy_errors_do_not() {
        assert!(can_try_image_alternative(&json!({"code":"nativeRequestFailed","name":"URLError"})));
        for value in [
            json!({"code":"nativeRequestFailed","name":"CancellationError"}),
            json!({"code":"nativeRequestFailed","name":"SourceError"}),
            json!({"code":"cloudflareHandoff","name":"MankoExtensionRuntimeError"}),
            json!({"type":"rateLimited"}),
            json!({"type":"authenticationRequired"}),
        ] { assert!(!can_try_image_alternative(&value)); }
    }
}
fn media(value: &str, base: &str) -> Result<String> {
    let url = url::Url::parse(base)
        .unwrap()
        .join(value)
        .map_err(|_| site::invalid())?;
    let host = url.host_str().ok_or_else(site::invalid)?;
    if !["img-r1.2xstorage.com", "img-r2.2xstorage.com", "imgs-2.2xstorage.com", "storage.waitst.com", "storage4.waitst.com"].contains(&host) && !SITE_HOSTS.contains(&host) && !MEDIA_SUFFIXES.iter().any(|suffix| host.ends_with(suffix)) {
        return Err(error("Unexpected MangaBox image host", "hostNotAllowed"));
    }
    site::link(url.as_str(), base, &[host])
}
fn path(value: &str, provider: Provider) -> Result<String> {
    let host = url::Url::parse(provider.base).unwrap();
    let link = site::link(value, provider.base, &[host.host_str().unwrap()])?;
    let url = url::Url::parse(&link).unwrap();
    if url.query().is_some() || url.fragment().is_some() {
        return Err(site::invalid());
    }
    let path = url.path().trim_matches('/');
    if path.is_empty() || path.len() > 512 {
        return Err(site::invalid());
    }
    for part in path.split('/') {
        site::slug(part)?;
    }
    Ok(path.to_owned())
}
fn work_url(id: &str, provider: Provider) -> Result<String> {
    let p = path(&format!("/{}", id), provider)?;
    if p != id {
        return Err(error("Invalid series identifier", "invalidIdentifier"));
    }
    Ok(format!("{}{}", provider.base, id))
}
fn config(provider: Provider) -> Value {
    let mut value = content::configuration(
        "mangabox-search",
        provider.name,
        &[
            ("genre", "Genre"),
            ("status", "Status"),
            ("author", "Author"),
        ],
        &[
            ("new", "Newest"),
            ("latest", "Latest updates"),
            ("hot", "Most read"),
        ],
        &["en"],
    );
    for field in value["fields"].as_array_mut().unwrap() {
        field["supportsExclusion"] = json!(false);
        field["maximumSelections"] = json!(1);
        if field["id"] == "status" {
            field["inputKind"] = json!("choice");
            field["options"] =
                json!([{"id":"ongoing","title":"Ongoing"},{"id":"completed","title":"Completed"}]);
        } else {
            field["inputKind"] = json!("text");
        }
    }
    value
}
fn encode(value: &str, separator: char) -> String {
    let value: String = value
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() {
                Some(c.to_ascii_lowercase())
            } else if matches!(c, ' ' | '-' | '_' | '\'' | '’') {
                Some(separator)
            } else {
                None
            }
        })
        .collect();
    content::encode_segment(&value)
}
fn catalog_url(provider: Provider, args: &Value, discovery: bool, page: u64) -> Result<String> {
    content::validate_search(&config(provider), args)?;
    let query = text(&args["query"]).trim();
    if query.len() > 500 {
        return Err(error("Search query is too long", "invalidSearchTerm"));
    }
    if !query.is_empty() {
        if args["selections"].as_array().is_some_and(|s| !s.is_empty()) {
            return Err(error(
                "MangaBox cannot combine title search and filters",
                "invalidSearchTerm",
            ));
        }
        return Ok(format!(
            "{}search/story/{}?page={page}",
            provider.base,
            encode(query, '_')
        ));
    }
    let section = args["sectionId"]
        .as_str()
        .or(args["section"]["id"].as_str())
        .unwrap_or("new");
    let order = if discovery {
        section
    } else {
        args["sort"].as_str().unwrap_or("new")
    };
    let sort = match order {
        "new" => 1,
        "latest" | "completed" => 4,
        "hot" => 7,
        _ => return Err(error("Unknown listing", "invalidSection")),
    };
    let mut status = if section == "completed" && discovery {
        1
    } else {
        0
    };
    let mut genre = "all".to_owned();
    let mut author = None;
    for selection in args["selections"].as_array().into_iter().flatten() {
        let value = text(&selection["value"]);
        match text(&selection["fieldID"]) {
            "genre" => genre = encode(value, '-'),
            "author" => author = Some(encode(value, '-')),
            "status" => {
                status = match value {
                    "completed" => 1,
                    "ongoing" => 2,
                    _ => return Err(error("Unknown status", "invalidSearchTerm")),
                }
            }
            _ => return Err(site::invalid()),
        }
    }
    if let Some(author) = author {
        return Ok(format!("{}author/{author}?page={page}", provider.base));
    }
    Ok(format!(
        "{}genre/{genre}?filter={}&page={page}",
        provider.base,
        sort + status
    ))
}
async fn catalog(context: &Context, body: &str, provider: Provider, current: u64) -> Result<Value> {
    // Scan only cards and pagination anchors. Building a DOM for navigation,
    // long summaries and advertisements exhausts the interpreted runtime budget.
    let classes = ["story_item", "list-truyen-item-wrap", "list-comic-item-wrap", "content-genres-item", "search-story-item"];
    let has_container = site::blocks(body, "div").any(|block| site::attribute(block.header, "class").is_some_and(|value|
        value.split_whitespace().any(|name| classes.contains(&name) || ["panel_story_list", "truyen-list", "list-comic", "panel-content-genres", "panel-search-story"].contains(&name))));
    context.now().await?;
    let mut items = Vec::new();
    for row in site::blocks(body, "div").filter(|block| site::attribute(block.header, "class").is_some_and(|value|
        value.split_whitespace().any(|name| classes.contains(&name)))) {
        context.now().await?;
        let Some(anchor) = site::blocks(row.body, "a").next() else { continue };
        let id = path(&site::attribute(anchor.header, "href").ok_or_else(site::invalid)?, provider)?;
        let title = site::attribute(anchor.header, "title").filter(|t| !t.trim().is_empty()).unwrap_or_else(|| {
            let heading = site::blocks(row.body, "h3").next().or_else(|| site::blocks(row.body, "h2").next());
            let fragment = Html::parse_fragment(heading.map(|h| h.body).unwrap_or(anchor.body));
            fragment.root_element().text().collect::<String>().trim().to_owned()
        });
        if title.trim().is_empty() { return Err(site::invalid()); }
        let cover = site::tag_header(row.body, "img").and_then(|header| site::attribute(header, "data-src").or_else(|| site::attribute(header, "src"))).unwrap_or_else(|| {
            let fragment = Html::parse_fragment(row.body);
            site::attr(&fragment, "img", "data-src")
        });
        let cover = if cover.is_empty() { site::attr(&Html::parse_fragment(row.body), "img", "src") } else { cover };
        items.push(site::card(&id, &title, &if cover.is_empty() { String::new() } else { media(&cover, provider.base)? }, "manga"));
    }
    if !has_container && items.is_empty() {
        return Err(site::invalid());
    }
    context.now().await?;
    let next = site::blocks(body, "a").any(|anchor| {
            let Some(href) = site::attribute(anchor.header, "href") else {
                return false;
            };
            let Ok(url) = url::Url::parse(provider.base).unwrap().join(&href) else {
                return false;
            };
            url.host_str() == url::Url::parse(provider.base).unwrap().host_str()
                && url.query_pairs().any(|(key, value)| {
                    key == "page" && value.parse::<u64>().is_ok_and(|p| p > current)
                })
        });
    Ok(
        json!({"items":site::unique(items,"workId"),"metadata":if next {json!({"page":current+1})}else{Value::Null}}),
    )
}
async fn document(context: &Context, provider: Provider, url: &str) -> Result<String> {
    let parsed = url::Url::parse(provider.base).unwrap();
    site::document(
        context,
        url,
        provider.base,
        &[parsed.host_str().unwrap()],
        AGENT,
    )
    .await
}
fn details(body: &str, provider: Provider, id: &str) -> Result<Value> {
    let region = body.split("id=\"chapter-list-container\"").next().unwrap_or(body);
    let plain = |value: &str| Html::parse_fragment(value).root_element().text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let title = site::blocks(region, "h1").next().map(|block| plain(block.body)).unwrap_or_default();
    if title.is_empty() { return Err(site::invalid()); }
    let cover = site::blocks(region, "div").find(|block| site::attribute(block.header, "class").is_some_and(|value| value.split_whitespace().any(|name| name == "manga-info-pic" || name == "info-image")))
        .or_else(|| site::blocks(region, "span").find(|block| site::attribute(block.header, "class").is_some_and(|value| value.split_whitespace().any(|name| name == "info-image"))));
    let image = cover.and_then(|block| site::tag_header(block.body, "img")).and_then(|header| site::attribute(header, "src")).unwrap_or_default();
    let mut author = String::new();
    let mut status = "unknown";
    let mut tags = Vec::new();
    for row in site::blocks(region, "li").chain(site::blocks(region, "tr")) {
        let label = plain(row.body);
        let lower = label.to_lowercase();
        if lower.starts_with("author") {
            author = site::blocks(row.body, "a").map(|a| plain(a.body)).collect::<Vec<_>>().join(", ");
            if author.is_empty() { author = label.split_once(':').map(|(_, value)| value.trim().to_owned()).unwrap_or_default(); }
        }
        if lower.starts_with("genre") { tags = site::blocks(row.body, "a").map(|a| plain(a.body)).collect(); }
        if lower.starts_with("status") { status = if lower.contains("ongoing") { "ongoing" } else if lower.contains("completed") { "completed" } else if lower.contains("hiatus") { "hiatus" } else { "unknown" }; }
    }
    let summary = site::blocks(body, "div").find(|block| site::attribute(block.header, "id").is_some_and(|id| id == "contentBox" || id == "panel-story-info-description")).map(|block| plain(&plain(block.body))).unwrap_or_default();
    let mut value = site::details(
        site::card(
            id,
            &title,
            &if image.is_empty() {
                String::new()
            } else {
                media(&image, provider.base)?
            },
            "manga",
        ),
        &summary,
        &author,
        status,
        if tags
            .iter()
            .any(|t: &String| matches!(t.to_lowercase().as_str(), "adult" | "mature" | "smut"))
        {
            "ADULT"
        } else if tags.iter().any(|t| t.eq_ignore_ascii_case("ecchi")) {
            "MATURE"
        } else {
            "SAFE"
        },
        &work_url(id, provider)?,
    );
    value["workInfo"]["searchFacets"] = json!(tags.iter().map(|tag| json!({"fieldID":"genre","value":encode(tag, '-'),"title":tag,"groupTitle":"Genres","presentation":"tag"})).collect::<Vec<_>>());
    Ok(value)
}
async fn chapters(context: &Context, provider: Provider, id: &str, body: &str) -> Result<Value> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    if let Some(index) = site::blocks(body, "div").find(|block| site::attribute(block.header, "id").as_deref() == Some("chapter-list-container"))
    {
        let slug = site::attribute(index.header, "data-comic-slug")
            .ok_or_else(site::invalid)?
            .to_owned();
        site::slug(&slug)?;
        let api = site::attribute(index.header, "data-api-url")
            .ok_or_else(site::invalid)?
            .replace("__SLUG__", &slug);
        let template = site::attribute(index.header, "data-chapter-url-template")
            .ok_or_else(site::invalid)?
            .replace("__MANGA__", &slug);
        let host = url::Url::parse(provider.base).unwrap();
        let api = site::link(&api, provider.base, &[host.host_str().unwrap()])?;
        let mut offset = 0;
        loop {
            if offset > 100_000 {
                return Err(error("Chapter index is too large", "invalidResponse"));
            }
            let mut url = url::Url::parse(&api).unwrap();
            url.query_pairs_mut()
                .append_pair("limit", "500")
                .append_pair("offset", &offset.to_string());
            let data: Value =
                serde_json::from_str(&document(context, provider, url.as_str()).await?)
                    .map_err(|_| site::invalid())?;
            let rows = data["data"]["chapters"]
                .as_array()
                .ok_or_else(site::invalid)?;
            let more = data["data"]["pagination"]["has_more"]
                .as_bool()
                .ok_or_else(site::invalid)?;
            context.now().await?;
            let before = items.len();
            for (position, row) in rows.iter().enumerate() {
                if position % 25 == 0 { context.now().await?; }
                let slug = site::slug(text(&row["chapter_slug"]))?;
                let chapter = path(&template.replace("__CHAPTER__", slug), provider)?;
                if seen.insert(chapter.clone()) {
                    let number = row["chapter_num"]
                        .as_f64()
                        .filter(|n| n.is_finite() && *n >= 0.0)
                        .ok_or_else(site::invalid)?;
                    items.push(json!({"installmentId":format!("{id}|{chapter}"),"title":row["chapter_name"],"number":number,"langCode":"en"}));
                }
            }
            if !more {
                break;
            }
            if rows.is_empty() || items.len() == before {
                return Err(error(
                    "Chapter pagination made no progress",
                    "invalidResponse",
                ));
            }
            offset += rows.len();
            context.now().await?;
        }
    } else {
        let html = Html::parse_document(body);
        let re = regex::Regex::new(r"(?i)chapter\s*([0-9]+(?:\.[0-9]+)?)").unwrap();
        for anchor in html.select(&site::selector(".row-content-chapter a, .chapter-list a")) {
            let chapter = path(
                anchor.value().attr("href").ok_or_else(site::invalid)?,
                provider,
            )?;
            if seen.insert(chapter.clone()) {
                let title = site::plain(anchor);
                let number = re.captures(&title).and_then(|c| c[1].parse::<f64>().ok());
                items.push(json!({"installmentId":format!("{id}|{chapter}"),"title":title,"number":number,"langCode":"en"}));
            }
            if items.len() % 50 == 0 {
                context.now().await?;
            }
        }
        if items.is_empty() {
            return Err(error("Missing chapter index", "invalidResponse"));
        }
    }
    for item in &mut items { item["workId"] = json!(id); }
    Ok(json!(items))
}
fn pages(body: &str, provider: Provider) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut arrays = std::collections::HashMap::new();
    for script in site::blocks(body, "script") {
        for values in PAGE_ARRAYS.captures_iter(script.body) {
            let parsed: Vec<String> =
                serde_json::from_str(&values[2]).map_err(|_| site::invalid())?;
            arrays.insert(values[1].to_owned(), parsed);
        }
    }
    if let Some(images) = arrays.get("chapterImages") {
        let bases: Vec<_> = arrays.get("cdns").into_iter().flatten()
            .chain(arrays.get("backupImage").into_iter().flatten()).collect();
        if bases.is_empty() || bases.len() > MAX_IMAGE_BASES || images.len() > MAX_REMEMBERED_PAGES { return Err(site::invalid()); }
        // Validate every advertised destination before remembering it.
        let bases = bases.into_iter().map(|base| {
            let base = media(base, provider.base)?;
            let mut parsed = url::Url::parse(&base).map_err(|_| site::invalid())?;
            if parsed.query().is_some() || parsed.fragment().is_some() { return Err(site::invalid()); }
            if !parsed.path().ends_with('/') { parsed.set_path(&format!("{}/", parsed.path())); }
            Ok(parsed)
        }).collect::<Result<Vec<_>>>()?;
        for image in images {
            if image.is_empty() || image.len() > 2048 || image.contains('\\') || image.chars().any(char::is_control) { return Err(site::invalid()); }
            let mut candidates = Vec::new();
            let relative = if image.starts_with("https://") || image.starts_with("//") {
                let absolute = media(image, provider.base)?;
                let parsed = url::Url::parse(&absolute).map_err(|_| site::invalid())?;
                let path = format!("{}{}", parsed.path().trim_start_matches('/'), parsed.query().map(|q| format!("?{q}")).unwrap_or_default());
                candidates.push(absolute);
                path
            } else { image.trim_start_matches('/').to_owned() };
            for base in &bases {
                let candidate = media(&relative, base.as_str())?;
                if !candidates.contains(&candidate) { candidates.push(candidate); }
            }
            let primary = candidates.remove(0);
            remember_alternatives(&primary, candidates);
            result.push(primary);
        }
    } else {
        // Parse only the reader fragment; navigation, chapter menus and comments
        // can otherwise consume the native WASM dispatch budget.
        let reader = site::blocks(body, "div").find(|block| site::attribute(block.header, "class")
            .is_some_and(|value| value.split_whitespace().any(|class| class == "container-chapter-reader")))
            .ok_or_else(site::invalid)?;
        let html = Html::parse_fragment(reader.whole);
        for image in html.select(&site::selector(".container-chapter-reader > img")) {
            result.push(media(
                image
                    .value()
                    .attr("data-src")
                    .or(image.value().attr("src"))
                    .ok_or_else(site::invalid)?,
                provider.base,
            )?);
        }
    }
    if result.is_empty() || result.len() > 5000 {
        return Err(error(
            "Missing or oversized chapter pages",
            "invalidResponse",
        ));
    }
    Ok(result)
}
// Details and installments share the series document, avoiding another paced
// request when a title is opened. Short-lived public metadata only.
async fn series_document(context: &Context, provider: Provider, id: &str) -> Result<String> {
    let url = work_url(id, provider)?;
    let value = context.remember(&format!("mangabox:series:{url}"), Some(120.0), async {
        let body = document(context, provider, &url).await?;
        details(&body, provider, id)?;
        Ok(json!(body))
    }, |value| {
        if !value.is_string() { return Err(site::invalid()); }
        Ok(())
    }).await?;
    Ok(text(&value).to_owned())
}

pub fn run(provider: Provider, context: Context, method: String, args: Value) -> Operation {
    Box::pin(async move {
        match method.as_str() {
            "initialize" | "invalidateCache" => Ok(Value::Null),
            "settings" => Ok(json!({"id":"settings","title":provider.name,"fields":[]})),
            "discoverSections" => Ok(
                json!([{"id":"hot","title":"Popular Manga","type":0},{"id":"latest","title":"Latest Updates","type":0},{"id":"new","title":"New Manga","type":0},{"id":"completed","title":"Completed","type":0}]),
            ),
            "searchFilters" => Ok(config(provider)),
            "searchSuggestions" => Ok(json!([])),
            "search" | "discover" => {
                let page = page(&args, 100_000)?;
                let url = catalog_url(provider, &args, method == "discover", page)?;
                catalog(&context, &document(&context, provider, &url).await?, provider, page).await
            }
            "details" => {
                let id = text(&args);
                let body = series_document(&context, provider, id).await?;
                details(&body, provider, id)
            }
            "installments" => {
                let id = text(&args["workId"]);
                let body = series_document(&context, provider, id).await?;
                chapters(&context, provider, id, &body).await
            }
            "imagePages" => {
                let id = text(&args["installmentId"]);
                let (work, chapter) = id.split_once('|').ok_or_else(site::invalid)?;
                work_url(work, provider)?;
                if args["workId"] != work {
                    return Err(error(
                        "Chapter belongs to another series",
                        "invalidIdentifier",
                    ));
                }
                let url = work_url(chapter, provider)?;
                Ok(
                    json!({"id":id,"workId":work,"pages":pages(&document(&context,provider,&url).await?,provider)?}),
                )
            }
            "imagePageContent" => {
                image_with_fallback(&context, provider, text(&args["url"])).await
            }
            _ => Err(error("Unsupported operation", "unsupportedOperation")),
        }
    })
}
