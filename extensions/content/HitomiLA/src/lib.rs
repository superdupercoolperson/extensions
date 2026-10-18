// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
use base64::{engine::general_purpose::STANDARD, Engine};
use html_escape::decode_html_entities;
use manko_sdk::{content::*, *};
use manko_site_support::{self as site, link};
use percent_encoding::percent_decode_str;
use regex::Regex;
use serde::{
    de::{Error as _, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;
use std::task::Poll;
const STATIC: &str = "https://ltn.gold-usergeneratedcontent.net";
const INDEX_CHUNK_BYTES: u64 = 16 * 1024;
const CATALOG_PAGE_SIZE: u64 = 25;
const CATALOG_INDEX_SIZE: u64 = 25;
const MAX_CATALOG_OFFSET: u64 = 16 * 1024 * 1024 / 4;
static PAGE_HOST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^a(?:[1-9][0-9]{0,2}|1000)\.gold-usergeneratedcontent\.net$").unwrap()
});
static GALLERY_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\s*(?:"use strict";\s*)?var\s+galleryinfo\s*=\s*"#).unwrap());
static ROUTING_START: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bgg\s*=\s*\{").unwrap());
static ROUTING_PATH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bb\s*:\s*["']([A-Za-z0-9._/-]+)["']"#).unwrap());
static ROUTING_DEFAULT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bvar\s+o\s*=\s*([0-9]{1,3})\s*;").unwrap());
static ROUTING_SWITCH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bswitch\s*\(\s*g\s*\)\s*\{").unwrap());
static ROUTING_RETURN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*return\s+o\s*;").unwrap());
const LANGUAGES: &[&str] = &[
    "english",
    "japanese",
    "chinese",
    "spanish",
    "french",
    "german",
    "korean",
    "russian",
    "italian",
    "portuguese",
    "polish",
    "dutch",
    "vietnamese",
    "indonesian",
    "thai",
    "czech",
    "hungarian",
    "arabic",
    "turkish",
    "ukrainian",
];
const FIELDS: &[(&str, &str)] = &[
    ("tag", "Tag"),
    ("female", "Female Tag"),
    ("male", "Male Tag"),
    ("artist", "Artist"),
    ("group", "Group"),
    ("series", "Series"),
    ("character", "Character"),
    ("language", "Language"),
    ("type", "Type"),
];
fn filters() -> Value {
    let mut value = configuration(
        "hitomi-search",
        "Hitomi.la Search",
        FIELDS,
        &[
            ("newest", "Newest (Date Added)"),
            ("date-published", "Date Published"),
            ("popular-today", "Popular Today"),
            ("popular-week", "Popular This Week"),
            ("popular-month", "Popular This Month"),
            ("popular-year", "Popular This Year"),
            ("random", "Random"),
        ],
        LANGUAGES,
    );
    value["sortedBrowseSectionID"] = json!("latest");
    value
}
fn invalid(message: &str) -> Value {
    error(message, "invalidResponse")
}
async fn request(
    context: &Context,
    url: &str,
    range: Option<(u64, u64)>,
    image: bool,
) -> Result<Value> {
    let parsed = url::Url::parse(url).map_err(|_| invalid("Invalid URL"))?;
    let host = parsed.host_str().unwrap_or("");
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || !([
            "hitomi.la",
            "tagindex.hitomi.la",
            "ltn.gold-usergeneratedcontent.net",
            "atn.gold-usergeneratedcontent.net",
        ]
        .contains(&host)
            || PAGE_HOST.is_match(host))
    {
        return Err(error("Unexpected image host", "hostNotAllowed"));
    }
    let mut headers = json!({"Referer":"https://hitomi.la/","Accept":if image {"image/avif,image/webp,image/gif,image/jpeg,image/png"} else {"application/octet-stream,text/plain,application/json"}});
    if let Some((start, end)) = range {
        headers["Range"] = json!(format!("bytes={start}-{end}"));
        headers["Accept-Encoding"] = json!("identity");
    }
    let response = context.request(url, "GET", headers, None, image).await?;
    if status(&response) == 404 || (range.is_some() && status(&response) == 416) {
        return Ok(response);
    }
    if !matches!(status(&response), 200 | 206) {
        return Err(error(
            format!("Hitomi.la returned HTTP {}", status(&response)),
            "serviceError",
        ));
    }
    Ok(response)
}
async fn cached_text(context: &Context, key: &str, url: &str, ttl: f64) -> Result<String> {
    let value = context
        .remember(
            key,
            Some(ttl),
            async {
                let response = request(context, url, None, false).await?;
                if status(&response) == 404 {
                    return Err(error("Gallery not found", "notFound"));
                }
                Ok(json!(String::from_utf8(bytes(&response)?)
                    .map_err(|_| invalid("Invalid text response"))?))
            },
            |value| {
                let source = value
                    .as_str()
                    .ok_or_else(|| invalid("Malformed cached text"))?;
                if key == "hitomi:index-version"
                    && (source.trim().is_empty()
                        || source.trim().len() > 20
                        || !source.trim().bytes().all(|v| v.is_ascii_digit()))
                {
                    return Err(invalid("Invalid index version"));
                }
                Ok(())
            },
        )
        .await?;
    Ok(text(&value).to_owned())
}
fn hash(value: &Value) -> Result<&str> {
    let hash = text(value);
    if !valid_hash(hash) {
        return Err(invalid("Invalid image hash"));
    }
    Ok(hash)
}
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn gallery_assignment(source: &str) -> Result<Value> {
    serde_json::from_str(gallery_json(source)?).map_err(|_| invalid("Malformed gallery JSON"))
}
fn gallery_json(source: &str) -> Result<&str> {
    let prefix = GALLERY_PREFIX
        .find(source)
        .ok_or_else(|| invalid("Malformed gallery assignment"))?;
    Ok(source[prefix.end()..].trim().trim_end_matches(';'))
}

// Catalog cards need only the first file; details and readers validate all files.
struct GalleryCard(Value);
impl<'de> Deserialize<'de> for GalleryCard {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct CardVisitor;
        impl<'de> Visitor<'de> for CardVisitor {
            type Value = GalleryCard;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("gallery metadata")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut gallery = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    let value = match key.as_str() {
                        "files" => map.next_value::<CoverFiles>()?.0,
                        "id" | "title" | "japanese_title" | "language" | "language_localname"
                        | "type" | "date" | "galleryurl" | "artists" | "groups" | "parodys"
                        | "characters" | "tags" => map.next_value::<Value>()?,
                        _ => {
                            map.next_value::<serde::de::IgnoredAny>()?;
                            continue;
                        }
                    };
                    gallery.insert(key, value);
                }
                Ok(GalleryCard(Value::Object(gallery)))
            }
        }
        deserializer.deserialize_map(CardVisitor)
    }
}
struct CoverFiles(Value);
impl<'de> Deserialize<'de> for CoverFiles {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct FilesVisitor;
        #[derive(Deserialize)]
        struct FileHash {
            hash: String,
        }
        impl<'de> Visitor<'de> for FilesVisitor {
            type Value = CoverFiles;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("gallery files")
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let first = seq.next_element::<FileHash>()?;
                if let Some(file) = &first {
                    if !valid_hash(&file.hash) {
                        return Err(A::Error::custom("Invalid image hash"));
                    }
                }
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                let first = first.map(|file| json!({"hash":file.hash}));
                Ok(CoverFiles(json!(first.into_iter().collect::<Vec<_>>())))
            }
        }
        deserializer.deserialize_seq(FilesVisitor)
    }
}
fn gallery_card_assignment(source: &str) -> Result<Value> {
    serde_json::from_str::<GalleryCard>(gallery_json(source)?)
        .map(|card| card.0)
        .map_err(|_| invalid("Malformed gallery JSON"))
}
fn validate_gallery(value: &Value, expected: &str) -> Result<()> {
    if id(&value["id"])? != expected || text(&value["title"]).trim().is_empty() {
        return Err(invalid("Gallery metadata mismatch"));
    }
    let files = value["files"]
        .as_array()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| invalid("Gallery files missing"))?;
    for file in files {
        hash(&file["hash"])?;
    }
    Ok(())
}
async fn gallery(context: &Context, value: &Value) -> Result<Value> {
    let id = id(value)?;
    let source = gallery_text(context, &id).await?;
    let result = gallery_assignment(&source).and_then(|value| {
        validate_gallery(&value, &id)?;
        Ok(value)
    });
    if result.is_err() {
        // Catalog validation covers only the first file. Evict a raw gallery
        // rejected by full validation so corrected detail/reader retries reload it.
        context
            .call(
                "cache",
                json!({"operation":"remove","key":format!("hitomi:gallery-v2:{id}")}),
            )
            .await?;
    }
    result
}
async fn gallery_data(context: &Context, id: &str) -> Result<(Value, Value)> {
    // Keep the validated projection instead of parsing the same gallery twice.
    let card = RefCell::new(None);
    let raw = context
        .remember(
            &format!("hitomi:gallery-v2:{id}"),
            Some(1800.0),
            async {
                let response =
                    request(context, &format!("{STATIC}/galleries/{id}.js"), None, false).await?;
                if status(&response) == 404 {
                    return Err(error("Gallery not found", "notFound"));
                }
                Ok(json!(String::from_utf8(bytes(&response)?)
                    .map_err(|_| invalid("Invalid text response"))?))
            },
            |value| {
                let source = value
                    .as_str()
                    .ok_or_else(|| invalid("Malformed cached text"))?;
                let projection = gallery_card_assignment(source)?;
                validate_gallery(&projection, id)?;
                *card.borrow_mut() = Some(projection);
                Ok(())
            },
        )
        .await?;
    Ok((
        raw,
        card.into_inner()
            .ok_or_else(|| invalid("Gallery metadata missing"))?,
    ))
}
async fn gallery_text(context: &Context, id: &str) -> Result<String> {
    Ok(text(&gallery_data(context, id).await?.0).to_owned())
}
fn card_path(path: &str, expected: &str) -> Result<()> {
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['?', '#', '\\'])
        || !(path == format!("/galleries/{expected}.html")
            || path.ends_with(&format!("-{expected}.html")))
    {
        return Err(invalid("Gallery card identifier mismatch"));
    }
    Ok(())
}
fn card_share(value: &str, expected: &str) -> Result<url::Url> {
    let share = link(value, "https://hitomi.la/", &["hitomi.la"])?;
    let url = url::Url::parse(&share).map_err(|_| invalid("Invalid gallery link"))?;
    card_path(url.path(), expected)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(invalid("Invalid gallery link"));
    }
    Ok(url)
}
fn card_text(source: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    let mut quote = None;
    for c in source.chars() {
        if in_tag {
            if let Some(q) = quote {
                if c == q { quote = None; }
            } else if matches!(c, '\'' | '"') { quote = Some(c); }
            else if c == '>' { in_tag = false; }
        } else if c == '<' {
            in_tag = true;
            output.push(' ');
        } else { output.push(c); }
    }
    decode_html_entities(&output).split_whitespace().collect::<Vec<_>>().join(" ")
}
fn card_class<'a>(source: &'a str, class: &str) -> &'a str {
    for tag in ["div", "td", "p"] {
        if let Some(block) = site::blocks(source, tag).find(|block|
            site::attribute(block.header, "class").is_some_and(|value| value.split_whitespace().any(|value| value == class))) {
            return block.body;
        }
    }
    ""
}
pub fn card_assignment(source: &str, expected: &str) -> Result<Value> {
    // Read the small, known card fields directly. Building an HTML5 DOM for
    // every decorative node on a 25-card page exceeds the guest fuel budget.
    let heading = site::blocks(source, "h1").next().ok_or_else(|| invalid("Missing gallery card title"))?;
    let anchor = site::blocks(heading.body, "a").next().ok_or_else(|| invalid("Missing gallery card title"))?;
    let title = card_text(anchor.body);
    let share_url = card_share(&site::attribute(anchor.header, "href").unwrap_or_default(), expected)?;
    if title.is_empty() { return Err(invalid("Missing gallery card title")); }
    let cover = card_class(source, "dj-img1");
    let mut remaining = cover;
    let mut source_set = String::new();
    while let Some(header) = site::tag_header(remaining, "source") {
        remaining = remaining.split_once(header).ok_or_else(|| invalid("Invalid card cover"))?.1;
        if site::attribute(header, "type").as_deref() == Some("image/avif") {
            source_set = site::attribute(header, "data-srcset").unwrap_or_default();
            break;
        }
    }
    let thumbnail = source_set.split(',').next().unwrap_or("").split_whitespace().next().unwrap_or("");
    let thumbnail = if thumbnail.is_empty() {
        site::tag_header(cover, "img").and_then(|h| site::attribute(h, "data-src")).unwrap_or_default()
    } else { thumbnail.to_owned() };
    let thumbnail = link(
        &thumbnail,
        "https://hitomi.la/",
        &[
            "tn.gold-usergeneratedcontent.net",
            "atn.gold-usergeneratedcontent.net",
        ],
    )?;
    let thumbnail = url::Url::parse(&thumbnail).map_err(|_| invalid("Invalid card cover"))?;
    static COVER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^/(?:avifbigtn|webpbigtn)/([0-9a-f])/([0-9a-f]{2})/([0-9a-f]{64})\.(?:avif|webp)$",
        )
        .unwrap()
    });
    let captures = COVER
        .captures(thumbnail.path())
        .ok_or_else(|| invalid("Invalid card cover"))?;
    let cover_hash = &captures[3];
    if &captures[1] != &cover_hash[63..]
        || &captures[2] != &cover_hash[61..63]
        || thumbnail.query().is_some()
        || thumbnail.fragment().is_some()
    {
        return Err(invalid("Invalid card cover hash path"));
    }
    let anchors: Vec<_> = site::blocks(source, "a").collect();
    let find_anchor = |prefix: &str| anchors.iter().find(|anchor|
        site::attribute(anchor.header, "href").is_some_and(|href| href.starts_with(prefix)));
    let language_anchor = find_anchor("/index-");
    let language_link = language_anchor.and_then(|a| site::attribute(a.header, "href")).unwrap_or_default();
    let language = language_link.strip_prefix("/index-").and_then(|s| s.strip_suffix(".html")).unwrap_or("unknown");
    let language_name = language_anchor.map(|a| card_text(a.body)).unwrap_or_default();
    let mut gallery = json!({
        "id":expected,"title":title,"galleryurl":share_url.path(),
        "language":language,"language_localname":if language_name.is_empty() {language} else {&language_name},
        "type":find_anchor("/type/").map(|a| card_text(a.body)).unwrap_or_default(),
        "files":[{"hash":cover_hash}],"date":card_text(card_class(source,"date")),"tags":[],
    });
    for (group, key, class, prefix) in [
        ("artists", "artist", "artist-list", ""),
        ("groups", "group", "", "/group/"),
        ("parodys", "parody", "series-list", ""),
        ("characters", "character", "", "/character/"),
    ] {
        let values = if class.is_empty() {
            anchors.iter().filter(|a| site::attribute(a.header,"href").is_some_and(|href| href.starts_with(prefix)))
                .map(|a| card_text(a.body)).collect::<Vec<_>>()
        } else {
            site::blocks(card_class(source,class),"a").map(|a| card_text(a.body)).collect()
        };
        gallery[group] = json!(values.into_iter().map(|value| {
            let mut row = serde_json::Map::new();
            row.insert(key.into(), json!(value));
            Value::Object(row)
        }).collect::<Vec<_>>());
    }
    for tag in site::blocks(card_class(source,"relatedtags"),"a") {
        let href = site::attribute(tag.header,"href").unwrap_or_default();
        let Some(encoded) = href.strip_prefix("/tag/").and_then(|s| s.strip_suffix("-all.html")) else { continue; };
        let decoded = percent_decode_str(encoded).decode_utf8().map_err(|_| invalid("Invalid tag encoding"))?;
        let (namespace,name) = decoded.split_once(':').unwrap_or(("tag", &decoded));
        if name.trim().is_empty() { continue; }
        let mut value = json!({"tag":name});
        if matches!(namespace,"female"|"male") { value[namespace] = json!(1); }
        gallery["tags"].as_array_mut().unwrap().push(value);
    }
    validate_gallery(&gallery, expected)?;
    Ok(gallery)
}
async fn gallery_card(context: &Context, id: &str) -> Result<Value> {
    context
        .remember(
            &format!("hitomi:gallery-card-v3:{id}"),
            Some(1800.0),
            async {
                let response = request(
                    context,
                    &format!("{STATIC}/galleryblock/{id}.html"),
                    None,
                    false,
                )
                .await?;
                if status(&response) == 404 {
                    return Err(error("Gallery not found", "notFound"));
                }
                let source = String::from_utf8(bytes(&response)?)
                    .map_err(|_| invalid("Invalid card text"))?;
                card_assignment(&source, id)
            },
            |value| {
                validate_gallery(value, id)?;
                if value["files"].as_array().map(Vec::len) != Some(1) {
                    return Err(invalid("Invalid cached gallery card"));
                }
                card_path(text(&value["galleryurl"]), id)?;
                Ok(())
            },
        )
        .await
}
fn values(gallery: &Value, group: &str, key: &str) -> Vec<String> {
    gallery[group]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| text(&v[key]).trim())
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}
pub fn work(gallery: &Value) -> Result<Value> {
    work_with_share(gallery, None)
}
fn work_with_share(gallery: &Value, validated_share: Option<String>) -> Result<Value> {
    let id = id(&gallery["id"])?;
    validate_gallery(gallery, &id)?;
    let hash = hash(&gallery["files"][0]["hash"])?;
    let cover = format!(
        "https://atn.gold-usergeneratedcontent.net/avifbigtn/{}/{}/{hash}.avif",
        &hash[63..],
        &hash[61..63]
    );
    let mut tags: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut facets = vec![];
    for tag in gallery["tags"].as_array().into_iter().flatten() {
        let value = text(&tag["tag"]).trim();
        if value.is_empty() {
            continue;
        }
        let field = if tag["female"] == true || tag["female"] == 1 || tag["female"] == "1" {
            "female"
        } else if tag["male"] == true || tag["male"] == 1 || tag["male"] == "1" {
            "male"
        } else {
            "tag"
        };
        tags.entry(field.into()).or_default().push(value.into());
    }
    for (group, key, field, title) in [
        ("artists", "artist", "artist", "Artists"),
        ("groups", "group", "group", "Groups"),
        ("parodys", "parody", "series", "Series"),
        ("characters", "character", "character", "Characters"),
    ] {
        for value in values(gallery, group, key) {
            facets.push(json!({"fieldID":field,"value":value,"title":value,"groupTitle":title,"presentation":if matches!(field,"artist"|"group") {"creator"} else {"tag"}}));
        }
    }
    for (field, group) in [("female", "Female"), ("male", "Male"), ("tag", "Tags")] {
        for value in tags.get(field).into_iter().flatten() {
            facets.push(json!({"fieldID":field,"value":value,"title":value,"groupTitle":group,"presentation":"tag"}));
        }
    }
    for (field, title) in [("language", "Language"), ("type", "Type")] {
        if !text(&gallery[field]).is_empty() {
            facets.push(json!({"fieldID":field,"value":gallery[field],"title":gallery[field],"groupTitle":title,"presentation":"tag"}));
        }
    }
    let alternate = text(&gallery["japanese_title"]).trim();
    let primary = text(&gallery["title"]).trim();
    let mut share = validated_share
        .clone()
        .unwrap_or_else(|| format!("https://hitomi.la/galleries/{id}.html"));
    if validated_share.is_none() && text(&gallery["galleryurl"]).starts_with('/') {
        share = https(
            url::Url::parse("https://hitomi.la")
                .unwrap()
                .join(text(&gallery["galleryurl"]))
                .map_err(|_| invalid("Invalid gallery URL"))?
                .as_str(),
            &["hitomi.la"],
        )?
        .to_string();
    }
    Ok(
        json!({"type":"work","id":id,"workId":id,"title":primary,"subtitle":gallery.get("language_localname").unwrap_or(&gallery["language"]),"imageUrl":cover,"previewImageUrl":cover.replace("/avifbigtn/", "/avifsmallbigtn/"),"coverURL":cover,"contentRating":"ADULT","mediaKind":"manga","language":gallery["language"],"tags":tags,"publishedAt":gallery["date"],"workInfo":{"thumbnailUrl":cover,"synopsis":"","primaryTitle":primary,"secondaryTitles":if alternate.is_empty() || alternate == primary {vec![]} else {vec![alternate]},"contentRating":"ADULT","status":"completed","artist":values(gallery,"artists","artist").join(", "),"author":([values(gallery,"artists","artist"),values(gallery,"groups","group")].concat().join(", ")),"searchFacets":facets,"shareUrl":share}}),
    )
}
#[derive(Debug, PartialEq)]
pub struct Routing {
    path: String,
    routes: Vec<u16>,
}
#[derive(PartialEq)]
enum RoutingToken {
    Case,
    Output,
    Break,
    Number(u32),
    Colon,
    Equals,
    Semicolon,
}
struct RoutingLexer<'a>(&'a str);
impl RoutingLexer<'_> {
    fn next(&mut self) -> Result<Option<RoutingToken>> {
        self.0 = self.0.trim_start();
        let Some(first) = self.0.bytes().next() else {
            return Ok(None);
        };
        let length = if first.is_ascii_digit() {
            self.0.bytes().take_while(u8::is_ascii_digit).count()
        } else if first.is_ascii_alphabetic() || first == b'_' || first == b'$' {
            self.0
                .bytes()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'$')
                .count()
        } else if matches!(first, b':' | b'=' | b';') {
            1
        } else {
            return Err(invalid("Unsupported routing token"));
        };
        let value = &self.0[..length];
        self.0 = &self.0[length..];
        let token = match value {
            "case" => RoutingToken::Case,
            "o" => RoutingToken::Output,
            "break" => RoutingToken::Break,
            ":" => RoutingToken::Colon,
            "=" => RoutingToken::Equals,
            ";" => RoutingToken::Semicolon,
            _ if first.is_ascii_digit() => RoutingToken::Number(
                value
                    .parse()
                    .map_err(|_| invalid("Invalid routing number"))?,
            ),
            _ => return Err(invalid("Unsupported routing token")),
        };
        Ok(Some(token))
    }
    fn expect(&mut self, token: RoutingToken) -> Result<()> {
        if self.next()? != Some(token) {
            return Err(invalid("Malformed routing statement"));
        }
        Ok(())
    }
    fn number(&mut self) -> Result<u32> {
        match self.next()? {
            Some(RoutingToken::Number(value)) => Ok(value),
            _ => Err(invalid("Routing number missing")),
        }
    }
}
pub fn routing_assignment(source: &str) -> Result<Routing> {
    if source.len() > 128 * 1024 || !ROUTING_START.is_match(source) {
        return Err(invalid("Routing assignment missing"));
    }
    let path = ROUTING_PATH
        .captures(source)
        .and_then(|c| c.get(1))
        .map(|v| v.as_str().to_owned())
        .ok_or_else(|| invalid("Routing path missing"))?;
    let default = ROUTING_DEFAULT
        .captures(source)
        .and_then(|c| c[1].parse::<u32>().ok())
        .ok_or_else(|| invalid("Hitomi.la routing configuration is malformed"))?;
    if path.len() > 256 || path.contains("..") || path.contains("//") || default > 999 {
        return Err(invalid("Invalid routing configuration"));
    }
    let switch = ROUTING_SWITCH
        .find(source)
        .ok_or_else(|| invalid("Routing switch missing"))?;
    let (body, remainder) = source[switch.end()..]
        .split_once('}')
        .ok_or_else(|| invalid("Routing switch unclosed"))?;
    if !ROUTING_RETURN.is_match(remainder) {
        return Err(invalid("Routing return missing"));
    }
    let mut routes = vec![default as u16; 4096];
    let mut pending = Vec::new();
    let mut seen = [false; 4096];
    let mut lexer = RoutingLexer(body);
    // One token per label avoids a capture group spanning thousands of cases.
    while let Some(token) = lexer.next()? {
        if token == RoutingToken::Case {
            let key = lexer.number()?;
            lexer.expect(RoutingToken::Colon)?;
            if key > 4095 || seen[key as usize] {
                return Err(invalid("Routing key out of bounds"));
            }
            seen[key as usize] = true;
            pending.push(key);
        } else if token == RoutingToken::Output {
            lexer.expect(RoutingToken::Equals)?;
            let route = lexer.number()?;
            lexer.expect(RoutingToken::Semicolon)?;
            lexer.expect(RoutingToken::Break)?;
            lexer.expect(RoutingToken::Semicolon)?;
            if route > 999 || pending.is_empty() {
                return Err(invalid("Routing result out of bounds"));
            }
            for key in pending.drain(..) {
                routes[key as usize] = route as u16;
            }
        } else {
            return Err(invalid("Unsupported routing statement"));
        }
    }
    Ok(Routing { path, routes })
}
fn cached_routing(value: &Value) -> Result<Routing> {
    let path = value["path"]
        .as_str()
        .ok_or_else(|| invalid("Routing path missing"))?;
    if path.is_empty()
        || path.len() > 256
        || path.contains("..")
        || path.contains("//")
        || !path
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._/-".contains(&c))
    {
        return Err(invalid("Invalid routing path"));
    }
    let encoded = value["routes"]
        .as_str()
        .filter(|v| v.len() == 10924)
        .ok_or_else(|| invalid("Invalid routing table size"))?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| invalid("Invalid routing table"))?;
    if bytes.len() != 8192 {
        return Err(invalid("Invalid routing table size"));
    }
    let routes = bytes
        .chunks_exact(2)
        .map(|v| u16::from_be_bytes([v[0], v[1]]))
        .collect::<Vec<_>>();
    if routes.iter().any(|v| *v > 999) {
        return Err(invalid("Routing result out of bounds"));
    }
    Ok(Routing {
        path: path.to_owned(),
        routes,
    })
}
async fn routing(context: &Context) -> Result<Routing> {
    let value = context
        .remember(
            "hitomi:routing-v2",
            Some(60.0),
            async {
                let response = request(context, &format!("{STATIC}/gg.js"), None, false).await?;
                successful(&response)?;
                let source = String::from_utf8(bytes(&response)?)
                    .map_err(|_| invalid("Invalid routing text"))?;
                let routing = routing_assignment(&source)?;
                let bytes = routing
                    .routes
                    .iter()
                    .flat_map(|route| route.to_be_bytes())
                    .collect::<Vec<_>>();
                Ok(json!({"path":routing.path,"routes":STANDARD.encode(bytes)}))
            },
            |value| cached_routing(value).map(|_| ()),
        )
        .await?;
    cached_routing(&value)
}
fn number(hash: &str) -> u32 {
    u32::from_str_radix(&format!("{}{}", &hash[63..], &hash[61..63]), 16).unwrap()
}
fn page_url(file: &Value, routing: &Routing) -> Result<String> {
    let hash = hash(&file["hash"])?;
    let number = number(hash);
    let route = routing.routes[number as usize];
    let name = text(&file["name"]).to_lowercase();
    let original = name.rsplit('.').next().unwrap_or("");
    let extension = if file["hasavif"] == 1 || file["hasavif"] == "1" {
        "avif"
    } else if original == "gif" {
        "gif"
    } else if file["haswebp"] == 1 || file["haswebp"] == "1" {
        "webp"
    } else if ["jpg", "jpeg", "png", "gif"].contains(&original) {
        original
    } else {
        return Err(invalid("Unsupported image representation"));
    };
    Ok(format!(
        "https://a{}.gold-usergeneratedcontent.net/{}/{number}/{hash}.{extension}",
        route + 1,
        routing.path.trim_matches('/')
    ))
}
fn page_route(file: &Value, routing: &Routing) -> Result<u32> {
    let number = number(hash(&file["hash"])?);
    Ok(routing.routes[number as usize] as u32)
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| invalid("Truncated binary data"))?
            .try_into()
            .unwrap(),
    ))
}
fn u64_at(bytes: &[u8], offset: usize) -> Result<u64> {
    let value = (u32_at(bytes, offset)? as u64) << 32 | u32_at(bytes, offset + 4)? as u64;
    if value > 9_007_199_254_740_991 {
        return Err(invalid("Binary address exceeds range"));
    }
    Ok(value)
}
pub fn nozomi(bytes: &[u8]) -> Result<Vec<u32>> {
    if bytes.len() % 4 != 0 {
        return Err(invalid("Unaligned Nozomi data"));
    }
    bytes
        .chunks_exact(4)
        .map(|v| {
            let id = i32::from_be_bytes(v.try_into().unwrap());
            if id <= 0 {
                Err(invalid("Invalid gallery identifier"))
            } else {
                Ok(id as u32)
            }
        })
        .collect()
}
fn segment(value: &str, colon: bool) -> Result<String> {
    let value = value.replace('_', " ").trim().to_lowercase();
    if value.is_empty()
        || value.len() > 100
        || value.contains("..")
        || !value.bytes().all(|c| {
            c.is_ascii_lowercase()
                || c.is_ascii_digit()
                || b" .+'-".contains(&c)
                || (colon && c == b':')
        })
    {
        return Err(error("Invalid search term", "invalidSearchTerm"));
    }
    Ok(encode_segment(&value).replace("%3A", ":"))
}
async fn ids(
    context: &Context,
    language: &str,
    term: Option<&str>,
    sort: &str,
) -> Result<Vec<u32>> {
    let language = segment(language, false)?;
    let url = if let Some(term) = term {
        if let Some((namespace, value)) = term.split_once(':') {
            let (area, tag) = if matches!(namespace, "male" | "female") {
                ("tag", term)
            } else {
                (namespace, value)
            };
            if namespace == "language" || value.contains(':') {
                return Err(error("Invalid namespaced term", "invalidSearchTerm"));
            }
            format!(
                "{STATIC}/n/{}/{}-{language}.nozomi",
                segment(area, false)?,
                segment(tag, true)?
            )
        } else {
            return title_ids(context, &term.replace('_', " ")).await;
        }
    } else if let Some(period) = sort.strip_prefix("popular-") {
        format!(
            "{STATIC}/n/popular/{}-{language}.nozomi",
            segment(period, false)?
        )
    } else if sort == "date-published" {
        format!("{STATIC}/n/date/published-{language}.nozomi")
    } else {
        format!("{STATIC}/n/index-{language}.nozomi")
    };
    let mut result = vec![];
    let mut start = 0;
    loop {
        let (bytes, total) = nozomi_range(context, &url, start, INDEX_CHUNK_BYTES).await?;
        result.extend(nozomi(&bytes)?);
        start += bytes.len() as u64;
        if start >= total {
            break;
        }
    }
    Ok(result)
}

