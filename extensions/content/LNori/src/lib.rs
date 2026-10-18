// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
use manko_sdk::{
    content::{configuration, suggestions, validate_search},
    error, page, text, Context, Operation, Result,
};
use manko_site_support::{self as site, scraper::Html};
use serde_json::{json, Value};
use std::collections::HashSet;
const BASE: &str = "https://lnori.com/";
const HOSTS: &[&str] = &["lnori.com", "cdn.lnori.com", "img.lnori.com"];
const AGENT: &str = "manko LNori Extension/0.1.1 (+https://github.com/superdupercoolperson/extensions)";
fn identity(value: &str) -> Result<&str> {
    let parts: Vec<_> = value.split('/').collect();
    if parts.len() != 2 {
        return Err(error(
            "Invalid series or volume identifier",
            "invalidIdentifier",
        ));
    }
    manko_sdk::id(&json!(parts[0]))?;
    site::slug(parts[1])?;
    Ok(value)
}
fn path(value: &str, category: &str) -> Result<String> {
    let url = site::link(value, BASE, &["lnori.com"])?;
    let url = url::Url::parse(&url).map_err(|_| site::invalid())?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(site::invalid());
    }
    let id = url
        .path()
        .strip_prefix(&format!("/{category}/"))
        .ok_or_else(site::invalid)?;
    Ok(identity(id)?.to_owned())
}
fn filters() -> Value {
    let mut value = configuration(
        "lnori-search",
        "LNORI",
        &[("genre", "Genre"), ("author", "Author")],
        &[
            ("relevance", "Relevance"),
            ("title", "Title"),
            ("year", "Year Released"),
            ("volumes", "Volumes"),
        ],
        &["en"],
    );
    value["sortedBrowseSectionID"] = json!("library");
    value
}
fn hydrate(block: &site::Block<'_>) -> Result<Value> {
    let figure = site::blocks(block.body, "figure").next().ok_or_else(site::invalid)?;
    let href = site::tag_header(figure.body, "a")
        .and_then(|h| site::attribute(h, "href")).ok_or_else(site::invalid)?;
    let id = if let Some(id) = href.strip_prefix("/series/") {
        identity(id)?.to_owned()
    } else { path(&href, "series")? };
    if id.split('/').next() != site::attribute(block.header, "data-id").as_deref() {
        return Err(site::invalid());
    }
    let title = site::attribute(block.header, "data-t").filter(|v| !v.is_empty()).ok_or_else(site::invalid)?;
    let image = site::tag_header(figure.body, "img")
        .and_then(|h| site::attribute(h, "src")).ok_or_else(site::invalid)?;
    Ok(site::card(&id, &title, &site::link(&image, BASE, HOSTS)?, "lightNovel"))
}
async fn catalog(context: &Context, args: &Value) -> Result<Value> {
    let query = text(&args["query"]).trim().to_lowercase();
    if query.len() > 500 { return Err(error("Search query is too long", "invalidSearchTerm")); }
    let page = page(args, 1_000_000)?;
    let sort = args["sort"].as_str().unwrap_or("relevance");
    let field = match sort { "year" => "data-d", "volumes" => "data-v", _ => "data-rel" };
    let key = format!("lnori:page:v7:{}", json!({"query":query,"page":page,"sort":sort,"selections":args["selections"]}));
    let load = async {
        let html = site::document(context, &format!("{BASE}library"), BASE, HOSTS, AGENT).await?;
        let mut seen = HashSet::new();
        let mut rows = Vec::new();
        let mut found_catalog = false;
        for (index, block) in site::blocks(&html,"article").filter(|b| b.header.contains("class=\"card\"")).enumerate() {
            if index % 100 == 0 { context.now().await?; }
            found_catalog = true;
            let attr = |name| site::attribute(block.header, name).unwrap_or_default();
            if !seen.insert(attr("data-id")) { continue; }
            if !query.is_empty() && !attr("data-t").to_lowercase().contains(&query)
                && !attr("data-a").to_lowercase().contains(&query) { continue; }
            if !args["selections"].as_array().into_iter().flatten().all(|selection| {
                let value = text(&selection["value"]).to_lowercase();
                let matches = if selection["fieldID"] == "genre" {
                    attr("data-tags").split(',').any(|g| g.trim().eq_ignore_ascii_case(&value))
                } else { attr("data-a").to_lowercase().contains(&value) };
                matches != (selection["polarity"] == "exclude")
            }) { continue; }
            let title = if sort == "title" { attr("data-t").to_lowercase() } else { String::new() };
            let rank = attr(field).parse::<u64>().unwrap_or(0);
            rows.push((block, title, rank));
        }
        if !found_catalog { return Err(site::invalid()); }
        if sort == "title" { rows.sort_by(|a,b| a.1.cmp(&b.1)); }
        else { rows.sort_by(|a,b| b.2.cmp(&a.2)); }
        let start = (page as usize - 1) * 25;
        let next = if start + 25 < rows.len() { json!({"page":page+1}) } else { Value::Null };
        // Rank lightweight header references, then parse and cache one page.
        // Serializing all 884 cards exhausted the interpreted guest's budget.
        let items = rows.iter().skip(start).take(25).map(|row| hydrate(&row.0)).collect::<Result<Vec<_>>>()?;
        Ok(json!({"items":items,"metadata":next}))
    };
    if key.len() > 16_000 { return load.await; }
    context.remember(&key, Some(300.0), load, |value| {
        let items = value["items"].as_array().filter(|items| items.len() <= 25).ok_or_else(site::invalid)?;
        for item in items {
            identity(text(&item["workId"]))?;
            site::link(text(&item["imageUrl"]), BASE, HOSTS)?;
        }
        Ok(())
    }).await
}
async fn catalog_suggestions(context: &Context) -> Result<Value> {
    context.remember("lnori:suggestions:v1", Some(300.0), async {
        let html = site::document(context, &format!("{BASE}library"), BASE, HOSTS, AGENT).await?;
        let mut seen = HashSet::new();
        let mut values = Vec::new();
        let mut found_catalog = false;
        for (index, block) in site::blocks(&html,"article").filter(|b| b.header.contains("class=\"card\"")).enumerate() {
            if index % 100 == 0 { context.now().await?; }
            found_catalog = true;
            let author = site::attribute(block.header,"data-a").unwrap_or_default();
            if !author.is_empty() && seen.insert(("author",author.clone())) {
                values.push(json!({"fieldID":"author","value":author,"title":author}));
            }
            for genre in site::attribute(block.header,"data-tags").unwrap_or_default().split(',')
                .map(|v| v.trim().to_lowercase()).filter(|v| !v.is_empty()) {
                if seen.insert(("genre",genre.clone())) {
                    values.push(json!({"fieldID":"genre","value":genre,"title":genre}));
                }
            }
        }
        if !found_catalog { return Err(site::invalid()); }
        Ok(Value::Array(values))
    }, |value| {
        if value.as_array().is_none_or(|rows| rows.iter().any(|row|
            !matches!(row["fieldID"].as_str(),Some("author"|"genre")) || !row["value"].is_string() || !row["title"].is_string())) {
            return Err(site::invalid());
        }
        Ok(())
    }).await
}
// Cache compact cards, not the homepage DOM or the complete library.
async fn homepage(context: &Context) -> Result<Value> {
    context.remember("lnori:homepage:v1", Some(300.0), async {
        let html = site::document(context, BASE, BASE, HOSTS, AGENT).await?;
        let mut result = json!({});
        for block in site::blocks(&html, "section") {
            let id = site::attribute(block.header, "id").unwrap_or_default();
            let key = match id.as_str() {
                "hero-section" => "featured", "library" => "popular",
                "spring" | "summer" | "autumn" | "fall" | "winter" => "seasonal",
                _ => continue,
            };
            context.now().await?;
            let mut cards = Vec::new();
            if key == "featured" {
                // Carousel cards nest decorative divs. Read only opening tags;
                // constructing every decorative node is costly in iOS WASM.
                let mut remaining = block.body;
                while let Some(header) = site::tag_header(remaining, "div") {
                    remaining = remaining.split_once(header).ok_or_else(site::invalid)?.1;
                    let (Some(link), Some(title), Some(image)) = (
                        site::attribute(header, "data-link"),
                        site::attribute(header, "data-title"),
                        site::attribute(header, "data-image"),
                    ) else { continue; };
                    let id = path(&link, "series")?;
                    if title.is_empty() { return Err(site::invalid()); }
                    cards.push(site::card(&id, &title, &site::link(&image, BASE, HOSTS)?, "lightNovel"));
                }
            } else {
                for item in site::blocks(block.body, "a") {
                    let Some(link) = site::attribute(item.header, "href").filter(|v| v.starts_with("/series/")) else { continue; };
                    let Some(image) = site::tag_header(item.body, "img") else { continue; };
                    let Some(source) = site::attribute(image, "src") else { continue; };
                    let id = path(&link, "series")?;
                    let title = site::attribute(image, "alt").filter(|v| !v.is_empty())
                        .unwrap_or_else(|| site::field(&Html::parse_fragment(item.whole), "a"));
                    if title.is_empty() { return Err(site::invalid()); }
                    cards.push(site::card(&id, &title, &site::link(&source, BASE, HOSTS)?, "lightNovel"));
                }
            }
            result[key] = json!(site::unique(cards, "workId"));
        }
        if result.as_object().is_none_or(|v| v.is_empty()) { return Err(site::invalid()); }
        Ok(result)
    }, |value| {
        for cards in value.as_object().ok_or_else(site::invalid)?.values() {
            for card in cards.as_array().ok_or_else(site::invalid)? {
                identity(text(&card["workId"]))?;
                site::link(text(&card["imageUrl"]), BASE, HOSTS)?;
            }
        }
        Ok(())
    }).await
}

