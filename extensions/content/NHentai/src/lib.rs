// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
use manko_sdk::{content::*, *};
use serde_json::{json, Value};
use std::collections::BTreeMap;
const BASE: &str = "https://nhentai.net";
const API: &str = "https://nhentai.net/api/v2";
const FIELDS: &[(&str, &str)] = &[
    ("tag", "Tag"),
    ("artist", "Artist"),
    ("parody", "Parody"),
    ("character", "Character"),
    ("group", "Group"),
    ("language", "Language"),
    ("category", "Category"),
    ("pages", "Page Count"),
    ("favorites", "Favorites"),
    ("uploaded", "Upload Date"),
    ("title", "Title"),
    ("jtitle", "Japanese Title"),
];
std::thread_local! {static OBSERVED: std::cell::RefCell<Vec<Value>> = const {std::cell::RefCell::new(Vec::new())};}
fn filters() -> Value {
    let mut value = configuration(
        "nhentai-search",
        "nHentai Search",
        FIELDS,
        &[
            ("date", "Newest"),
            ("popular-today", "Popular Today"),
            ("popular-week", "Popular This Week"),
            ("popular", "Popular All Time"),
        ],
        &["english", "japanese", "chinese"],
    );
    value["sortedBrowseSectionID"] = json!("latest");
    value
}
async fn request(context: &Context, url: &str, body: Option<Value>, image: bool) -> Result<Value> {
    https(url, &["nhentai.net", "i.nhentai.net", "t.nhentai.net"])?;
    for attempt in 0..2 {
        let response = context.request(url, if body.is_some() {"POST"} else {"GET"}, json!({"Accept":"application/json,image/avif,image/webp,image/png,image/jpeg,image/gif","Content-Type":"application/json","User-Agent":"manko NHentai Extension/2.0.0 (+https://github.com/superdupercoolperson/extensions)","Referer":format!("{BASE}/")}),body.as_ref().map(|v| serde_json::to_vec(v).unwrap()),image).await?;
        if matches!(status(&response), 403 | 503) {
            let data = bytes(&response)?;
            let sample = String::from_utf8_lossy(&data[..data.len().min(8192)]).to_lowercase();
            if header(&response, "cf-mitigated")
                .trim()
                .eq_ignore_ascii_case("challenge")
                || sample.contains("challenge-form")
                || sample.contains("managed-challenge")
                || sample.contains("<title>just a moment")
            {
                context
                    .call("challenge", json!({"url":format!("{BASE}/")}))
                    .await?;
                return Err(error("Visible verification required", "challengeRequired"));
            }
        }
        if status(&response) == 429 && attempt == 0 {
            let seconds = header(&response, "retry-after")
                .parse::<u64>()
                .unwrap_or(1)
                .clamp(1, 60);
            context
                .call("sleep", json!({"milliseconds":seconds*1000}))
                .await?;
            continue;
        }
        if status(&response) == 404 {
            return Err(error(
                "The requested nHentai gallery was not found",
                "notFound",
            ));
        }
        if !(200..300).contains(&status(&response)) {
            return Err(error(
                format!("nHentai returned HTTP {}", status(&response)),
                "serviceError",
            ));
        }
        return Ok(response);
    }
    Err(error("Service rate limit exceeded", "serviceError"))
}
pub fn media(path: &str, cover: bool) -> Result<String> {
    if path.is_empty()
        || path.len() > 2048
        || path
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '\\' | '?' | '#'))
    {
        return Err(error("Invalid media path", "invalidResponse"));
    }
    let host = if cover {
        "t.nhentai.net"
    } else {
        "i.nhentai.net"
    };
    let relative = path.trim_start_matches('/');
    if !path.starts_with("https://")
        && relative.split('/').any(|s| {
            s.is_empty()
                || s == "."
                || s == ".."
                || s.to_lowercase().contains("%2e")
                || s.contains('%')
        })
    {
        return Err(error("Invalid media path", "invalidResponse"));
    }
    if path.starts_with("https://") {
        Ok(https(path, &[host])?.to_string())
    } else {
        // The authority is ours; validating the relative components avoids
        // reparsing hundreds of identical HTTPS authorities for each gallery.
        Ok(format!("https://{host}/{relative}"))
    }
}
fn path(value: &Value) -> &str {
    value.as_str().unwrap_or_else(|| text(&value["path"]))
}
fn titles(gallery: &Value) -> Result<Vec<String>> {
    let values = [
        &gallery["title"]["pretty"],
        gallery["title"]
            .get("english")
            .unwrap_or(&gallery["english_title"]),
        gallery["title"]
            .get("japanese")
            .unwrap_or(&gallery["japanese_title"]),
    ];
    let mut titles = vec![];
    for value in values {
        let title = text(value).trim().to_owned();
        if !title.is_empty() && !titles.contains(&title) {
            titles.push(title);
        }
    }
    if titles.is_empty() {
        return Err(error("Gallery title is missing", "invalidResponse"));
    }
    Ok(titles)
}
pub fn card(gallery: &Value, detail: bool) -> Result<Value> {
    let id = id(&gallery["id"])?;
    manko_sdk::id(&gallery["media_id"])?;
    let titles = titles(gallery)?;
    let cover = media(
        path(if detail {
            gallery.get("cover").unwrap_or(&gallery["thumbnail"])
        } else {
            &gallery["thumbnail"]
        }),
        true,
    )?;
    Ok(
        json!({"type":"work","id":id,"workId":id,"title":titles[0],"subtitle":text(&gallery["title"]["japanese"]),"imageUrl":cover,"coverURL":cover,"contentRating":"ADULT","mediaKind":"manga"}),
    )
}
pub fn work(gallery: &Value) -> Result<Value> {
    let mut result = card(gallery, true)?;
    let pages = gallery["pages"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| error("Gallery pages are missing", "invalidResponse"))?;
    let paths: Vec<Value> = pages
        .iter()
        .map(|page| {
            media(path(page), false)?;
            Ok(json!(path(page)))
        })
        .collect::<Result<_>>()?;
    let mut tags: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for tag in gallery["tags"].as_array().into_iter().flatten() {
        if !text(&tag["name"]).is_empty() && !text(&tag["type"]).is_empty() {
            tags.entry(text(&tag["type"]).into())
                .or_default()
                .push(text(&tag["name"]).into());
        }
    }
    let mut facets = vec![];
    for (field, title) in [
        ("parody", "Parodies"),
        ("character", "Characters"),
        ("tag", "Tags"),
        ("artist", "Artists"),
        ("group", "Groups"),
        ("language", "Languages"),
        ("category", "Categories"),
    ] {
        for value in tags.get(field).into_iter().flatten() {
            facets.push(json!({"fieldID":field,"value":value,"title":value,"groupTitle":title,"presentation":if matches!(field,"artist"|"group") {"creator"} else {"tag"}}));
        }
    }
    let language = tags
        .get("language")
        .and_then(|v| v.first())
        .map(String::as_str)
        .unwrap_or("multi");
    let creators = [
        tags.get("artist").cloned().unwrap_or_default(),
        tags.get("group").cloned().unwrap_or_default(),
    ]
    .concat();
    result["mediaId"] = json!(id(&gallery["media_id"])?);
    result["pagePaths"] = json!(paths);
    result["language"] = json!(language);
    result["uploadedAt"] = gallery["upload_date"]
        .as_i64()
        .map(timestamp)
        .unwrap_or(Value::Null);
    result["tags"] = json!(tags);
    OBSERVED.with(|observed| {
        let mut observed = observed.borrow_mut();
        for facet in &facets {
            if observed.len() < 1000
                && !observed
                    .iter()
                    .any(|v| v["fieldID"] == facet["fieldID"] && v["value"] == facet["value"])
            {
                observed.push(facet.clone());
            }
        }
    });
    result["workInfo"] = json!({"thumbnailUrl":result["imageUrl"],"synopsis":"","primaryTitle":result["title"],"secondaryTitles":titles(gallery)?.into_iter().skip(1).collect::<Vec<_>>(),"contentRating":"ADULT","status":"completed","artist":tags.get("artist").map(|v| v.join(", ")),"author":creators.join(", "),"searchFacets":facets,"shareUrl":format!("{BASE}/g/{}/",text(&result["id"]))});
    Ok(result)
}
fn validate_gallery(gallery: &Value, expected: &str) -> Result<()> {
    if id(&gallery["id"])? != expected {
        return Err(error("Gallery identifier mismatch", "invalidResponse"));
    }
    id(&gallery["media_id"])?;
    titles(gallery)?;
    media(
        path(gallery.get("cover").unwrap_or(&gallery["thumbnail"])),
        true,
    )?;
    for page in pages(gallery)? {
        media(path(page), false)?;
    }
    Ok(())
}
fn pages(gallery: &Value) -> Result<&Vec<Value>> {
    gallery["pages"]
        .as_array()
        .filter(|pages| !pages.is_empty())
        .ok_or_else(|| error("Gallery pages are missing", "invalidResponse"))
}
fn page_paths(gallery: &Value) -> Result<Vec<Value>> {
    pages(gallery)?
        .iter()
        .map(|page| {
            let path = path(page);
            media(path, false)?;
            Ok(json!(path))
        })
        .collect()
}
async fn gallery(context: &Context, value: &Value) -> Result<Value> {
    let id = id(value)?;
    let key = format!("nhentai:gallery:{id}");
    context
        .remember(
            &key,
            Some(1800.0),
            async {
                decode_json(&request(context, &format!("{API}/galleries/{id}"), None, false).await?)
            },
            |value| validate_gallery(value, &id),
        )
        .await
}
fn query(input: &Value) -> String {
    let mut terms = vec![text(input.get("query").unwrap_or(&input["text"]))
        .trim()
        .to_owned()];
    for selection in input["selections"].as_array().into_iter().flatten() {
        let value = text(&selection["value"])
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let value = if value.chars().any(char::is_whitespace) {
            format!("\"{value}\"")
        } else {
            value
        };
        terms.push(format!(
            "{}{}:{value}",
            if selection["polarity"] == "exclude" {
                "-"
            } else {
                ""
            },
            text(&selection["fieldID"])
        ));
    }
    terms
        .into_iter()
        .filter(|v| !v.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn run(context: Context, method: String, args: Value) -> Operation {
    Box::pin(async move {
        match method.as_str() {
            "initialize" => Ok(Value::Null),
            "invalidateCache" => {
                OBSERVED.with(|v| v.borrow_mut().clear());
                Ok(Value::Null)
            }
            "settings" => Ok(json!({"id":"settings","title":"nHentai","fields":[]})),
            "discoverSections" => Ok(
                json!([{"id":"latest","title":"Latest","type":0},{"id":"popular","title":"Popular Today","type":0}]),
            ),
            "searchFilters" => Ok(filters()),
            "search" | "discover" => {
                validate_search(&filters(), &args)?;
                let page = page(&args, 1_000_000)?;
                let query = query(&args);
                let section = args["sectionId"]
                    .as_str()
                    .or(args["section"]["id"].as_str())
                    .unwrap_or("latest");
                if method == "discover" && !matches!(section, "latest" | "popular") {
                    return Err(error("Unknown discovery section", "invalidSection"));
                }
                let sort = args["sort"].as_str().unwrap_or(
                    if method == "discover" && section == "popular" {
                        "popular-today"
                    } else {
                        "date"
                    },
                );
                if !query.is_empty() && query.bytes().all(|v| v.is_ascii_digit()) {
                    return if page > 1 {
                        Ok(json!({"items":[],"metadata":null}))
                    } else {
                        Ok(
                            json!({"items":[card(&gallery(&context,&json!(query)).await?,true)?],"metadata":null}),
                        )
                    };
                }
                let popular = method == "discover"
                    && section == "popular"
                    && query.is_empty()
                    && args["sort"].is_null();
                if popular && page > 1 {
                    return Ok(json!({"items":[],"metadata":null}));
                }
                let endpoint = if popular {
                    format!("{API}/galleries/popular")
                } else if method == "discover" && query.is_empty() && args["sort"].is_null() {
                    format!("{API}/galleries?page={page}")
                } else {
                    format!(
                        "{API}/search?query={}&sort={sort}&page={page}",
                        encode_segment(if query.is_empty() { " " } else { &query })
                    )
                };
                let payload = decode_json(&request(&context, &endpoint, None, false).await?)?;
                let galleries = if popular {
                    payload.as_array()
                } else {
                    payload["result"].as_array()
                }
                .ok_or_else(|| error("Malformed gallery list", "invalidResponse"))?;
                let mut items = vec![];
                for value in galleries {
                    if value["pages"].as_array().is_some_and(|v| !v.is_empty()) {
                        work(value)?;
                        let key = format!("nhentai:gallery:{}", id(&value["id"])?);
                        let ticket = context.cache_read(&key, Some(1800.0)).await?;
                        context.cache_write(&key, &ticket, value).await?;
                    }
                    items.push(card(value, false)?);
                }
                let total = if popular {
                    0
                } else {
                    payload["num_pages"]
                        .as_u64()
                        .ok_or_else(|| error("Invalid page count", "invalidResponse"))?
                };
                Ok(
                    json!({"items":items,"metadata":if page < total {json!({"page":page+1})} else {Value::Null}}),
                )
            }
            "searchSuggestions" => {
                let field = text(&args["fieldID"]);
                if !field.is_empty() && !FIELDS.iter().take(7).any(|(id, _)| *id == field) {
                    return Err(error("Unknown suggestion field", "invalidSearchTerm"));
                }
                let observed = OBSERVED.with(|v| suggestions(&args, v.borrow().clone()));
                if !observed.as_array().unwrap().is_empty()
                    || text(&args["query"]).trim().is_empty()
                {
                    return Ok(observed);
                }
                let global_key = format!(
                    "nhentai:suggestions::{}",
                    text(&args["query"]).trim().to_lowercase()
                );
                if !field.is_empty() {
                    let ticket = context.cache_read(&global_key, Some(300.0)).await?;
                    if ticket["hit"] == true {
                        let matches = suggestions(
                            &args,
                            ticket["value"].as_array().cloned().unwrap_or_default(),
                        );
                        if !matches.as_array().unwrap().is_empty() {
                            return Ok(matches);
                        }
                    }
                }
                let key = format!(
                    "nhentai:suggestions:{field}:{}",
                    text(&args["query"]).trim().to_lowercase()
                );
                let values = context.remember(&key,Some(300.0),async {
            let values = decode_json(&request(&context,&format!("{API}/tags/search"),Some(json!({"type":if field.is_empty() {Value::Null} else {json!(field)},"query":args["query"],"limit":30})),false).await?)?;
            let mut result = vec![];
            for value in values.as_array().ok_or_else(|| error("Malformed suggestions","invalidResponse"))? {if text(&value["name"]).is_empty() || text(&value["name"]).len() > 200 || value["type"].as_str().is_none() {return Err(error("Malformed suggestion","invalidResponse"));}if FIELDS.iter().take(7).any(|(field,_)|*field==text(&value["type"])) {result.push(json!({"fieldID":value["type"],"value":value["name"],"title":value["name"],"subtitle":format!("{} galleries",value["count"].as_u64().unwrap_or(0))}));}}
            Ok(json!(result))}, |v|if v.is_array() {Ok(())} else {Err(error("Invalid cached suggestions","invalidResponse"))}).await?;
                Ok(suggestions(
                    &args,
                    values.as_array().cloned().unwrap_or_default(),
                ))
            }
            "details" => work(&gallery(&context, &args).await?),
            "installments" => {
                let id = id(args.get("workId").unwrap_or(&args["id"]))?;
                let source = if args["pagePaths"].as_array().is_some_and(|v| !v.is_empty()) {
                    args
                } else {
                    let gallery = gallery(&context, &json!(id)).await?;
                    json!({"pagePaths":page_paths(&gallery)?,"mediaId":gallery["media_id"],
                        "uploadedAt":gallery["upload_date"].as_i64().map(timestamp).unwrap_or(Value::Null),
                        "language":gallery["tags"].as_array().into_iter().flatten()
                            .find(|tag| tag["type"] == "language").map(|tag| text(&tag["name"])).unwrap_or("multi")})
                };
                Ok(
                    json!([{"installmentId":format!("gallery:{id}"),"workId":id,"langCode":source["language"],"number":1,"volume":1,"title":"Gallery","publishDate":source["uploadedAt"],"mediaId":source["mediaId"],"pagePaths":source["pagePaths"]}]),
                )
            }
            "imagePages" => {
                let id = id(args.get("workId").unwrap_or(&json!(
                    text(&args["installmentId"]).trim_start_matches("gallery:")
                )))?;
                let source = if args["pagePaths"].as_array().is_some_and(|v| !v.is_empty()) {
                    args
                } else {
                    json!({"pagePaths":page_paths(&gallery(&context, &json!(id)).await?)?})
                };
                let pages: Vec<String> = source["pagePaths"]
                    .as_array()
                    .ok_or_else(|| error("Missing pages", "invalidResponse"))?
                    .iter()
                    .map(|p| media(text(p), false))
                    .collect::<Result<_>>()?;
                Ok(json!({"id":format!("gallery:{id}"),"workId":id,"pages":pages}))
            }
            "imagePageContent" => {
                let url = https(
                    text(args.get("url").unwrap_or(&args["pageURL"])),
                    &["i.nhentai.net", "t.nhentai.net"],
                )?;
                image(&request(&context, url.as_str(), None, true).await?)
            }
            "updates" => Ok(json!({"items":[],"metadata":null})),
            "managedCollections" => Ok(json!([])),
            _ => Err(error("Unsupported operation", "unsupportedOperation")),
        }
    })
}
export_extension!("NHentai", "content", run);
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_rejects_traversal() {
        assert!(media("../page.jpg", false).is_err());
        assert!(media("https://evil.test/page.jpg", false).is_err());
        assert_eq!(
            media("/galleries/1/1.jpg", false).unwrap(),
            "https://i.nhentai.net/galleries/1/1.jpg"
        );
    }
    #[test]
    fn gallery_validation_does_not_construct_work_facets() {
        let mut gallery = json!({"id":101,"media_id":"9001","title":{"english":"Sample"},
            "cover":{"path":"galleries/9001/cover.webp"},"pages":[{"path":"galleries/9001/1.jpg"}],
            "tags":[{"type":"tag","name":"landscape"}]});
        validate_gallery(&gallery, "101").unwrap();
        OBSERVED.with(|observed| assert!(observed.borrow().is_empty()));
        assert!(validate_gallery(&gallery, "102").is_err());
        gallery["pages"][0]["path"] = json!("galleries/%2e%2e/page.jpg");
        assert!(validate_gallery(&gallery, "101").is_err());
    }
}