fn cached_nozomi_range(value: &Value, start: u64, length: u64) -> Result<(Vec<u8>, u64)> {
    let total = value["total"]
        .as_u64()
        .ok_or_else(|| invalid("Invalid Nozomi extent"))?;
    let bytes = STANDARD
        .decode(text(&value["bytes"]))
        .map_err(|_| invalid("Invalid cached Nozomi bytes"))?;
    let expected = total.saturating_sub(start).min(length);
    if total > 16 * 1024 * 1024 || total % 4 != 0 || bytes.len() as u64 != expected {
        return Err(invalid("Inconsistent Nozomi extent"));
    }
    nozomi(&bytes)?;
    Ok((bytes, total))
}

async fn nozomi_range(
    context: &Context,
    url: &str,
    start: u64,
    length: u64,
) -> Result<(Vec<u8>, u64)> {
    let key = format!("hitomi:nozomi-v2:{url}:{start}:{length}");
    let ticket = context.cache_read(&key, Some(300.0)).await?;
    if ticket["hit"] == true {
        if let Ok(range) = cached_nozomi_range(&ticket["value"], start, length) {
            return Ok(range);
        }
    }
    let end = start + length - 1;
    let response = request(context, url, Some((start, end)), false).await?;
    let (data, total) = match status(&response) {
        404 => (vec![], 0),
        416 => {
            let total = header(&response, "content-range")
                .strip_prefix("bytes */")
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or_else(|| invalid("Invalid unsatisfied Nozomi range"))?;
            if start < total {
                return Err(invalid("Unexpected unsatisfied Nozomi range"));
            }
            (vec![], total)
        }
        _ => {
            let mime = response["mimeType"]
                .as_str()
                .unwrap_or_else(|| header(&response, "content-type"))
                .split(';')
                .next()
                .unwrap_or("")
                .trim();
            if !["", "application/octet-stream", "application/x-nozomi"].contains(&mime) {
                return Err(invalid("Invalid Nozomi representation"));
            }
            let data = bytes(&response)?;
            if status(&response) == 206 {
                let total = validate_range(&response, start, end, data.len() as u64)?;
                (data, total)
            } else {
                if data.len() as u64 > INDEX_CHUNK_BYTES {
                    return Err(invalid("Nozomi server ignored the byte range"));
                }
                let total = data.len() as u64;
                let first = (start.min(total)) as usize;
                let last = (start + length).min(total) as usize;
                (data[first..last].to_vec(), total)
            }
        }
    };
    let value = json!({"bytes":STANDARD.encode(data),"total":total});
    let range = cached_nozomi_range(&value, start, length)?;
    context.cache_write(&key, &ticket, &value).await?;
    Ok(range)
}
async fn cards(context: &Context, ids: &[u32], progressive: bool) -> Result<Vec<Value>> {
    let mut seen = HashSet::new();
    let ids: Vec<_> = ids.iter().copied().filter(|id| seen.insert(*id)).collect();
    let mut result = vec![Value::Null; ids.len()];
    let mut remaining = ids.into_iter().enumerate();
    let mut pending = Vec::new();
    // The guest is polled on host-service resumes, without per-future wakeups.
    // Poll all four slots each time and refill immediately on completion.
    let mut published = 0;
    loop {
        let finished = futures::future::poll_fn(|cx| {
            let mut index = 0;
            loop {
                while pending.len() < 4 {
                    let Some((position, id)) = remaining.next() else {
                        break;
                    };
                    pending.push((
                        position,
                        Box::pin(async move {
                            let gallery = gallery_card(context, &id.to_string()).await?;
                            // The loader/cache validator checked this single-origin path.
                            // Keep the exact validated share URL without reparsing it per card.
                            let share = format!("https://hitomi.la{}", text(&gallery["galleryurl"]));
                            work_with_share(&gallery, Some(share))
                        }),
                    ));
                }
                if index >= pending.len() {
                    let ready = result.iter().take_while(|card| !card.is_null()).count();
                    return if pending.is_empty() {
                        Poll::Ready(Ok(true))
                    } else if progressive && ready > published {
                        Poll::Ready(Ok(false))
                    } else {
                        Poll::Pending
                    };
                }
                match std::future::Future::poll(pending[index].1.as_mut(), cx) {
                    Poll::Ready(Ok(card)) => {
                        let (position, _) = pending.swap_remove(index);
                        result[position] = card;
                    }
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Pending => index += 1,
                }
            }
        })
        .await?;
        let ready = result.iter().take_while(|card| !card.is_null()).count();
        if progressive && ready > published {
            // Publish only the ready prefix so title positions never change as
            // slower cards arrive. The final response still owns pagination.
            context.call("catalogProgress", json!({"items": &result[..ready]})).await?;
            published = ready;
        }
        if finished { break; }
    }
    Ok(result)
}
fn validate_range(response: &Value, start: u64, end: u64, length: u64) -> Result<u64> {
    if !["", "identity"].contains(&header(response, "content-encoding")) {
        return Err(invalid("Compressed byte range"));
    }
    static RANGE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^bytes ([0-9]+)-([0-9]+)/([0-9]+)$").unwrap());
    let captures = RANGE
        .captures(header(response, "content-range"))
        .ok_or_else(|| invalid("Missing byte range"))?;
    let first = captures[1]
        .parse::<u64>()
        .map_err(|_| invalid("Invalid byte range"))?;
    let last = captures[2]
        .parse::<u64>()
        .map_err(|_| invalid("Invalid byte range"))?;
    let total = captures[3]
        .parse::<u64>()
        .map_err(|_| invalid("Invalid byte range"))?;
    if first != start
        || last < first
        || last > end
        || total <= last
        || last - first + 1 != length
        || total > 9_007_199_254_740_991
    {
        return Err(invalid("Inconsistent byte range"));
    }
    Ok(total)
}
#[derive(Debug)]
pub struct Node {
    keys: Vec<Vec<u8>>,
    data: Vec<(u64, u64)>,
    children: Vec<u64>,
}
pub fn decode_node(bytes: &[u8]) -> Result<Node> {
    let count = u32_at(bytes, 0)? as usize;
    if count > 16 {
        return Err(invalid("Invalid B-tree key count"));
    }
    let mut offset = 4;
    let mut keys = vec![];
    for _ in 0..count {
        let length = u32_at(bytes, offset)? as usize;
        offset += 4;
        if !(1..=32).contains(&length) {
            return Err(invalid("Invalid B-tree key"));
        }
        keys.push(
            bytes
                .get(offset..offset + length)
                .ok_or_else(|| invalid("Truncated B-tree key"))?
                .to_vec(),
        );
        offset += length;
    }
    if keys.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(invalid("Unordered B-tree keys"));
    }
    if u32_at(bytes, offset)? as usize != count {
        return Err(invalid("B-tree data count mismatch"));
    }
    offset += 4;
    let mut data = vec![];
    for _ in 0..count {
        let address = u64_at(bytes, offset)?;
        let length = u32_at(bytes, offset + 8)? as u64;
        if length == 0 || length > 100_000_000 {
            return Err(invalid("Invalid B-tree data length"));
        }
        data.push((address, length));
        offset += 12;
    }
    let mut children = vec![];
    for _ in 0..17 {
        children.push(u64_at(bytes, offset)?);
        offset += 8;
    }
    Ok(Node {
        keys,
        data,
        children,
    })
}
async fn index_range(
    context: &Context,
    version: &str,
    suffix: &str,
    address: u64,
    length: u64,
) -> Result<Vec<u8>> {
    let key = format!("hitomi:index-v2:{version}:{suffix}:{address}:{length}");
    let value = context
        .remember(
            &key,
            None,
            async {
                let response = request(
                    context,
                    &format!("{STATIC}/galleriesindex/galleries.{version}.{suffix}"),
                    Some((address, address + length - 1)),
                    false,
                )
                .await?;
                let received = bytes(&response)?;
                let received = if status(&response) == 200 {
                    received
                        .get(address as usize..(address + length) as usize)
                        .map(|v| v.to_vec())
                        .ok_or_else(|| invalid("Incomplete index representation"))?
                } else {
                    if status(&response) != 206 || received.len() as u64 != length {
                        return Err(invalid("Invalid index byte range"));
                    }
                    validate_range(&response, address, address + length - 1, length)?;
                    received
                };
                Ok(json!(STANDARD.encode(received)))
            },
            |value| {
                let decoded = STANDARD
                    .decode(text(value))
                    .map_err(|_| invalid("Invalid cached index"))?;
                if decoded.len() as u64 != length {
                    return Err(invalid("Invalid cached index length"));
                }
                if suffix == "index" {
                    decode_node(&decoded)?;
                }
                // Data ranges can be fragments of one posting list. Its count
                // and gallery IDs are validated after reassembling all chunks.
                Ok(())
            },
        )
        .await?;
    STANDARD
        .decode(text(&value))
        .map_err(|_| invalid("Invalid index encoding"))
}
async fn index_bytes(
    context: &Context,
    version: &str,
    suffix: &str,
    address: u64,
    length: u64,
) -> Result<Vec<u8>> {
    if length > 16 * 1024 * 1024 {
        return Err(invalid("Index response exceeds resource limit"));
    }
    let mut result = Vec::with_capacity(length as usize);
    while result.len() < length as usize {
        let offset = result.len() as u64;
        result.extend(
            index_range(
                context,
                version,
                suffix,
                address + offset,
                (length - offset).min(INDEX_CHUNK_BYTES),
            )
            .await?,
        );
    }
    Ok(result)
}
async fn title_ids(context: &Context, term: &str) -> Result<Vec<u32>> {
    let version = cached_text(
        context,
        "hitomi:index-version",
        &format!("{STATIC}/galleriesindex/version"),
        1800.0,
    )
    .await?
    .trim()
    .to_owned();
    if version.is_empty() || version.len() > 20 || !version.bytes().all(|v| v.is_ascii_digit()) {
        return Err(invalid("Invalid index version"));
    }
    let digest = Sha256::digest(term.as_bytes());
    let key = &digest[..4];
    let mut address = 0;
    for _ in 0..64 {
        let node = decode_node(&index_bytes(context, &version, "index", address, 464).await?)?;
        let position = node.keys.partition_point(|value| value.as_slice() < key);
        if position < node.keys.len() && node.keys[position] == key {
            let (address, length) = node.data[position];
            let bytes = index_bytes(context, &version, "data", address, length).await?;
            let result = (|| {
                let count = u32_at(&bytes, 0)? as usize;
                if count == 0 || count > 10_000_000 || bytes.len() != count * 4 + 4 {
                    return Err(invalid("Malformed title index data"));
                }
                nozomi(&bytes[4..])
            })();
            if result.is_err() {
                // Fragments cannot be validated independently. Evict them if
                // aggregate validation fails so a corrected response can retry.
                for offset in (0..length).step_by(INDEX_CHUNK_BYTES as usize) {
                    let key = format!("hitomi:index-v2:{version}:data:{}:{}", address + offset, (length - offset).min(INDEX_CHUNK_BYTES));
                    context.call("cache", json!({"operation":"remove","key":key})).await?;
                }
            }
            return result;
        }
        if node.children.iter().all(|v| *v == 0) {
            return Ok(vec![]);
        }
        address = node.children[position];
        if address == 0 {
            return Err(invalid("Invalid B-tree child"));
        }
    }
    Err(invalid("B-tree depth exceeded"))
}
fn catalog_cursor(args: &Value) -> Option<&Value> {
    args.get("metadata")
        .filter(|value| !value.is_null())
        .or_else(|| args.get("cursor").filter(|value| !value.is_null()))
}
fn catalog_offset(args: &Value) -> Result<u64> {
    let cursor = catalog_cursor(args);
    if let Some(offset) = cursor.and_then(|value| value.get("offset")) {
        return offset
            .as_u64()
            .filter(|value| *value <= MAX_CATALOG_OFFSET)
            .ok_or_else(|| error("Invalid catalog offset", "invalidCursor"));
    }
    // Legacy page cursors and current offset cursors both count consumed IDs.
    Ok((page(&json!({"metadata":cursor}), 100_000)? - 1) * 25)
}

