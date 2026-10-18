// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
use manko_sdk::{error, page, text, Context, Operation, Result};
use manko_site_support::{self as site, scraper::Html};
use serde_json::{json, Value};
const BASE: &str = "https://fanfox.net/";
const MOBILE: &str = "https://m.fanfox.net/";
const MEDIA: &[&str] = &["fmcdn.mfcdn.net", "zjcdn.mangafox.me", "c.mfcdn.net"];
const HOSTS: &[&str] = &[
    "fanfox.net",
    "m.fanfox.net",
    "fmcdn.mfcdn.net",
    "zjcdn.mangafox.me",
    "c.mfcdn.net",
];
const AGENT: &str = "manko MangaFox Extension/0.1.0 (+https://github.com/superdupercoolperson/extensions)";
fn installment(value: &str) -> Result<(&str, f64, Option<f64>)> {
    let parts: Vec<_> = value.split('/').collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(error("Invalid chapter identifier", "invalidIdentifier"));
    }
    for part in &parts {
        site::slug(part)?;
    }
    let chapter = parts
        .last()
        .unwrap()
        .strip_prefix('c')
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or_else(|| error("Invalid chapter identifier", "invalidIdentifier"))?;
    let volume = if parts.len() == 3 {
        if !parts[1].starts_with('v') {
            return Err(error("Invalid volume identifier", "invalidIdentifier"));
        }
        parts[1][1..]
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n >= 0.0)
    } else {
        None
    };
    Ok((parts[0], chapter, volume))
}
fn reader(value: &str) -> Result<(String, u64)> {
    let url = site::link(value, MOBILE, &["m.fanfox.net"])?;
    let url = url::Url::parse(&url).unwrap();
    if url.query().is_some() || url.fragment().is_some() {
        return Err(site::invalid());
    }
    let (id, page) = url
        .path()
        .strip_prefix("/manga/")
        .and_then(|p| p.rsplit_once('/'))
        .ok_or_else(site::invalid)?;
    installment(id)?;
    let number = page
        .strip_suffix(".html")
        .and_then(|p| p.parse::<u64>().ok())
        .filter(|p| *p > 0 && *p <= 5000 && page == format!("{p}.html"))
        .ok_or_else(site::invalid)?;
    Ok((id.to_owned(), number))
}
fn work_path(value: &str) -> Result<String> {
    let url = site::link(value, BASE, &["fanfox.net"])?;
    let url = url::Url::parse(&url).unwrap();
    if url.query().is_some() || url.fragment().is_some() {
        return Err(site::invalid());
    }
    Ok(site::slug(
        url.path()
            .strip_prefix("/manga/")
            .and_then(|p| p.strip_suffix('/'))
            .ok_or_else(site::invalid)?,
    )?
    .to_owned())
}
async fn catalog(context: &Context, html: &str, page: u64, search: bool) -> Result<Value> {
    let class = if search {
        "manga-list-4-list"
    } else {
        "manga-list-1-list"
    };
    let list = site::blocks(html, "ul")
        .find(|b| b.header.contains(class))
        .ok_or_else(site::invalid)?;
    let mut items = Vec::new();
    for (index, row) in site::blocks(list.body, "li").enumerate() {
        if index % 25 == 0 {
            context.now().await?;
        }
        let fragment = site::blocks(row.body, "a")
            .find(|b| b.header.contains("href=\"/manga/"))
            .ok_or_else(site::invalid)?;
        let document = Html::parse_fragment(fragment.whole);
        let anchor = document
            .select(&site::selector("a"))
            .next()
            .ok_or_else(site::invalid)?;
        let id = work_path(anchor.value().attr("href").ok_or_else(site::invalid)?)?;
        let title = anchor
            .value()
            .attr("title")
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| site::plain(anchor));
        if title.is_empty() {
            return Err(site::invalid());
        }
        let image = site::attr(&document, "img", "src");
        items.push(site::card(
            &id,
            &title,
            &site::link(&image, BASE, MEDIA)?,
            "manga",
        ));
    }
    let pager = site::blocks(html, "div").find(|b| b.header.contains("class=\"pager-list-left\""));
    let pager = Html::parse_fragment(pager.map(|b| b.body).unwrap_or(""));
    let next = pager.select(&site::selector("a[href]")).any(|e| {
        let Some(href) = e.value().attr("href") else {
            return false;
        };
        let Ok(value) = site::link(href, BASE, &["fanfox.net"]) else {
            return false;
        };
        let url = url::Url::parse(&value).unwrap();
        if search {
            url.path() == "/search"
                && url
                    .query_pairs()
                    .any(|(k, v)| k == "page" && v.parse::<u64>().ok() == Some(page + 1))
        } else {
            url.path().starts_with("/directory/") && url.path().ends_with(&format!("/{}.html", page + 1))
        }
    });
    Ok(
        json!({"items":site::unique(items,"workId"),"metadata":if next{json!({"page":page+1})}else{Value::Null}}),
    )
}
async fn homepage(context: &Context) -> Result<Value> {
    context.remember("mangafox:homepage:v1", Some(300.0), async {
        let html = site::document(context, BASE, BASE, HOSTS, AGENT).await?;
        let headings = regex::Regex::new(r#"<div class="manga-list-[1-4]-title">"#).unwrap();
        let starts: Vec<_> = headings.find_iter(&html).map(|m| (m.start(), m.end())).collect();
        let mut result = json!({});
        for (index, (start, content)) in starts.iter().enumerate() {
            let title = html[*content..].split('<').next().unwrap_or("").trim();
            let key = match title {
                "Hot Manga Releases" => "hot", "Being Read Right Now" => "reading",
                "Popular Manga Ranking" => "ranking", "Recommended" => "recommended",
                "New Manga Release" => "new", "Lastest Updates" | "Latest Updates" => "latest",
                "Trending Manga" => "trending", _ => continue,
            };
            let end = starts.get(index + 1).map(|v| v.0).unwrap_or(html.len());
            context.now().await?;
            let document = Html::parse_fragment(&html[*start..end]);
            let mut cards = Vec::new();
            for anchor in document.select(&site::selector("a[href^='/manga/']")) {
                let Ok(id) = work_path(anchor.value().attr("href").unwrap_or("")) else { continue; };
                let title = anchor.value().attr("title").filter(|v| !v.is_empty()).map(str::to_owned).unwrap_or_else(|| site::plain(anchor));
                if title.is_empty() { continue; }
                let image = anchor.select(&site::selector("img[src]")).next().and_then(|v| v.value().attr("src"));
                let mut card = site::card(&id, &title, "", "manga");
                card["imageUrl"] = match image { Some(url) => json!(site::link(url, BASE, MEDIA)?), None => Value::Null };
                cards.push(card);
            }
            result[key] = json!(site::unique(cards, "workId"));
        }
        if result.as_object().is_none_or(|v| v.is_empty()) { return Err(site::invalid()); }
        Ok(result)
    }, |value| {
        for cards in value.as_object().ok_or_else(site::invalid)?.values() {
            for card in cards.as_array().ok_or_else(site::invalid)? {
                site::slug(text(&card["workId"]))?;
                if !card["imageUrl"].is_null() { site::link(text(&card["imageUrl"]), BASE, MEDIA)?; }
            }
        }
        Ok(())
    }).await
}

fn filters() -> Value {
    let mut value = manko_sdk::content::configuration("mangafox-search", "MangaFox", &[("genre", "Genre")], &[("default", "Site Order")], &["en"]);
    value["fields"][0]["supportsExclusion"] = json!(false);
    value["fields"][0]["maximumSelections"] = json!(1);
    value
}

async fn parse_series(context: &Context, html: &str, id: &str) -> Result<Value> {
    // Details appear before the chapter list; parse the long chapter index as
    // individual anchors so large series remain within the guest's budget.
    let prefix = html.split("id=\"chapterlist\"").next().unwrap_or(html);
    let document = Html::parse_document(prefix);
    let title = site::field(&document, ".detail-info-right-title-font");
    if title.is_empty() {
        return Err(site::invalid());
    }
    let cover = site::link(
        &site::attr(&document, ".detail-info-cover-img", "src"),
        BASE,
        MEDIA,
    )?;
    let author = site::field(&document, ".detail-info-right-say a");
    let mut description = site::field(&document, ".fullcontent");
    if description.is_empty() {
        description = site::field(&document, ".detail-info-right-content");
    }
    let mut value = site::details(
        site::card(id, &title, &cover, "manga"),
        &description,
        &author,
        &site::field(&document, ".detail-info-right-title-tip").to_lowercase(),
        "MATURE",
        &format!("{BASE}manga/{id}/"),
    );
    let chapters = html
        .split("id=\"chapterlist\"")
        .nth(1)
        .ok_or_else(site::invalid)?;
    let mut items = Vec::new();
    for (index, block) in site::blocks(chapters, "a")
        .filter(|b| b.header.contains("/1.html"))
        .enumerate()
    {
        if index % 25 == 0 {
            context.now().await?;
        }
        let href = site::attribute(block.header, "href").ok_or_else(site::invalid)?;
        let Some(path) = href
            .strip_prefix("/manga/")
            .and_then(|p| p.strip_suffix("/1.html"))
        else {
            continue;
        };
        let (work, number, volume) = installment(path)?;
        if work != id {
            continue;
        }
        let title = site::attribute(block.header, "title")
            .unwrap_or_else(|| site::field(&Html::parse_fragment(block.whole), "a"));
        items.push(json!([path, title, number, volume]));
    }
    if items.is_empty() {
        return Err(site::invalid());
    }
    let mut seen = std::collections::HashSet::new();
    items.retain(|row| seen.insert(text(&row[0]).to_owned()));
    value["chapters"] = Value::String(
        items
            .iter()
            .map(|r| serde_json::to_string(r).unwrap())
            .collect::<Vec<_>>()
            .join("\n"),
    );
    context.now().await?;
    Ok(value)
}
async fn series(context: &Context, id: &str) -> Result<Value> {
    site::slug(id)?;
    context
        .remember(
            &format!("mangafox:series:v3:{id}"),
            Some(300.0),
            async {
                parse_series(
                    context,
                    &site::document(context, &format!("{BASE}manga/{id}/"), BASE, HOSTS, AGENT)
                        .await?,
                    id,
                )
                .await
            },
            |v| {
                if v["workId"] != id || !v["chapters"].is_string() {
                    return Err(site::invalid());
                }
                site::link(text(&v["imageUrl"]), BASE, MEDIA)?;
                Ok(())
            },
        )
        .await
}
async fn page_info(context: &Context, url: &str) -> Result<Value> {
    let (id, number) = reader(url)?;
    context
        .remember(
            &format!("mangafox:page:{url}"),
            Some(120.0),
            async {
                let html = site::document(context, url, MOBILE, HOSTS, AGENT).await?;
                let document = Html::parse_document(&html);
                let image =
                    site::link(&site::attr(&document, "#viewer #image", "src"), url, MEDIA)?;
                let mut pages = Vec::new();
                for option in
                    document.select(&site::selector("select.mangaread-page option[value]"))
                {
                    let page = site::link(
                        option.value().attr("value").unwrap(),
                        url,
                        &["m.fanfox.net"],
                    )?;
                    let (chapter, n) = reader(&page)?;
                    if chapter != id || n != pages.len() as u64 + 1 {
                        return Err(site::invalid());
                    }
                    pages.push(page);
                }
                if pages.is_empty() || number > pages.len() as u64 {
                    return Err(site::invalid());
                }
                Ok(json!({"image":image,"pages":pages}))
            },
            |v| {
                site::link(text(&v["image"]), url, MEDIA)?;
                let pages = v["pages"]
                    .as_array()
                    .filter(|a| !a.is_empty() && a.len() <= 5000)
                    .ok_or_else(site::invalid)?;
                for (index, p) in pages.iter().enumerate() {
                    let (chapter, n) = reader(text(p))?;
                    if chapter != id || n != index as u64 + 1 {
                        return Err(site::invalid());
                    }
                }
                Ok(())
            },
        )
        .await
}
fn run(context: Context, method: String, args: Value) -> Operation {
    Box::pin(async move {
        match method.as_str() {
            "initialize" | "invalidateCache" => Ok(Value::Null),
            "settings" => Ok(json!({"id":"settings","title":"MangaFox","fields":[]})),
            "discoverSections" => Ok(json!([
                {"id":"hot","title":"Hot Manga Releases","type":0,"presentation":"carousel"},
                {"id":"reading","title":"Being Read Right Now","type":0,"presentation":"carousel"},
                {"id":"ranking","title":"Popular Manga Ranking","type":0,"presentation":"carousel"},
                {"id":"recommended","title":"Recommended","type":0,"presentation":"carousel"},
                {"id":"new","title":"New Manga","type":0,"presentation":"carousel"},
                {"id":"trending","title":"Trending Manga","type":0,"presentation":"carousel"},
                {"id":"directory","title":"All Manga","type":0,"presentation":"carousel"},
                {"id":"latest","title":"Latest Updates","type":0,"presentation":"latestGallery"}
            ])),
            "searchFilters" => Ok(filters()),
            "searchSuggestions" => Ok(manko_sdk::content::suggestions(&args, ["action", "comedy", "drama", "fantasy", "romance", "school-life", "supernatural", "shoujo", "shounen"].into_iter()
                .filter(|genre| genre.contains(&text(&args["query"]).to_lowercase()))
                .map(|genre| json!({"fieldID":"genre","value":genre,"title":genre.replace('-', " ")})).collect())),
            "search" | "discover" => {
                manko_sdk::content::validate_search(&filters(), &args)?;
                let section = args["sectionId"].as_str().or(args["section"]["id"].as_str()).unwrap_or("directory");
                if method == "discover" && section != "directory" {
                    if !matches!(section, "hot" | "reading" | "ranking" | "recommended" | "new" | "trending" | "latest") {
                        return Err(error("Unknown discovery section", "invalidSection"));
                    }
                    let page = page(&args, 100_000)?;
                    let data = homepage(&context).await?;
                    let items = data[section].as_array().ok_or_else(site::invalid)?;
                    let start = (page as usize - 1) * 25;
                    return Ok(json!({"items":items.iter().skip(start).take(25).collect::<Vec<_>>(),
                        "metadata":if start + 25 < items.len() { json!({"page":page+1}) } else { Value::Null }}));
                }
                let page = page(&args, 100_000)?;
                let query = text(&args["query"]).trim();
                if query.len() > 500 {
                    return Err(error("Search query is too long", "invalidSearchTerm"));
                }
                let genres: Vec<_> = args["selections"].as_array().into_iter().flatten().filter(|v| v["fieldID"] == "genre").collect();
                if genres.len() > 1 || (!genres.is_empty() && !query.is_empty()) {
                    return Err(error("Choose one genre or search by title", "invalidSearchTerm"));
                }
                let genre = genres.first().map(|v| site::slug(text(&v["value"]))).transpose()?;
                let search = method == "search" && !query.is_empty();
                let url = if search {
                    let mut url = url::Url::parse(&format!("{BASE}search")).unwrap();
                    url.query_pairs_mut().append_pair("title", query).append_pair("page", &page.to_string());
                    url.to_string()
                } else {
                    let directory = genre.map(|v| format!("directory/{v}/")).unwrap_or_else(|| "directory/".to_owned());
                    if page == 1 { format!("{BASE}{directory}") } else { format!("{BASE}{directory}{page}.html") }
                };
                context
                    .remember(
                        &format!("mangafox:catalog:{url}"),
                        Some(300.0),
                        async {
                            catalog(
                                &context,
                                &site::document(&context, &url, BASE, HOSTS, AGENT).await?,
                                page,
                                search,
                            )
                            .await
                        },
                        |v| {
                            for item in v["items"].as_array().ok_or_else(site::invalid)? {
                                site::slug(text(&item["workId"]))?;
                                site::link(text(&item["imageUrl"]), BASE, MEDIA)?;
                            }
                            Ok(())
                        },
                    )
                    .await
            }
            "details" => {
                let mut value = series(&context, text(&args)).await?;
                value.as_object_mut().unwrap().remove("chapters");
                Ok(value)
            }
            "installments" => {
                let id = text(&args["workId"]);
                let data = series(&context, id).await?;
                let mut items = Vec::new();
                for (index, line) in text(&data["chapters"]).lines().enumerate() {
                    if index % 50 == 0 {
                        context.now().await?;
                    }
                    let row: Value = serde_json::from_str(line).map_err(|_| site::invalid())?;
                    let path = text(&row[0]);
                    let (work, number, volume) = installment(path)?;
                    if work != id {
                        return Err(site::invalid());
                    }
                    items.push(json!({"installmentId":path,"title":row[1],"number":number,"volume":volume,"langCode":"en"}));
                }
                drop(data);
                context.now().await?;
                Ok(Value::Array(items))
            }
            "imagePages" => {
                let id = text(&args["installmentId"]);
                let (work, _, _) = installment(id)?;
                if !args["workId"].is_null() && args["workId"] != work {
                    return Err(error(
                        "Chapter belongs to another series",
                        "invalidIdentifier",
                    ));
                }
                let url = format!("{MOBILE}manga/{id}/1.html");
                Ok(json!({"id":id,"workId":work,"pages":page_info(&context,&url).await?["pages"]}))
            }
            "imagePageContent" => {
                let url = site::link(text(&args["url"]), BASE, HOSTS)?;
                if url::Url::parse(&url).unwrap().host_str() == Some("m.fanfox.net") {
                    let info = page_info(&context, &url).await?;
                    site::image(&context, text(&info["image"]), &url, MEDIA, AGENT).await
                } else {
                    site::image(&context, &url, BASE, MEDIA, AGENT).await
                }
            }
            _ => Err(error("Unsupported operation", "unsupportedOperation")),
        }
    })
}
manko_sdk::export_extension!("MangaFox", "content", run);