fn parse_series(html: &str, id: &str) -> Result<Value> {
    let schema = site::blocks(html, "script")
        .filter(|b| b.header.contains("application/ld+json"))
        .filter_map(|b| serde_json::from_str::<Value>(b.body).ok())
        .find(|v| v["hasPart"].is_array())
        .ok_or_else(site::invalid)?;
    if path(text(&schema["url"]), "series")? != id {
        return Err(site::invalid());
    }
    let title = text(&schema["name"]);
    if title.is_empty() {
        return Err(site::invalid());
    }
    // Current series metadata names the promotional volume card. The visible
    // /cover/ image is the actual book artwork used by catalog cards.
    // Keep the series path streaming: building a DOM for a large series page
    // is prohibitively expensive in the interpreted iOS WASM runtime.
    let mut remaining = html;
    let mut visible_cover = String::new();
    while let Some(header) = site::tag_header(remaining, "img") {
        let source = site::attribute(header, "src").unwrap_or_default();
        if source.contains("/cover/") {
            visible_cover = source;
            break;
        }
        remaining = remaining.split_once(header).ok_or_else(site::invalid)?.1;
    }
    let image = site::link(
        if visible_cover.is_empty() {
            text(&schema["image"])
        } else {
            &visible_cover
        },
        BASE,
        HOSTS,
    )?;
    let mut value = site::details(
        site::card(id, title, &image, "lightNovel"),
        text(&schema["description"]),
        text(&schema["author"]["name"]),
        "unknown",
        "MATURE",
        text(&schema["url"]),
    );
    let mut volumes = Vec::new();
    let mut seen = HashSet::new();
    for part in schema["hasPart"].as_array().unwrap() {
        let book = path(text(&part["url"]), "book")?;
        if !seen.insert(book.split('/').next().unwrap().to_owned()) {
            continue;
        }
        let number = part["position"]
            .as_f64()
            .or_else(|| text(&part["position"]).parse().ok())
            .filter(|n| n.is_finite() && *n > 0.0)
            .ok_or_else(site::invalid)?;
        volumes.push(json!({"installmentId":book,"workId":id,"title":part["name"],"number":number,"volume":number,"langCode":"en","format":"xhtml"}));
    }
    value["volumes"] = json!(volumes);
    Ok(value)
}
async fn series(context: &Context, id: &str) -> Result<Value> {
    identity(id)?;
    context
        .remember(
            &format!("lnori:series:{id}"),
            Some(1800.0),
            async {
                parse_series(
                    &site::document(context, &format!("{BASE}series/{id}"), BASE, HOSTS, AGENT)
                        .await?,
                    id,
                )
            },
            |v| {
                if v["workId"] != id || !v["volumes"].is_array() {
                    return Err(site::invalid());
                }
                site::link(text(&v["imageUrl"]), BASE, HOSTS)?;
                Ok(())
            },
        )
        .await
}
async fn parse_book(context: &Context, html: &str, book: &str, work: &str) -> Result<Value> {
    let schema = site::blocks(html, "script")
        .filter(|b| b.header.contains("application/ld+json"))
        .filter_map(|b| serde_json::from_str::<Value>(b.body).ok())
        .find(|v| v["additionalProperty"].is_array())
        .ok_or_else(site::invalid)?;
    let property = |name| {
        schema["additionalProperty"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == name)
            .map(|v| text(&v["value"]))
            .unwrap_or("")
    };
    if text(&schema["id"]) != book.split('/').next().unwrap()
        || format!("{}/{}", property("series_id"), property("series_slug")) != work
    {
        return Err(site::invalid());
    }
    let heading = site::blocks(html, "h1")
        .find(|b| b.header.contains("id=\"book-title\""))
        .ok_or_else(site::invalid)?;
    let title = site::field(&Html::parse_fragment(heading.whole), "h1");
    if title.is_empty() {
        return Err(site::invalid());
    }
    let article = site::blocks(html, "article")
        .find(|b| b.header.contains("class=\"content-body"))
        .ok_or_else(site::invalid)?;
    let mut output = format!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{}</title></head><body>",
        site::escape(&title)
    );
    if site::tag_header(article.body, "section").is_none() {
        return Err(site::invalid());
    }
    // Stream the complete reading body once. Volume markup contains nested
    // sections, which must not be scanned as separate duplicated chapters.
    output.push_str(
        &site::reading_fragment(context, article.body, &format!("{BASE}book/{book}"), HOSTS)
            .await?,
    );
    context.now().await?;
    output.push_str("</body></html>");
    if output.len() > 2 * 1024 * 1024 {
        return Err(error(
            "Volume text exceeds the app's supported size",
            "invalidResponse",
        ));
    }
    Ok(json!({"id":book,"workId":work,"mimeType":"application/xhtml+xml","text":output}))
}
fn run(context: Context, method: String, args: Value) -> Operation {
    Box::pin(async move {
        match method.as_str() {
            "initialize" | "invalidateCache" => Ok(Value::Null),
            "settings" => Ok(json!({"id":"settings","title":"LNORI","fields":[]})),
            "discoverSections" => Ok(json!([
                {"id":"featured","title":"Featured","type":0,"presentation":"carousel"},
                {"id":"seasonal","title":"Seasonal Preview","type":0,"presentation":"carousel"},
                {"id":"popular","title":"Popular Novels","type":0,"presentation":"carousel"},
                {"id":"library","title":"Library","type":0,"presentation":"latestGallery"}
            ])),
            "searchFilters" => Ok(filters()),
            "searchSuggestions" => Ok(suggestions(&args, catalog_suggestions(&context).await?.as_array().unwrap().clone())),
            "search" | "discover" => {
                validate_search(&filters(), &args)?;
                let page = page(&args, 1_000_000)?;
                let section = args["sectionId"].as_str().or(args["section"]["id"].as_str()).unwrap_or("library");
                if method == "discover" && section != "library" {
                    if !matches!(section, "featured" | "seasonal" | "popular") {
                        return Err(error("Unknown discovery section", "invalidSection"));
                    }
                    let data = homepage(&context).await?;
                    let items = data[section].as_array().ok_or_else(site::invalid)?;
                    let start = (page as usize - 1) * 25;
                    return Ok(json!({"items":items.iter().skip(start).take(25).collect::<Vec<_>>(),
                        "metadata":if start + 25 < items.len() { json!({"page":page+1}) } else { Value::Null }}));
                }
                catalog(&context, &args).await
            }
            "details" => {
                let mut value = series(&context, text(&args)).await?;
                value.as_object_mut().unwrap().remove("volumes");
                Ok(value)
            }
            "installments" => Ok(series(&context, text(&args["workId"])).await?["volumes"].clone()),
            "publicationStream" => {
                let book = identity(text(&args["installmentId"]))?;
                let work = identity(text(&args["workId"]))?;
                // Native incremental parsing validates the book's structured
                // identity before delivering any text. Do not fetch the volume here.
                Ok(
                    json!({"id":book,"workId":work,"url":format!("{BASE}book/{book}"),
                    "headers":{"Referer":BASE,"User-Agent":AGENT,"Accept":"text/html"}}),
                )
            }
            "publicationContent" => {
                let book = identity(text(&args["installmentId"]))?;
                let work = identity(text(&args["workId"]))?;
                if !series(&context, work).await?["volumes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["installmentId"] == book)
                {
                    return Err(error("Unknown volume for this series", "invalidIdentifier"));
                }
                context
                    .remember(
                        &format!("lnori:book:{book}"),
                        Some(1800.0),
                        async {
                            parse_book(
                                &context,
                                &site::document(
                                    &context,
                                    &format!("{BASE}book/{book}"),
                                    BASE,
                                    HOSTS,
                                    AGENT,
                                )
                                .await?,
                                book,
                                work,
                            )
                            .await
                        },
                        |v| {
                            if v["id"] == book
                                && v["workId"] == work
                                && v["mimeType"] == "application/xhtml+xml"
                                && v["text"].is_string()
                            {
                                Ok(())
                            } else {
                                Err(site::invalid())
                            }
                        },
                    )
                    .await
            }
            "imagePageContent" => {
                site::image(
                    &context,
                    text(&args["url"]),
                    BASE,
                    &["cdn.lnori.com", "img.lnori.com"],
                    AGENT,
                )
                .await
            }
            _ => Err(error("Unsupported operation", "unsupportedOperation")),
        }
    })
}
manko_sdk::export_extension!("LNori", "content", run);
