// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
use manko_sdk::{
    content::{configuration, validate_search},
    error, page, text, Context, Operation, Result,
};
use manko_site_support as site;
use serde_json::{json, Value};
const BASE: &str = "https://omegascans.org/";
const API: &str = "https://api.omegascans.org/";
const HOSTS: &[&str] = &[
    "omegascans.org",
    "api.omegascans.org",
    "media.omegascans.org",
];
const AGENT: &str = "manko OmegaScans Extension/0.1.1 (+https://github.com/superdupercoolperson/extensions)";
fn filters() -> Value {
    let mut config = configuration(
        "omega-search",
        "Omega Scans",
        &[("genre", "Genre")],
        &[
            ("created_at", "Newest"),
            ("updated_at", "Recently Updated"),
            ("total_views", "Popular"),
        ],
        &["en"],
    );
    config["fields"][0]["supportsExclusion"] = json!(false);
    config
}
fn card(value: &Value) -> Result<Value> {
    let id = site::slug(text(&value["series_slug"]))?;
    let title = text(&value["title"]);
    if title.is_empty() || value["series_type"] != "Comic" {
        return Err(site::invalid());
    }
    let image = site::link(text(&value["thumbnail"]), BASE, &["media.omegascans.org"])?;
    Ok(site::card(id, title, &image, "comic"))
}
async fn api(context: &Context, url: &str) -> Result<Value> {
    let body = site::document(context, url, BASE, HOSTS, AGENT).await?;
    serde_json::from_str(&body).map_err(|_| site::invalid())
}
async fn series(context: &Context, id: &str) -> Result<Value> {
    site::slug(id)?;
    context
        .remember(
            &format!("omega:series:{id}"),
            Some(1800.0),
            async {
                let html =
                    site::document(context, &format!("{BASE}series/{id}"), BASE, HOSTS, AGENT)
                        .await?;
                site::flight_object(&html, |v| {
                    v["series_slug"] == id && v["author"].is_string() && v["title"].is_string()
                })
            },
            |v| {
                if v["series_slug"] != id || v["id"].as_u64().filter(|id| *id > 0).is_none() {
                    return Err(site::invalid());
                }
                card(v)?;
                Ok(())
            },
        )
        .await
}
fn chapter_identity(value: &str, work: &str) -> Result<String> {
    let (series, chapter) = value
        .split_once('/')
        .ok_or_else(|| error("Invalid chapter identifier", "invalidIdentifier"))?;
    site::slug(series)?;
    site::slug(chapter)?;
    if series != work {
        return Err(error(
            "Chapter belongs to another series",
            "invalidIdentifier",
        ));
    }
    Ok(chapter.to_owned())
}
async fn chapters(context: &Context, id: &str) -> Result<Value> {
    let data = series(context, id).await?;
    let remote = data["id"].as_u64().unwrap();
    context.remember(&format!("omega:chapters:{id}"),Some(300.0),async {
        let value=api(context,&format!("{API}chapter/query?page=1&perPage=10000&series_id={remote}")).await?;
        let chapters=value["data"].as_array().ok_or_else(site::invalid)?;
        if value["meta"]["last_page"].as_u64().is_none_or(|p|p>1) {return Err(error("Chapter list exceeds the supported catalog size","invalidResponse"));}
        let mut items=Vec::new();
        let numbers=regex::Regex::new(r"(?i)chapter\s+(\d+(?:\.\d+)?)").unwrap();
        for chapter in chapters {
            if chapter["series"]["series_slug"]!=id || chapter["series"]["id"].as_u64()!=Some(remote) {return Err(site::invalid());}
            let price=chapter["price"].as_f64().filter(|p|p.is_finite()&&*p>=0.0).ok_or_else(site::invalid)?;
            if price>0.0 {continue;}
            let slug=site::slug(text(&chapter["chapter_slug"]))?;
            let name = chapter["chapter_name"].as_str().filter(|name| !name.trim().is_empty()).ok_or_else(site::invalid)?;
            let number = chapter["index"].as_f64()
                .or_else(|| text(&chapter["index"]).parse().ok())
                .or_else(|| numbers.captures(name).and_then(|c| c[1].parse::<f64>().ok()));
            let number = match number {
                Some(number) if number.is_finite() && number >= 0.0 => number,
                // Season markers and announcements have no chapter number. The
                // native installment contract requires one; do not invent it.
                None if chapter["index"].is_null() => continue,
                _ => return Err(site::invalid()),
            };
            items.push(json!({"installmentId":format!("{id}/{slug}"),"workId":id,"title":chapter["chapter_name"],"number":number,"publishDate":chapter["created_at"],"langCode":"en","format":"remoteImageSequence"}));
        }
        Ok(json!(site::unique(items,"installmentId")))
    },|v|{
        for item in v.as_array().ok_or_else(site::invalid)? {chapter_identity(text(&item["installmentId"]),id)?; if item["workId"]!=id {return Err(site::invalid());}}
        Ok(())
    }).await
}
async fn tags(context: &Context) -> Result<Value> {
    context
        .remember(
            "omega:tags",
            Some(1800.0),
            api(context, &format!("{API}tags?all=true")),
            |v| {
                for tag in v.as_array().ok_or_else(site::invalid)? {
                    if tag["id"].as_u64().filter(|id| *id > 0).is_none()
                        || text(&tag["name"]).is_empty()
                    {
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
            "settings" => Ok(json!({"id":"settings","title":"Omega Scans","fields":[]})),
            "discoverSections" => Ok(
                json!([{"id":"popular","title":"Popular","type":0,"presentation":"carousel"},{"id":"new","title":"New Comics","type":0,"presentation":"carousel"},{"id":"latest","title":"Latest Comic Updates","type":0,"presentation":"latestGallery"}]),
            ),
            "searchFilters" => Ok(filters()),
            "searchSuggestions" => {
                if args["fieldID"]
                    .as_str()
                    .is_some_and(|f| !f.is_empty() && f != "genre")
                {
                    return Ok(json!([]));
                }
                let query = text(&args["query"]).trim().to_lowercase();
                let values = tags(&context).await?;
                let mut values:Vec<_>=values.as_array().unwrap().iter().filter(|v|text(&v["name"]).to_lowercase().contains(&query)||v["id"].to_string()==query)
                .map(|v|json!({"fieldID":"genre","value":v["id"].to_string(),"title":v["name"]})).collect();
                values.sort_by_key(|v| text(&v["title"]).to_lowercase());
                values.truncate(args["limit"].as_u64().unwrap_or(20).clamp(1, 30) as usize);
                Ok(json!(values))
            }
            "search" | "discover" => {
                validate_search(&filters(), &args)?;
                let page = page(&args, 100_000)?;
                let section = args["sectionId"]
                    .as_str()
                    .or(args["section"]["id"].as_str())
                    .unwrap_or("latest");
                if method == "discover" && !matches!(section, "latest" | "popular" | "new") {
                    return Err(error("Unknown discovery section", "invalidSection"));
                }
                let sort = args["sort"].as_str().unwrap_or(
                    if method == "discover" && section == "popular" {
                        "total_views"
                    } else if method == "discover" && section == "latest" {
                        "updated_at"
                    } else {
                        "created_at"
                    },
                );
                let query = text(&args["query"]).trim();
                if query.len() > 500 {
                    return Err(error("Search query is too long", "invalidSearchTerm"));
                }
                let mut genres = Vec::new();
                for selection in args["selections"].as_array().into_iter().flatten() {
                    genres.push(manko_sdk::id(&selection["value"])?.parse::<u64>().unwrap());
                }
                genres.sort_unstable();
                genres.dedup();
                let mut url = url::Url::parse(&format!("{API}query")).unwrap();
                url.query_pairs_mut()
                    .append_pair("page", &page.to_string())
                    .append_pair("perPage", "20")
                    .append_pair("series_type", "Comic")
                    .append_pair("query_string", query)
                    .append_pair("orderBy", sort)
                    .append_pair("adult", "true")
                    .append_pair("order", "desc")
                    .append_pair("status", "All")
                    .append_pair("tags_ids", &serde_json::to_string(&genres).unwrap());
                context.remember(&format!("omega:catalog:{url}"),Some(300.0),async {
                let response=api(&context,url.as_str()).await?;
                let values=response["data"].as_array().ok_or_else(site::invalid)?;
                let last=response["meta"]["last_page"].as_u64().filter(|n|*n<=100_000).ok_or_else(site::invalid)?;
                if response["meta"]["current_page"].as_u64()!=Some(page) {return Err(site::invalid());}
                let items=site::unique(values.iter().map(card).collect::<Result<Vec<_>>>()?,"workId");
                Ok(json!({"items":items,"metadata":if page<last {json!({"page":page+1})}else{Value::Null}}))
            },|v|{for item in v["items"].as_array().ok_or_else(site::invalid)? {site::slug(text(&item["workId"]))?;site::link(text(&item["imageUrl"]),BASE,&["media.omegascans.org"])?;}Ok(())}).await
            }
            "details" => {
                let id = site::slug(text(&args))?;
                let data = series(&context, id).await?;
                Ok(site::details(
                    card(&data)?,
                    &site::plain(site::scraper::Html::parse_fragment(text(&data["description"])).root_element()),
                    text(&data["author"]),
                    &text(&data["status"]).to_lowercase(),
                    "ADULT",
                    &format!("{BASE}series/{id}"),
                ))
            }
            "installments" => chapters(&context, text(&args["workId"])).await,
            "imagePages" => {
                let work = site::slug(text(&args["workId"]))?;
                let id = text(&args["installmentId"]);
                let chapter = chapter_identity(id, work)?;
                context
                    .remember(
                        &format!("omega:reader:{id}"),
                        Some(300.0),
                        async {
                            let html = site::document(
                                &context,
                                &format!("{BASE}series/{work}/{chapter}"),
                                BASE,
                                HOSTS,
                                AGENT,
                            )
                            .await?;
                            let data = site::flight_object(&html, |v| {
                                v["chapter_slug"] == chapter
                                    && v["series"]["series_slug"] == work
                                    && v["price"].is_number()
                            })?;
                            if data["price"].as_f64() != Some(0.0) {
                                return Err(error(
                                    "This chapter requires a purchase on Omega Scans",
                                    "authenticationRequired",
                                ));
                            }
                            let pages = data["chapter_data"]["images"]
                                .as_array()
                                .filter(|a| !a.is_empty() && a.len() <= 5000)
                                .ok_or_else(site::invalid)?
                                .iter()
                                .map(|v| {
                                    site::link(text(v), BASE, &["media.omegascans.org"])
                                        .map(Value::String)
                                })
                                .collect::<Result<Vec<_>>>()?;
                            Ok(json!({"id":id,"workId":work,"pages":pages}))
                        },
                        |v| {
                            if v["id"] != id || v["workId"] != work {
                                return Err(site::invalid());
                            }
                            for p in v["pages"]
                                .as_array()
                                .filter(|a| !a.is_empty())
                                .ok_or_else(site::invalid)?
                            {
                                site::link(text(p), BASE, &["media.omegascans.org"])?;
                            }
                            Ok(())
                        },
                    )
                    .await
            }
            "imagePageContent" => {
                let mut response = site::request(
                    &context,
                    text(&args["url"]),
                    BASE,
                    &["media.omegascans.org"],
                    AGENT,
                    true,
                )
                .await?;
                // The live media CDN labels JPEGs with the nonstandard jpg
                // alias. Normalize only that alias, retaining native handles
                // and the SDK's rejection of HTML and unsupported formats.
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
manko_sdk::export_extension!("OmegaScans", "content", run);