async fn sorted_search_page(
    context: &Context,
    language: &str,
    sort: &str,
    allowed: HashSet<u32>,
    offset: u64,
    progressive: bool,
) -> Result<Value> {
    let path = if let Some(period) = sort.strip_prefix("popular-") {
        format!("popular/{}", segment(period, false)?)
    } else if sort == "date-published" {
        "date/published".into()
    } else {
        "index".into()
    };
    let url = format!("{STATIC}/n/{path}-{}.nozomi", segment(language, false)?);
    let mut start = 0;
    let mut skipped = 0;
    let mut page = Vec::new();
    let mut remaining = allowed.clone();
    while !remaining.is_empty() {
        let (bytes, total) = nozomi_range(context, &url, start, INDEX_CHUNK_BYTES).await?;
        start += bytes.len() as u64;
        // Intersect in catalog order and stop after one page plus a lookahead.
        // Do not materialize the entire language catalog for every title query.
        for id in nozomi(&bytes)? {
            if !allowed.contains(&id) { continue; }
            remaining.remove(&id);
            if skipped < offset { skipped += 1; continue; }
            page.push(id);
            if page.len() > CATALOG_PAGE_SIZE as usize { break; }
        }
        if page.len() > CATALOG_PAGE_SIZE as usize || start >= total { break; }
    }
    let has_next = page.len() > CATALOG_PAGE_SIZE as usize;
    page.truncate(CATALOG_PAGE_SIZE as usize);
    Ok(json!({"items":cards(context, &page, progressive).await?,
        "metadata":if has_next { json!({"offset":offset+CATALOG_PAGE_SIZE}) } else { Value::Null }}))
}
async fn search(context: &Context, args: &Value) -> Result<Value> {
    validate_search(&filters(), args)?;
    let offset = catalog_offset(args)?;
    let progressive = args["catalogProgress"].as_bool().unwrap_or(false);
    let sort = args["sort"].as_str().unwrap_or("newest");
    let mut raw = text(args.get("query").unwrap_or(&args["text"]))
        .trim()
        .to_lowercase();
    for selection in args["selections"].as_array().into_iter().flatten() {
        raw.push_str(&format!(
            " {}{}:{}",
            if selection["polarity"] == "exclude" {
                "-"
            } else {
                ""
            },
            text(&selection["fieldID"]),
            text(&selection["value"])
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("_")
        ));
    }
    let raw = raw.trim();
    if !raw.is_empty() && raw.bytes().all(|v| v.is_ascii_digit()) {
        if offset > 0 {
            return Ok(json!({"items":[],"metadata":null}));
        }
        return Ok(
            json!({"items":cards(context,&[raw.parse::<u32>().map_err(|_| error("Invalid gallery ID", "invalidIdentifier"))?], progressive).await?,"metadata":null}),
        );
    }
    let mut language = "english".to_owned();
    let mut positive = vec![];
    let mut negative = vec![];
    for term in raw.split_whitespace() {
        if let Some(lang) = term.strip_prefix("language:") {
            if !LANGUAGES.contains(&lang) {
                return Err(error("Unsupported language", "invalidSearchTerm"));
            }
            language = lang.into();
        } else if let Some(term) = term.strip_prefix('-') {
            if term.is_empty() {
                return Err(error("Empty negative term", "invalidSearchTerm"));
            }
            negative.push(term);
        } else {
            positive.push(term);
        }
    }
    if positive.is_empty() && negative.is_empty() && sort != "random" {
        let path = if let Some(period) = sort.strip_prefix("popular-") {
            format!("popular/{period}")
        } else if sort == "date-published" {
            "date/published".into()
        } else {
            "index".into()
        };
        let window_offset = offset / CATALOG_INDEX_SIZE * CATALOG_INDEX_SIZE;
        let start = window_offset * 4;
        let (bytes, total) = nozomi_range(
            context,
            &format!("{STATIC}/n/{path}-{language}.nozomi"),
            start,
            CATALOG_INDEX_SIZE * 4,
        )
        .await?;
        let ids = nozomi(&bytes)?;
        let relative_offset = (offset - window_offset) as usize;
        let mut seen: HashSet<_> = ids.iter().take(relative_offset).copied().collect();
        // Complete this cached 25-ID window, including legacy unaligned offsets.
        let ids: Vec<_> = ids
            .into_iter()
            .skip(relative_offset)
            .take((CATALOG_PAGE_SIZE - offset % CATALOG_PAGE_SIZE) as usize)
            .collect();
        let next_offset = offset + ids.len() as u64;
        let has_next = !ids.is_empty() && next_offset * 4 < total;
        // Cursor positions count consumed IDs, including duplicates. Never reuse
        // a gallery already consumed within this bounded index window.
        let ids: Vec<_> = ids.into_iter().filter(|id| seen.insert(*id)).collect();
        return Ok(
            json!({"items":cards(context,&ids, progressive).await?,"metadata":if has_next {json!({"offset":next_offset})} else {Value::Null}}),
        );
    }
    let mut result = if let Some(term) = positive.first() {
        ids(context, &language, Some(term), sort).await?
    } else {
        ids(context, &language, None, sort).await?
    };
    for term in positive.iter().skip(1) {
        let allowed: HashSet<_> = ids(context, &language, Some(term), sort)
            .await?
            .into_iter()
            .collect();
        result.retain(|id| allowed.contains(id));
    }
    for term in negative {
        let blocked: HashSet<_> = ids(context, &language, Some(term), sort)
            .await?
            .into_iter()
            .collect();
        result.retain(|id| !blocked.contains(id));
    }
    if !positive.is_empty() && sort != "random" {
        return sorted_search_page(context, &language, sort, result.into_iter().collect(), offset, progressive).await;
    }
    let mut seed = 0u32;
    if sort == "random" {
        seed = if offset == 0 {
            context.now().await?["random"].as_u64().unwrap_or(1) as u32
        } else {
            catalog_cursor(args)
                .and_then(|v| v["randomSeed"].as_u64())
                .filter(|v| *v > 0 && *v <= u32::MAX as u64)
                .ok_or_else(|| invalid("Hitomi.la random pagination seed is invalid"))?
                as u32
        };
        let mut state = seed;
        for index in (1..result.len()).rev() {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let target = ((state as u64 * (index + 1) as u64) >> 32) as usize;
            result.swap(index, target);
        }
    }
    let start = offset as usize;
    let items = cards(
        context,
        &result
            .iter()
            .skip(start)
            .take(CATALOG_PAGE_SIZE as usize)
            .copied()
            .collect::<Vec<_>>(),
        progressive,
    )
    .await?;
    Ok(
        json!({"items":items,"metadata":if start+(CATALOG_PAGE_SIZE as usize)<result.len() {let mut value=json!({"offset":offset+CATALOG_PAGE_SIZE});if sort=="random" {value["randomSeed"]=json!(seed);}value} else {Value::Null}}),
    )
}
pub fn run(context: Context, method: String, mut args: Value) -> Operation {
    Box::pin(async move {
        match method.as_str() {
            "initialize" | "invalidateCache" => Ok(Value::Null),
            "settings" => Ok(json!({"id":"settings","title":"Hitomi.la","fields":[]})),
            "searchFilters" => Ok(filters()),
            "discoverSections" => Ok(json!([
                {"id":"latest","title":"Newest","type":0,"presentation":"carousel"},
                {"id":"date-published","title":"Date Published","type":0,"presentation":"carousel"},
                {"id":"popular-today","title":"Popular Today","type":0,"presentation":"carousel"},
                {"id":"popular-week","title":"Popular This Week","type":0,"presentation":"carousel"},
                {"id":"popular-month","title":"Popular This Month","type":0,"presentation":"carousel"},
                {"id":"popular-year","title":"Popular This Year","type":0,"presentation":"carousel"},
                {"id":"random","title":"Random","type":0,"presentation":"carousel"}
            ])),
            "search" | "discover" => {
                if method == "discover" {
                    let section = args["sectionId"]
                        .as_str()
                        .or(args["section"]["id"].as_str())
                        .unwrap_or("latest");
                    let section_sort = match section {
                        "latest" => "newest",
                        "popular" => "popular-week",
                        "date-published" | "popular-today" | "popular-week" | "popular-month"
                        | "popular-year" | "random" => section,
                        _ => return Err(error("Unknown section", "invalidSection")),
                    };
                    if args["sort"].is_null() {
                        args["sort"] = json!(section_sort);
                    }
                }
                search(&context, &args).await
            }
            "details" => work(&gallery(&context, &args).await?),
            "installments" => {
                let id = id(args.get("workId").unwrap_or(&args["id"]))?;
                let source = if args["files"].as_array().is_some_and(|v| !v.is_empty()) {
                    args
                } else {
                    gallery(&context, &json!(id)).await?
                };
                Ok(
                    json!([{"installmentId":format!("gallery:{id}"),"workId":id,"langCode":source["language"],"number":1,"volume":1,"title":"Gallery","publishDate":source.get("publishedAt").unwrap_or(&source["date"])}]),
                )
            }
            "imagePages" => {
                let id = id(args.get("workId").unwrap_or(&json!(
                    text(&args["installmentId"]).trim_start_matches("gallery:")
                )))?;
                let source = if args["files"].as_array().is_some_and(|v| !v.is_empty()) {
                    args
                } else {
                    gallery(&context, &json!(id)).await?
                };
                let routing = routing(&context).await?;
                let files = source["files"]
                    .as_array()
                    .ok_or_else(|| invalid("Missing files"))?;
                let pages: Vec<String> = files
                    .iter()
                    .map(|f| page_url(f, &routing))
                    .collect::<Result<_>>()?;
                if files
                    .iter()
                    .map(|file| page_route(file, &routing))
                    .collect::<Result<HashSet<_>>>()?
                    .len()
                    > 16
                {
                    return Err(invalid("Too many image origins"));
                }
                Ok(json!({"id":format!("gallery:{id}"),"workId":id,"pages":pages}))
            }
            "imagePageContent" => {
                let url = url::Url::parse(text(args.get("url").unwrap_or(&args["pageURL"])))
                    .map_err(|_| invalid("Invalid image URL"))?;
                if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some()
                {
                    return Err(invalid("Image URLs require HTTPS without credentials"));
                }
                let cover = url.host_str() == Some("atn.gold-usergeneratedcontent.net")
                    && Regex::new(r"^/avif(?:small)?bigtn/[0-9a-f]/[0-9a-f]{2}/[0-9a-f]{64}\.avif$")
                        .unwrap()
                        .is_match(url.path());
                if !cover {
                    let pattern = Regex::new(
                        r"^/([A-Za-z0-9._/-]+)/([0-9]+)/([0-9a-f]{64})\.(avif|webp|gif|jpe?g|png)$",
                    )
                    .unwrap();
                    let captures = pattern
                        .captures(url.path())
                        .ok_or_else(|| invalid("Invalid image path"))?;
                    let number = number(&captures[3]);
                    if captures[2] != number.to_string() {
                        return Err(invalid("Invalid image hash route"));
                    }
                    let routing = routing(&context).await?;
                    let expected = format!(
                        "https://a{}.gold-usergeneratedcontent.net",
                        routing.routes[number as usize] + 1
                    );
                    if url.origin().ascii_serialization() != expected
                        || captures[1] != *routing.path.trim_matches('/')
                    {
                        return Err(error("Invalid Hitomi.la image URL", "invalidIdentifier"));
                    }
                }
                let response = request(&context, url.as_str(), None, true).await?;
                successful(&response)?;
                image(&response)
            }
            "searchSuggestions" => {
                let field = args["fieldID"].as_str().unwrap_or("global");
                if field != "global" && !FIELDS.iter().any(|(id, _)| *id == field) {
                    return Err(error("Unknown suggestion field", "invalidSearchTerm"));
                }
                let query = text(&args["query"]).trim();
                if query.is_empty() {
                    return Ok(json!([]));
                }
                let key = format!("hitomi:suggestions:{field}:{query}");
                let values = context
                    .remember(
                        &key,
                        Some(300.0),
                        async {
                            let mut prefix = query.to_owned();
                            loop {
                                let segments = prefix
                                    .chars()
                                    .map(|c| {
                                        encode_segment(match c {
                                            ' ' => "_",
                                            '/' => "slash",
                                            '.' => "dot",
                                            _ => return encode_segment(&c.to_string()),
                                        })
                                    })
                                    .collect::<Vec<_>>()
                                    .join("/");
                                let response = request(
                                    &context,
                                    &format!("https://tagindex.hitomi.la/{field}/{segments}.json"),
                                    None,
                                    false,
                                )
                                .await?;
                                if status(&response) == 404 {
                                    if prefix.chars().count() <= 2 {
                                        return Ok(json!([]));
                                    }
                                    prefix.pop();
                                    continue;
                                }
                                let bytes = bytes(&response)?;
                                if bytes.len() > 256 * 1024 {
                                    return Err(invalid("Suggestions too large"));
                                }
                                return serde_json::from_slice::<Value>(&bytes)
                                    .map_err(|_| invalid("Malformed suggestions"));
                            }
                        },
                        |v| {
                            let values = v
                                .as_array()
                                .ok_or_else(|| invalid("Malformed suggestion list"))?;
                            for value in values.iter().take(100) {
                                if value.as_array().is_none()
                                    || text(&value[0]).trim().is_empty()
                                    || !value[2].is_string()
                                {
                                    return Err(invalid("Malformed suggestion"));
                                }
                            }
                            Ok(())
                        },
                    )
                    .await?;
                let mut result = vec![];
                for value in values
                    .as_array()
                    .ok_or_else(|| invalid("Malformed suggestion list"))?
                    .iter()
                    .take(100)
                {
                    if value.as_array().is_none()
                        || text(&value[0]).trim().is_empty()
                        || !value[2].is_string()
                    {
                        return Err(invalid("Malformed suggestion"));
                    }
                    let namespace = match text(&value[2]) {
                        "tags" => "tag",
                        "artists" => "artist",
                        "groups" => "group",
                        "parody" | "parodys" => "series",
                        "characters" => "character",
                        "languages" => "language",
                        "types" => "type",
                        v => v,
                    };
                    if FIELDS.iter().any(|(id, _)| *id == namespace) {
                        result.push(json!({"fieldID":namespace,"value":value[0],"title":value[0],"subtitle":format!("{} galleries",value[1])}));
                    }
                }
                Ok(suggestions(&args, result))
            }
            "updates" => Ok(json!({"items":[],"metadata":null})),
            "managedCollections" => Ok(json!([])),
            _ => Err(error("Unsupported operation", "unsupportedOperation")),
        }
    })
}
export_extension!("HitomiLA", "content", run);
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routing_is_data_not_code() {
        let route=routing_assignment("var gg={b:'route/',m:function(g){var o=0;switch(g){case 1:case 2:o=1;break;}return o;}}").unwrap();
        assert_eq!(route.routes[1], 1);
        assert!(routing_assignment("gg={b:'../',m:function(g){var o=0;}}").is_err());
        assert!(gallery_assignment("var galleryinfo={}; alert(1)").is_err());
    }
    #[test]
    fn routing_large_switch_is_bounded_and_validated() {
        let cases = (0..4096)
            .map(|key| format!("case {key}:\n"))
            .collect::<String>();
        let source = format!(
            "gg={{m:function(g){{var o=1;switch(g){{{cases}o=0;break;}}return o;}},b:'route/'}};"
        );
        let route = routing_assignment(&source).unwrap();
        assert_eq!(route.routes.len(), 4096);
        assert_eq!(route.routes[4095], 0);
        assert!(routing_assignment(&source.replace("case 4095:", "case 4096:")).is_err());
        assert!(routing_assignment(&source.replace("o=0;break;", "o=1000;break;")).is_err());
        assert!(routing_assignment(&source.replace("o=0;break;", "o=compute();break;")).is_err());
        assert!(routing_assignment(&source.replace("case 4095:", "case 1:")).is_err());
    }
    #[test]
    fn compact_routing_cache_is_validated() {
        let route = routing_assignment(
            "gg={b:'route/',m:function(g){var o=1;switch(g){case 1:o=0;break;}return o;}}",
        )
        .unwrap();
        let bytes = route
            .routes
            .iter()
            .flat_map(|route| route.to_be_bytes())
            .collect::<Vec<_>>();
        let value = json!({"path":route.path,"routes":STANDARD.encode(&bytes)});
        assert_eq!(cached_routing(&value).unwrap(), route);
        assert!(cached_routing(&json!({"path":"../","routes":value["routes"]})).is_err());
        assert!(
            cached_routing(&json!({"path":"route/","routes":STANDARD.encode(&bytes[..8190])}))
                .is_err()
        );
        let mut bad = bytes;
        bad[0..2].copy_from_slice(&1000u16.to_be_bytes());
        assert!(cached_routing(&json!({"path":"route/","routes":STANDARD.encode(bad)})).is_err());
    }
    #[test]
    fn binary_data_is_bounded() {
        assert_eq!(nozomi(&[0, 0, 0, 1]).unwrap(), vec![1]);
        assert!(nozomi(&[0xff, 0xff, 0xff, 0xff]).is_err());
        assert!(decode_node(&[0, 0, 0, 17]).is_err());
    }
    #[test]
    fn captured_cards_match_gallery_covers() {
        for (id, html, metadata) in [
            (
                "4247333",
                include_str!("../tests/fixtures/4247333-card.html"),
                include_str!("../tests/fixtures/4247333-gallery.json"),
            ),
            (
                "4247316",
                include_str!("../tests/fixtures/4247316-card.html"),
                include_str!("../tests/fixtures/4247316-gallery.json"),
            ),
        ] {
            let gallery: Value = serde_json::from_str(metadata).unwrap();
            let card = card_assignment(html, id).unwrap();
            assert_eq!(card["title"], gallery["title"]);
            assert_eq!(card["files"][0]["hash"], gallery["files"][0]["hash"]);
            assert_eq!(
                work(&card).unwrap()["coverURL"],
                work(&gallery).unwrap()["coverURL"]
            );
            assert_eq!(card["language"], gallery["language"]);
            assert!(card_assignment(html, "1").is_err());
        }
    }
    #[test]
    fn cards_handle_entities_optional_fields_and_reject_bad_identity_or_covers() {
        let html = include_str!("../tests/fixtures/4247333-card.html");
        let start = html.find("<h1").unwrap();
        let end = html[start..].find("</h1>").unwrap() + start + 5;
        let minimal = format!(
            "{}<h1><a href='/galleries/4247333.html'>日本語 &amp; &#x1F431;</a></h1>",
            &html[..start]
        );
        let card = card_assignment(&minimal, "4247333").unwrap();
        assert_eq!(card["title"], "日本語 & 🐱");
        assert_eq!(card["language"], "unknown");
        assert_eq!(card["tags"], json!([]));
        assert_eq!(card["files"].as_array().unwrap().len(), 1);
        let metadata = card_assignment(
            &format!(
                "{minimal}{}",
                r#"<div class="artist-list"><a>A &amp; B</a></div>
                <a href="/group/sample-all.html">Sample group</a>
                <div class="series-list"><a>Sample series</a></div>
                <a href="/character/sample-all.html">Sample character</a>
                <a href="/index-japanese.html">日本語</a>
                <p class="date">2026-01-01 00:00:00+00</p>
                <div class="relatedtags"><a href="/tag/male%3Asample-all.html">sample</a>
                <a href="/tag/space%20travel-all.html">space travel</a></div>"#
            ),
            "4247333",
        )
        .unwrap();
        assert_eq!(metadata["artists"][0]["artist"], "A & B");
        assert_eq!(metadata["groups"][0]["group"], "Sample group");
        assert_eq!(metadata["parodys"][0]["parody"], "Sample series");
        assert_eq!(metadata["characters"][0]["character"], "Sample character");
        assert_eq!(metadata["language"], "japanese");
        assert_eq!(metadata["date"], "2026-01-01 00:00:00+00");
        assert_eq!(metadata["tags"][0], json!({"tag":"sample", "male":1}));
        assert_eq!(metadata["tags"][1], json!({"tag":"space travel"}));
        assert!(card_assignment(&html.replace("dj-img1", "wrong-image"), "4247333").is_err());
        assert!(card_assignment(
            &html.replace("tn.gold-usergeneratedcontent.net", "evil.example"),
            "4247333"
        )
        .is_err());
        assert!(
            card_assignment(&format!("{}{}", &html[..start], &html[end..]), "4247333").is_err()
        );
        assert!(card_assignment("<html>Verification required</html>", "4247333").is_err());
        for path in [
            "//evil.example/a-1.html",
            "/a-1.html?x=1",
            "/a-1.html#x",
            "/a-2.html",
            "/a\\b-1.html",
        ] {
            assert!(card_path(path, "1").is_err());
        }
        let tags = card_assignment(html, "4247333").unwrap()["tags"].clone();
        assert!(tags
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag["tag"] == "big breasts" && tag["female"] == 1));
    }
    #[test]
    fn catalog_projection_retains_only_the_cover_and_details_validate_all_files() {
        let mut gallery = json!({"id":"1","title":"Fixture","files":(0..1070).map(|_| json!({"hash":"a".repeat(64),"name":"1.png"})).collect::<Vec<_>>()});
        let source = format!("var galleryinfo={gallery};");
        let compact = gallery_card_assignment(&source).unwrap();
        assert_eq!(compact["files"].as_array().unwrap().len(), 1);
        assert_eq!(work(&compact).unwrap(), work(&gallery).unwrap());
        gallery["files"][1069]["hash"] = json!("invalid");
        let source = format!("var galleryinfo={gallery};");
        assert!(gallery_card_assignment(&source).is_ok());
        assert!(validate_gallery(&gallery_assignment(&source).unwrap(), "1").is_err());
        gallery["files"][0]["hash"] = json!("invalid");
        assert!(gallery_card_assignment(&format!("var galleryinfo={gallery};")).is_err());
        assert!(gallery_card_assignment("var galleryinfo={\"files\":null};").is_err());
        assert!(
            gallery_card_assignment("var galleryinfo={\"files\":[{\"hash\":\"invalid\"}]};")
                .is_err()
        );
        assert!(gallery_card_assignment(&source.replace("invalid", "bad\"json")).is_err());
    }
    #[test]
    fn catalog_cursors_preserve_legacy_offsets() {
        assert_eq!(catalog_offset(&json!({})).unwrap(), 0);
        assert_eq!(
            catalog_offset(&json!({"metadata":{"offset":5}})).unwrap(),
            5
        );
        assert_eq!(
            catalog_offset(&json!({"cursor":{"offset":30}})).unwrap(),
            30
        );
        assert_eq!(catalog_offset(&json!({"metadata":{"page":2}})).unwrap(), 25);
        assert_eq!(catalog_offset(&json!({"metadata":{"page":3}})).unwrap(), 50);
        assert_eq!(
            catalog_offset(&json!({"metadata":null,"cursor":{"page":2}})).unwrap(),
            25
        );
        assert_eq!(
            catalog_offset(&json!({"metadata":null,"cursor":{"offset":5}})).unwrap(),
            5
        );
        for value in [
            json!(-1),
            json!(1.5),
            json!("5"),
            json!(null),
            json!(MAX_CATALOG_OFFSET + 1),
        ] {
            assert!(catalog_offset(&json!({"metadata":{"offset":value}})).is_err());
        }
    }
    #[test]
    fn cached_ranges_validate_alignment_and_extent() {
        let value = json!({"total":104,"bytes":base64::engine::general_purpose::STANDARD.encode([0,0,0,26])});
        assert_eq!(cached_nozomi_range(&value, 100, 100).unwrap().0.len(), 4);
        assert!(cached_nozomi_range(&value, 0, 100).is_err());
        assert!(
            cached_nozomi_range(&json!({"total":103,"bytes":value["bytes"]}), 100, 100).is_err()
        );
        assert!(cached_nozomi_range(&json!({"total":104,"bytes":"/////w=="}), 100, 100).is_err());
        assert!(cached_nozomi_range(&json!({"total":100,"bytes":""}), 100, 100).is_ok());
    }
}
