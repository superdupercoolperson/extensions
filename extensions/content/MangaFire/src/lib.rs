// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
mod signing;
use manko_sdk::{
    content::{configuration, validate_search},
    error, id, page, text, Context, Operation, Result,
};
use manko_site_support::{self as site, scraper::Html};
use serde_json::{json, Value};
const BASE: &str = "https://mangafire.to/";
const AGENT: &str = "manko MangaFire Extension/0.1.1 (+https://github.com/superdupercoolperson/extensions)";
const LANGUAGES: &[&str] = &["en", "ja", "fr", "es", "es-la", "pt", "pt-br"];
const SORTS: &[(&str, &str)] = &[
    ("relevance:desc", "Best match"),
    ("chapter_updated_at:desc", "Latest update"),
    ("created_at:desc", "Recently added"),
    ("title:asc", "Title (A–Z)"),
    ("title:desc", "Title (Z–A)"),
    ("year:desc", "Year (newest)"),
    ("year:asc", "Year (oldest)"),
    ("score:desc", "Highest rated"),
    ("views_7d:desc", "Most viewed · 7 days"),
    ("views_30d:desc", "Most viewed · 30 days"),
    ("views_total:desc", "Most viewed · all time"),
    ("follows_total:desc", "Most followed"),
];
fn filters() -> Value {
    let mut config = configuration(
        "mangafire-search",
        "MangaFire",
        &[
            ("genre", "Genre"),
            ("type", "Type"),
            ("status", "Status"),
            ("language", "Language"),
        ],
        SORTS,
        LANGUAGES,
    );
    for field in config["fields"].as_array_mut().unwrap() {
        if field["id"] != "genre" {
            field["supportsExclusion"] = json!(false);
        }
        let options = match text(&field["id"]) {
            "type" => vec![
                ("manga", "Manga"),
                ("manhwa", "Manhwa"),
                ("manhua", "Manhua"),
                ("other", "Other"),
            ],
            "status" => vec![
                ("releasing", "Releasing"),
                ("finished", "Finished"),
                ("on_hiatus", "On Hiatus"),
                ("discontinued", "Discontinued"),
                ("not_yet_released", "Not Yet Released"),
            ],
            "language" => vec![
                ("en", "English"),
                ("ja", "Japanese"),
                ("fr", "French"),
                ("es", "Spanish"),
                ("es-la", "Spanish (Latin America)"),
                ("pt", "Portuguese"),
                ("pt-br", "Portuguese (Brazil)"),
            ],
            _ => vec![],
        };
        if !options.is_empty() {
            field["inputKind"] = json!("choice");
            field["options"] = json!(options
                .iter()
                .map(|(id, title)| json!({"id":id,"title":title}))
                .collect::<Vec<_>>());
        }
    }
    config
}
// CDN subdomains rotate. Validate the DNS suffix, then use the exact host in
// the common helper; suffix lookalikes and non-HTTPS URLs fail.
fn media_link(value: &str) -> Result<String> {
    let url = url::Url::parse(BASE)
        .unwrap()
        .join(value)
        .map_err(|_| site::invalid())?;
    let host = url.host_str().ok_or_else(site::invalid)?;
    if !(host.ends_with(".mfcdn.nl")
        || host.ends_with(".mfcdn1.xyz")
        || host.ends_with(".mfcdn2.xyz")
        || host.ends_with(".mfcdn3.xyz"))
    {
        return Err(error(
            "MangaFire supplied an unexpected image host",
            "hostNotAllowed",
        ));
    }
    if url.fragment().is_some() {
        return Err(site::invalid());
    }
    site::link(url.as_str(), BASE, &[host])
}
fn work_id(value: &str) -> Result<&str> {
    site::slug(value)?;
    if value.len() > 64 || !value.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(error(
            "Invalid MangaFire title identifier",
            "invalidIdentifier",
        ));
    }
    Ok(value)
}
fn card(data: &Value) -> Result<Value> {
    let hid = work_id(text(&data["hid"]))?;
    let title = text(&data["title"]);
    if title.trim().is_empty() {
        return Err(site::invalid());
    }
    let cover = ["large", "medium", "small"]
        .into_iter()
        .find_map(|size| data["poster"][size].as_str().filter(|s| !s.is_empty()))
        .map(media_link)
        .transpose()?
        .unwrap_or_default();
    let kind = match text(&data["type"]) {
        "manga" => "manga",
        "manhwa" | "manhua" | "other" => "comic",
        _ => return Err(site::invalid()),
    };
    Ok(site::card(hid, title, &cover, kind))
}
async fn api(context: &Context, path: &str, pairs: Vec<(String, String)>) -> Result<Value> {
    let url = signing::request_url(path, pairs);
    let body = site::document(context, &url, BASE, &["mangafire.to"], AGENT).await
        .map_err(|e| if e["type"] == "authenticationRequired" {
            error("MangaFire rejected the request. Its request signing may have changed; update the extension.", "serviceUnavailable")
        } else { e })?;
    serde_json::from_str(&body).map_err(|_| site::invalid())
}
fn pagination(data: &Value, current: u64) -> Result<bool> {
    let meta = &data["meta"];
    let last = meta["lastPage"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= 100_000)
        .ok_or_else(site::invalid)?;
    let next = meta["hasNext"].as_bool().ok_or_else(site::invalid)?;
    if meta["page"].as_u64() != Some(current) || current > last || next != (current < last) {
        return Err(site::invalid());
    }
    Ok(next)
}
async fn series(context: &Context, work: &str) -> Result<Value> {
    work_id(work)?;
    context
        .remember(
            &format!("mangafire:series:v1:{work}"),
            Some(300.0),
            async { Ok(api(context, &format!("/titles/{work}"), vec![]).await?["data"].clone()) },
            |data| {
                if data["hid"] != work {
                    return Err(site::invalid());
                }
                card(data)?;
                Ok(())
            },
        )
        .await
}
fn details(data: &Value) -> Result<Value> {
    let authors = data["authors"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| text(&a["title"]))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let synopsis = Html::parse_fragment(text(&data["synopsisHtml"]));
    let status = match text(&data["status"]) {
        "releasing" => "ongoing",
        "finished" => "completed",
        "on_hiatus" => "hiatus",
        "discontinued" => "cancelled",
        _ => "unknown",
    };
    let rating = match text(&data["contentRating"]) {
        "safe" => "SAFE",
        "suggestive" => "MATURE",
        _ => "ADULT",
    };
    let work = work_id(text(&data["hid"]))?;
    let mut value = site::details(
        card(data)?,
        &site::plain(synopsis.root_element()),
        &authors,
        status,
        rating,
        &format!("{BASE}title/{work}"),
    );
    value["workInfo"]["secondaryTitles"] = json!(data["altTitles"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>());
    value["workInfo"]["artist"] = json!(data["artists"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| text(&a["title"]))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", "));
    Ok(value)
}
fn chapter_identity(value: &str, work: &str) -> Result<String> {
    work_id(work)?;
    let (owner, chapter) = value
        .split_once('/')
        .ok_or_else(|| error("Invalid chapter identifier", "invalidIdentifier"))?;
    if owner != work {
        return Err(error(
            "Chapter belongs to another MangaFire title",
            "invalidIdentifier",
        ));
    }
    id(&json!(chapter))
}
async fn chapters(context: &Context, work: &str) -> Result<Value> {
    work_id(work)?;
    context.remember(&format!("mangafire:chapters:v1:{work}"), Some(300.0), async {
        let mut items = Vec::new(); let mut current = 1;
        loop {
            let data = api(context, &format!("/titles/{work}/chapters"), vec![
                ("sort".into(), "number".into()), ("order".into(), "desc".into()),
                ("page".into(), current.to_string()), ("limit".into(), "200".into()),
            ]).await?;
            let next = pagination(&data, current)?;
            let rows = data["items"].as_array().filter(|r| r.len() <= 200 && (!next || !r.is_empty())).ok_or_else(site::invalid)?;
            for row in rows {
                let chapter = id(&row["id"])?;
                let language = text(&row["language"]);
                if !LANGUAGES.contains(&language) { return Err(site::invalid()); }
                let number = row["number"].as_f64().filter(|n| n.is_finite() && *n >= 0.0).ok_or_else(site::invalid)?;
                let mut title = format!("Chapter {number}");
                if !text(&row["name"]).is_empty() { title.push_str(&format!(" · {}", text(&row["name"]))); }
                items.push(json!({"installmentId":format!("{work}/{chapter}"),"workId":work,
                    "title":title,"number":number,"langCode":language,"scanlator":row["type"],
                    "publishDate":row["createdAt"].as_i64().map(manko_sdk::timestamp).unwrap_or(Value::Null),"format":"remoteImageSequence"}));
            }
            if !next { break; }
            current += 1;
            if current > 500 { return Err(error("MangaFire's chapter list exceeds the supported size", "invalidResponse")); }
        }
        Ok(json!(site::unique(items, "installmentId")))
    }, |data| {
        for row in data.as_array().ok_or_else(site::invalid)? {
            chapter_identity(text(&row["installmentId"]), work)?;
            if row["workId"] != work { return Err(site::invalid()); }
        }
        Ok(())
    }).await
}
async fn genres(context: &Context) -> Result<Value> {
    context
        .remember(
            "mangafire:genres:v1",
            Some(3600.0),
            async { Ok(api(context, "/filter-options", vec![]).await?["data"]["genres"].clone()) },
            |data| {
                for row in data.as_array().ok_or_else(site::invalid)? {
                    id(&row["id"])?;
                    if text(&row["name"]).is_empty() {
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
            "settings" => Ok(json!({"id":"settings","title":"MangaFire","fields":[]})),
            "searchFilters" => Ok(filters()),
            "discoverSections" => Ok(json!([
                {"id":"trending-day","title":"Trending Today","type":0,"presentation":"carousel"},
                {"id":"trending","title":"Trending This Week","type":0,"presentation":"carousel"},
                {"id":"trending-month","title":"Trending This Month","type":0,"presentation":"carousel"},
                {"id":"latest","title":"Latest Updates","type":0,"presentation":"latestGallery"},
            ])),
            "searchSuggestions" => {
                let field = text(&args["fieldID"]);
                if !field.is_empty() && field != "genre" {
                    return Ok(json!([]));
                }
                let query = text(&args["query"]).trim().to_lowercase();
                let data = genres(&context).await?;
                let mut values = data.as_array().unwrap().iter().filter(|row| text(&row["name"]).to_lowercase().contains(&query) || row["id"].to_string() == query)
                    .map(|row| json!({"fieldID":"genre","value":row["id"].to_string(),"title":row["name"]})).collect::<Vec<_>>();
                values.sort_by_key(|row| text(&row["title"]).to_lowercase());
                values.truncate(args["limit"].as_u64().unwrap_or(20).clamp(1, 30) as usize);
                Ok(json!(values))
            }
            "search" | "discover" => {
                let config = filters();
                validate_search(&config, &args)?;
                let current = page(&args, 100_000)?;
                let section = args["sectionId"]
                    .as_str()
                    .or(args["section"]["id"].as_str())
                    .unwrap_or("latest");
                if method == "discover" && !matches!(section, "trending-day" | "trending" | "trending-month" | "latest") {
                    return Err(error("Unknown discovery section", "invalidSection"));
                }
                let trending = method == "discover" && section.starts_with("trending");
                if trending && current != 1 {
                    return Ok(json!({"items":[],"metadata":null}));
                }
                let mut pairs = if trending {
                    vec![
                        ("type".into(), "trending".into()),
                        ("days".into(), match section { "trending-day" => "1", "trending-month" => "30", _ => "7" }.into()),
                        ("limit".into(), "30".into()),
                    ]
                } else {
                    vec![
                        ("page".into(), current.to_string()),
                        ("limit".into(), "30".into()),
                    ]
                };
                if !trending {
                    let query = text(&args["query"]).trim();
                    if query.len() > 500 {
                        return Err(error("Search query is too long", "invalidSearchTerm"));
                    }
                    if !query.is_empty() {
                        pairs.push(("keyword".into(), query.into()));
                    }
                    let sort = args["sort"].as_str().unwrap_or(if method == "discover" {
                        "chapter_updated_at:desc"
                    } else {
                        "relevance:desc"
                    });
                    let (field, order) = sort.split_once(':').unwrap();
                    pairs.push((format!("order[{field}]"), order.into()));
                    for selection in args["selections"].as_array().into_iter().flatten() {
                        let field = text(&selection["fieldID"]);
                        let value = text(&selection["value"]);
                        let param = match field {
                            "genre" => {
                                id(&selection["value"])?;
                                if selection["polarity"] == "exclude" {
                                    "genres_ex[]"
                                } else {
                                    "genres_in[]"
                                }
                            }
                            "type" | "status" | "language" => {
                                let definition = config["fields"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .find(|f| f["id"] == field)
                                    .unwrap();
                                if !definition["options"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .any(|o| o["id"] == value)
                                {
                                    return Err(error(
                                        "Unknown MangaFire filter value",
                                        "invalidSearchTerm",
                                    ));
                                }
                                match field {
                                    "type" => "types[]",
                                    "status" => "statuses[]",
                                    _ => "languages[]",
                                }
                            }
                            _ => return Err(error("Unknown search field", "invalidSearchTerm")),
                        };
                        pairs.push((param.into(), value.into()));
                    }
                }
                // Cache the unsigned request identity and parsed metadata only.
                pairs.sort_by(|a, b| a.0.cmp(&b.0));
                let key = format!(
                    "mangafire:catalog:v1:{trending}:{}",
                    serde_json::to_string(&pairs).unwrap()
                );
                context.remember(&key, Some(300.0), async {
                    let data = api(&context, if trending { "/top-titles" } else { "/titles" }, pairs).await?;
                    let rows = data["items"].as_array().filter(|a| a.len() <= 30).ok_or_else(site::invalid)?;
                    let next = !trending && pagination(&data, current)?;
                    Ok(json!({"items":site::unique(rows.iter().map(card).collect::<Result<Vec<_>>>()?,"workId"),"metadata":if next {json!({"page":current+1})}else {Value::Null}}))
                }, |value| {
                    for row in value["items"].as_array().ok_or_else(site::invalid)? {
                        work_id(text(&row["workId"]))?;
                        if !text(&row["imageUrl"]).is_empty() { media_link(text(&row["imageUrl"]))?; }
                    }
                    Ok(())
                }).await
            }
            "details" => details(&series(&context, text(&args)).await?),
            "installments" => chapters(&context, text(&args["workId"])).await,
            "imagePages" => {
                let work = work_id(text(&args["workId"]))?;
                let installment = text(&args["installmentId"]);
                let chapter = chapter_identity(installment, work)?;
                // Resolve page URLs on every reader open because they can expire.
                let data =
                    api(&context, &format!("/chapters/{chapter}"), vec![]).await?["data"].clone();
                if id(&data["id"])? != chapter || data["title"]["hid"] != work {
                    return Err(site::invalid());
                }
                let rows = data["pages"]
                    .as_array()
                    .filter(|a| !a.is_empty() && a.len() <= 5000)
                    .ok_or_else(site::invalid)?;
                let pages = rows
                    .iter()
                    .map(|row| media_link(text(&row["url"])))
                    .collect::<Result<Vec<_>>>()?;
                Ok(json!({"id":installment,"workId":work,"pages":pages}))
            }
            "imagePageContent" => {
                let url = media_link(text(&args["url"]))?;
                let parsed = url::Url::parse(&url).unwrap();
                let mut response = site::request(&context, &url, BASE,
                    &[parsed.host_str().unwrap()], AGENT, true).await?;
                // Both cover and chapter CDNs use the nonstandard JPEG alias.
                let mime = response["mimeType"].as_str()
                    .unwrap_or_else(|| manko_sdk::header(&response, "content-type"))
                    .split(';').next().unwrap_or("").trim();
                if mime.eq_ignore_ascii_case("image/jpg") {
                    response["mimeType"] = json!("image/jpeg");
                }
                manko_sdk::content::image(&response)
            }
            _ => Err(error("Unsupported operation", "unsupportedOperation")),
        }
    })
}
manko_sdk::export_extension!("MangaFire", "content", run);
