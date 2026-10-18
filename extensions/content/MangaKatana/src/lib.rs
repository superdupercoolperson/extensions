// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
use manko_sdk::{content, error, page, text, Context, Operation, Result};
use manko_site_support::{self as site, scraper::Html};
use serde_json::{json, Value};
use std::collections::HashSet;
const BASE: &str = "https://mangakatana.com/";
const HOSTS: &[&str] = &["mangakatana.com", "i1.mangakatana.com"];
const AGENT: &str = "manko MangaKatana Extension/3.0.1";
const GENRES: &str = "4-koma action adult adventure artbook award-winning comedy cooking doujinshi drama ecchi erotica fantasy gender-bender gore harem historical horror isekai josei loli manhua manhwa martial-arts mecha medical music mystery one-shot psychological romance school-life sci-fi seinen sexual-violence shota shoujo shoujo-ai shounen shounen-ai slice-of-life sports super-power supernatural survival time-travel tragedy webtoon yaoi yuri";
fn invalid_search() -> Value {
    error(
        "Choose title/author search or directory filters, and use the available options",
        "invalidSearchTerm",
    )
}
fn work(id: &str) -> Result<&str> {
    site::slug(id)?;
    let (slug, number) = id.rsplit_once('.').ok_or_else(site::invalid)?;
    if slug.is_empty() || number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return Err(error("Invalid title identifier", "invalidIdentifier"));
    }
    Ok(id)
}
fn chapter(id: &str) -> Result<(&str, f64)> {
    let (w, c) = id.split_once('/').ok_or_else(site::invalid)?;
    work(w)?;
    let n = c.strip_prefix('c').ok_or_else(site::invalid)?;
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return Err(site::invalid());
    }
    let number = n
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.)
        .ok_or_else(site::invalid)?;
    Ok((w, number))
}
fn path(value: &str) -> Result<String> {
    let u = url::Url::parse(&site::link(value, BASE, &["mangakatana.com"])?)
        .map_err(|_| site::invalid())?;
    if u.query().is_some() || u.fragment().is_some() {
        return Err(site::invalid());
    }
    Ok(u.path()
        .strip_prefix("/manga/")
        .ok_or_else(site::invalid)?
        .trim_end_matches('/')
        .to_owned())
}
fn config() -> Value {
    let mut v = content::configuration(
        "mangakatana-search",
        "MangaKatana",
        &[
            ("genre", "Genre"),
            ("author", "Author"),
            ("include_mode", "Genre matching"),
            ("status", "Status"),
            ("chapters", "Chapters"),
        ],
        &[
            ("default", "Site Order"),
            ("latest", "Latest Update"),
            ("new", "Newest"),
            ("az", "A–Z"),
            ("numc", "Chapter Count"),
        ],
        &["en"],
    );
    v["sortedBrowseSectionID"] = json!("directory");
    for field in v["fields"].as_array_mut().unwrap() {
        match text(&field["id"]) {
            "genre" => {}
            "author" => {
                field["inputKind"] = json!("text");
                field["maximumSelections"] = json!(1);
                field["supportsExclusion"] = json!(false);
            }
            id => {
                let options = match id {
                    "include_mode" => {
                        vec![("and", "All selected genres"), ("or", "Any selected genre")]
                    }
                    "status" => vec![("1", "Ongoing"), ("2", "Completed"), ("0", "Cancelled")],
                    _ => vec![
                        ("1", "1+"),
                        ("e1", "Exactly 1"),
                        ("5", "5+"),
                        ("10", "10+"),
                        ("20", "20+"),
                        ("30", "30+"),
                        ("50", "50+"),
                        ("100", "100+"),
                        ("150", "150+"),
                        ("200", "200+"),
                    ],
                };
                field["options"] = json!(options
                    .into_iter()
                    .map(|(id, title)| json!({"id":id,"title":title}))
                    .collect::<Vec<_>>());
                field["inputKind"] = json!("choice");
                field["maximumSelections"] = json!(1);
                field["supportsExclusion"] = json!(false);
            }
        }
    }
    v
}
fn catalog_url(args: &Value, section: &str, p: u64) -> Result<String> {
    let cfg = config();
    content::validate_search(&cfg, args)?;
    let selections: Vec<_> = args["selections"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    let query = text(&args["query"]).trim();
    if query.len() > 500 {
        return Err(invalid_search());
    }
    let author = selections
        .iter()
        .find(|v| v["fieldID"] == "author")
        .map(|v| text(&v["value"]).trim());
    let sort = args["sort"].as_str().unwrap_or("default");
    let is_text = !query.is_empty() || author.is_some();
    if is_text
        && ((!query.is_empty() && author.is_some())
            || selections.iter().any(|v| v["fieldID"] != "author")
            || sort != "default")
    {
        return Err(invalid_search());
    }
    let route = if is_text {
        ""
    } else {
        match section {
            "hot" => "",
            "latest" => "latest",
            "new" => "new-manga",
            "directory" => "manga",
            _ => return Err(error("Unknown discovery section", "invalidSection")),
        }
    };
    let suffix = if p == 1 {
        String::new()
    } else {
        format!("page/{p}")
    };
    let mut u = url::Url::parse(&format!(
        "{BASE}{}{suffix}",
        if route.is_empty() {
            String::new()
        } else {
            format!("{route}/")
        }
    ))
    .unwrap();
    if is_text {
        u.query_pairs_mut()
            .append_pair("search", author.unwrap_or(query))
            .append_pair(
                "search_by",
                if author.is_some() { "author" } else { "m_name" },
            );
    } else if !selections.is_empty() || sort != "default" {
        let mut include = Vec::new();
        let mut exclude = Vec::new();
        let mut mode = "and";
        let mut status = None;
        let mut chapters = "1";
        for s in &selections {
            let val = text(&s["value"]);
            let field = cfg["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["id"] == s["fieldID"])
                .unwrap();
            if field["inputKind"] == "choice"
                && !field["options"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|o| o["id"] == val)
            {
                return Err(invalid_search());
            }
            match text(&s["fieldID"]) {
                "genre" => {
                    if !GENRES.split_whitespace().any(|g| g == val) {
                        return Err(invalid_search());
                    }
                    if s["polarity"] == "exclude" {
                        exclude.push(val)
                    } else {
                        include.push(val)
                    }
                }
                "include_mode" => mode = val,
                "status" => status = Some(val),
                "chapters" => chapters = val,
                _ => return Err(invalid_search()),
            }
        }
        if include.iter().any(|v| exclude.contains(v)) {
            return Err(invalid_search());
        }
        let mut q = u.query_pairs_mut();
        q.append_pair("filter", "1")
            .append_pair("include_mode", mode)
            .append_pair("bookmark_opts", "off")
            .append_pair("chapters", chapters)
            .append_pair("order", if sort == "default" { "az" } else { sort });
        if !include.is_empty() {
            q.append_pair("include", &include.join("_"));
        }
        if !exclude.is_empty() {
            q.append_pair("exclude", &exclude.join("_"));
        }
        if let Some(s) = status {
            q.append_pair("status", s);
        }
    }
    Ok(u.into())
}
async fn catalog(context: &Context, html: &str, hot: bool, p: u64) -> Result<Value> {
    let doc = Html::parse_document(html);
    let container = doc
        .select(&site::selector(if hot {
            "#hot_update"
        } else {
            "#book_list"
        }))
        .next()
        .ok_or_else(site::invalid)?;
    let mut items = Vec::new();
    for (i, row) in container.select(&site::selector(".item")).enumerate() {
        if i % 25 == 0 {
            context.now().await?;
        }
        let a = row
            .select(&site::selector(".title a[href]"))
            .next()
            .ok_or_else(site::invalid)?;
        let id = path(a.value().attr("href").unwrap())?;
        work(&id)?;
        let title = site::plain(a);
        if title.is_empty() {
            return Err(site::invalid());
        }
        let img = row
            .select(&site::selector(".wrap_img img"))
            .next()
            .ok_or_else(site::invalid)?;
        let src = ["lazy-src", "data-src", "src"]
            .into_iter()
            .filter_map(|k| img.value().attr(k))
            .find(|v| !v.is_empty() && *v != "#")
            .ok_or_else(site::invalid)?;
        items.push(site::card(
            &id,
            &title,
            &site::link(src, BASE, HOSTS)?,
            "manga",
        ));
    }
    let next = !hot
        && doc
            .select(&site::selector(
                ".pagination a[href], .uk-pagination a[href]",
            ))
            .any(|a| path_page(a.value().attr("href").unwrap()).is_some_and(|n| n == p + 1));
    Ok(
        json!({"items":site::unique(items,"workId"),"metadata":if next{json!({"page":p+1})}else{Value::Null}}),
    )
}
fn path_page(href: &str) -> Option<u64> {
    let u = url::Url::parse(&site::link(href, BASE, &["mangakatana.com"]).ok()?).ok()?;
    u.path()
        .trim_end_matches('/')
        .rsplit_once("/page/")?
        .1
        .parse()
        .ok()
}
async fn series(context: &Context, id: &str) -> Result<Value> {
    work(id)?;
    context.remember(&format!("mangakatana:series:v1:{id}"),Some(300.),async{
        let html=site::document(context,&format!("{BASE}manga/{id}"),BASE,HOSTS,AGENT).await?;
        let prefix=html.split("class=\"chapters\"").next().unwrap_or(&html);
        let doc=Html::parse_document(prefix);
        let title=site::field(&doc,"#single_book h1");if title.is_empty(){return Err(site::invalid());}
        let cover=site::link(&site::attr(&doc,"#single_book .cover img","src"),BASE,HOSTS)?;
        let authors=doc.select(&site::selector("#single_book .authors a")).map(site::plain).collect::<Vec<_>>().join(", ");
        let mut v=site::details(site::card(id,&title,&cover,"manga"),&site::field(&doc,"#single_book .summary p"),&authors,&site::field(&doc,"#single_book .status").to_lowercase(),"ADULT",&format!("{BASE}manga/{id}"));
        v["workInfo"]["secondaryTitles"]=json!(site::field(&doc,"#single_book .alt_name").split(';').map(str::trim).filter(|s|!s.is_empty()).collect::<Vec<_>>());
        v["workInfo"]["searchFacets"]=json!(doc.select(&site::selector("#single_book .genres a[href]")).filter_map(|a|{
            let u=url::Url::parse(&site::link(a.value().attr("href")?,BASE,&["mangakatana.com"]).ok()?).ok()?;
            Some(json!({"fieldID":"genre","value":u.path().strip_prefix("/genre/")?,"title":site::plain(a),"groupTitle":"Genres","presentation":"tag"}))
        }).collect::<Vec<_>>());
        let tail=html.split("class=\"chapters\"").nth(1).ok_or_else(site::invalid)?;
        let mut chapters=Vec::new();let mut seen=HashSet::new();
        for (i,row) in site::blocks(tail,"tr").enumerate(){
            if i%25==0{context.now().await?;}
            // Long indexes contain thousands of small rows. Building an HTML5
            // tree for every row exhausts the mobile runtime's invocation fuel.
            let section = site::blocks(row.body, "div").find(|b|
                site::attribute(b.header, "class").is_some_and(|v| v.split_whitespace().any(|c| c == "chapter")));
            let Some(a) = section.and_then(|b| site::blocks(b.body, "a").next()) else { continue; };
            let Some(href) = site::attribute(a.header, "href") else { continue; };
            let cid = if let Some(relative) = href.strip_prefix("/manga/")
                .or_else(|| href.strip_prefix("https://mangakatana.com/manga/")) {
                chapter(relative)?;
                relative.to_owned()
            } else { path(&href)? };
            let(w,n)=chapter(&cid)?;
            if w!=id || !seen.insert(cid.clone()){continue;}
            let date_text = site::blocks(row.body, "div").find(|b|
                site::attribute(b.header, "class").is_some_and(|v| v.split_whitespace().any(|c| c == "update_time")))
                .map(|b| inline_text(b.body)).unwrap_or_default();
            let date=chrono::NaiveDate::parse_from_str(&date_text,"%b-%d-%Y").ok().and_then(|d|d.and_hms_opt(0,0,0)).map(|d|d.format("%Y-%m-%dT%H:%M:%SZ").to_string());
            chapters.push(serde_json::to_string(&json!({"installmentId":cid,"title":inline_text(a.body),"number":n,"langCode":"en","publishDate":date})).unwrap());
        }
        v["chapters"]=json!(chapters.join("\n"));Ok(v)
    },|v|{if v["workId"]!=id || !v["chapters"].is_string(){return Err(site::invalid());}site::link(text(&v["imageUrl"]),BASE,HOSTS)?;Ok(())}).await
}
fn inline_text(html: &str) -> String {
    if html.contains('<') {
        site::plain(Html::parse_fragment(html).root_element())
    } else {
        html_escape::decode_html_entities(html).split_whitespace().collect::<Vec<_>>().join(" ")
    }
}
// A deliberately small string-array parser, never a JavaScript interpreter.
fn image_array(html: &str) -> Result<Vec<String>> {
    let re = regex::Regex::new(r"\b(?:var|let|const)\s+thzq\s*=\s*\[").unwrap();
    let start = re.find(html).ok_or_else(site::invalid)?.end();
    let mut chars = html[start..].chars().peekable();
    let mut images = Vec::new();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.peek() == Some(&']') {
            chars.next();
            break;
        }
        let quote = chars
            .next()
            .filter(|c| matches!(c, '\'' | '"'))
            .ok_or_else(site::invalid)?;
        let mut value = String::new();
        let mut closed = false;
        while let Some(c) = chars.next() {
            if c == quote {
                closed = true;
                break;
            }
            if c == '\\' {
                let e = chars.next().ok_or_else(site::invalid)?;
                match e {
                    '\\' | '/' | '\'' | '"' => value.push(e),
                    'u' | 'x' => {
                        let n = if e == 'u' { 4 } else { 2 };
                        let digits: String = chars.by_ref().take(n).collect();
                        if digits.len() != n {
                            return Err(site::invalid());
                        }
                        value.push(
                            char::from_u32(
                                u32::from_str_radix(&digits, 16).map_err(|_| site::invalid())?,
                            )
                            .ok_or_else(site::invalid)?,
                        );
                    }
                    _ => return Err(site::invalid()),
                }
            } else {
                if c.is_control() {
                    return Err(site::invalid());
                }
                value.push(c);
            }
            if value.len() > 8192 {
                return Err(site::invalid());
            }
        }
        if !closed {
            return Err(site::invalid());
        }
        images.push(site::link(&value, BASE, &["i1.mangakatana.com"])?);
        if images.len() > 5000 {
            return Err(site::invalid());
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        match chars.next() {
            Some(',') => {}
            Some(']') => break,
            _ => return Err(site::invalid()),
        }
    }
    if images.is_empty() {
        return Err(site::invalid());
    }
    Ok(images)
}
async fn reader(context: &Context, id: &str) -> Result<Value> {
    chapter(id)?;
    context
        .remember(
            &format!("mangakatana:reader:v1:{id}"),
            Some(120.),
            async {
                let html =
                    site::document(context, &format!("{BASE}manga/{id}"), BASE, HOSTS, AGENT)
                        .await?;
                Ok(json!(image_array(&html)?))
            },
            |v| {
                let a = v
                    .as_array()
                    .filter(|a| !a.is_empty() && a.len() <= 5000)
                    .ok_or_else(site::invalid)?;
                for u in a {
                    site::link(text(u), BASE, &["i1.mangakatana.com"])?;
                }
                Ok(())
            },
        )
        .await
}
fn page_ref(value: &str) -> Result<(String, usize)> {
    let u = url::Url::parse(&site::link(value, BASE, &["mangakatana.com"])?)
        .map_err(|_| site::invalid())?;
    if u.query().is_some() {
        return Err(site::invalid());
    }
    let id = u
        .path()
        .strip_prefix("/manga/")
        .ok_or_else(site::invalid)?
        .to_owned();
    chapter(&id)?;
    let raw = u
        .fragment()
        .and_then(|f| f.strip_prefix("page-"))
        .ok_or_else(site::invalid)?;
    let n = raw
        .parse::<usize>()
        .ok()
        .filter(|n| *n > 0 && *n <= 5000 && raw == n.to_string())
        .ok_or_else(site::invalid)?;
    Ok((id, n - 1))
}
// MangaKatana's image CDN serves JPEGs as application/octet-stream.
// Identify bytes only for that MIME type; never label arbitrary HTML as an image.
async fn reader_image(context: &Context, url: &str, referer: &str) -> Result<Value> {
    let mut response =
        site::request(context, url, referer, &["i1.mangakatana.com"], AGENT, true).await?;
    let mime = response["mimeType"]
        .as_str()
        .unwrap_or_else(|| manko_sdk::header(&response, "content-type"))
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    if mime.eq_ignore_ascii_case("application/octet-stream") {
        let data = manko_sdk::bytes(&response)?;
        let kind = if data.starts_with(&[0xff, 0xd8, 0xff]) {
            "image/jpeg"
        } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            "image/gif"
        } else {
            return Err(site::invalid());
        };
        response["mimeType"] = json!(kind);
    }
    content::image(&response)
}
async fn image(context: &Context, value: &str) -> Result<Value> {
    let u = url::Url::parse(&site::link(value, BASE, HOSTS)?).map_err(|_| site::invalid())?;
    if u.host_str() == Some("mangakatana.com")
        && u.path().starts_with("/imgs/cover/")
        && u.fragment().is_none()
    {
        return site::image(context, u.as_str(), BASE, HOSTS, AGENT).await;
    }
    let (id, index) = page_ref(value)?;
    let referer = format!("{BASE}manga/{id}");
    for attempt in 0..2 {
        let data = reader(context, &id).await?;
        let image = data
            .get(index)
            .and_then(Value::as_str)
            .ok_or_else(site::invalid)?;
        match reader_image(context, image, &referer).await {
            Ok(v) => return Ok(v),
            Err(e)
                if attempt == 0
                    && matches!(text(&e["type"]), "authenticationRequired" | "notFound") =>
            {
                context
                    .call(
                        "cache",
                        json!({"operation":"remove","key":format!("mangakatana:reader:v1:{id}")}),
                    )
                    .await?;
            }
            Err(e) => return Err(e),
        }
    }
    Err(site::invalid())
}
fn run(context: Context, method: String, args: Value) -> Operation {
    Box::pin(async move {
        match method.as_str() {
            "initialize" | "invalidateCache" => Ok(Value::Null),
            "settings" => Ok(json!({"id":"settings","title":"MangaKatana","fields":[]})),
            "discoverSections" => Ok(json!([
                {"id":"hot","title":"Hot Updates","type":0,"presentation":"carousel"},
                {"id":"latest","title":"Latest Updates","type":0,"presentation":"carousel"},
                {"id":"new","title":"New Manga","type":0,"presentation":"carousel"},
                {"id":"directory","title":"Manga Directory","type":0,"presentation":"carousel"}
            ])),
            "searchFilters" => Ok(config()),
            "searchSuggestions" => Ok(content::suggestions(
                &args,
                GENRES
                    .split_whitespace()
                    .map(|g| json!({"fieldID":"genre","value":g,"title":g.replace('-'," ")}))
                    .collect(),
            )),
            "discover" | "search" => {
                let section = if method == "search" {
                    "directory"
                } else {
                    args["sectionId"]
                        .as_str()
                        .or(args["section"]["id"].as_str())
                        .unwrap_or("directory")
                };
                let p = page(&args, 100_000)?;
                if section == "hot" && p > 1 {
                    return Ok(json!({"items":[],"metadata":null}));
                }
                let url = catalog_url(&args, section, p)?;
                context
                    .remember(
                        &format!("mangakatana:catalog:v1:{section}:{url}"),
                        Some(300.),
                        async {
                            catalog(
                                &context,
                                &site::document(&context, &url, BASE, HOSTS, AGENT).await?,
                                section == "hot",
                                p,
                            )
                            .await
                        },
                        |v| {
                            for item in v["items"].as_array().ok_or_else(site::invalid)? {
                                work(text(&item["workId"]))?;
                                site::link(text(&item["imageUrl"]), BASE, HOSTS)?;
                            }
                            Ok(())
                        },
                    )
                    .await
            }
            "details" => {
                let mut v = series(&context, text(&args)).await?;
                v.as_object_mut().unwrap().remove("chapters");
                Ok(v)
            }
            "installments" => {
                let id = text(&args["workId"]);
                let data = series(&context, id).await?;
                let mut items = Vec::new();
                for (i, line) in text(&data["chapters"]).lines().enumerate() {
                    if i % 50 == 0 {
                        context.now().await?;
                    }
                    let v: Value = serde_json::from_str(line).map_err(|_| site::invalid())?;
                    if chapter(text(&v["installmentId"]))?.0 != id {
                        return Err(site::invalid());
                    }
                    items.push(v);
                }
                Ok(json!(items))
            }
            "imagePages" => {
                let id = text(&args["installmentId"]);
                let (w, _) = chapter(id)?;
                if !args["workId"].is_null() && args["workId"] != w {
                    return Err(error(
                        "Chapter belongs to another title",
                        "invalidIdentifier",
                    ));
                }
                let data = reader(&context, id).await?;
                Ok(
                    json!({"id":id,"workId":w,"pages":(1..=data.as_array().unwrap().len()).map(|n|format!("{BASE}manga/{id}#page-{n}")).collect::<Vec<_>>()}),
                )
            }
            "imagePageContent" => image(&context, text(&args["url"])).await,
            _ => Err(error("Unsupported operation", "unsupportedOperation")),
        }
    })
}
manko_sdk::export_extension!("MangaKatana", "content", run);
