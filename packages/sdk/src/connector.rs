// Copyright 2026 Manko Contributors; SPDX-License-Identifier: Apache-2.0
use crate::{decode_json, error, successful, text, Context, Result};
use serde_json::{json, Value};
use url::Url;

pub fn base(input: &Value) -> Result<Url> {
    let url = Url::parse(text(&input["baseURL"])).map_err(|_| error("Invalid server address", "invalidResponse"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err(error("Invalid server address", "invalidResponse"));
    }
    Ok(url)
}

pub fn endpoint(input: &Value, path: &[&str], query: &[(&str, &str)]) -> Result<Url> {
    let mut url = base(input)?;
    url.set_query(None);
    url.set_fragment(None);
    url.path_segments_mut().map_err(|_| error("Invalid server path", "invalidResponse"))?.pop_if_empty().extend(path);
    url.query_pairs_mut().extend_pairs(query.iter().copied());
    Ok(url)
}

pub fn option<'a>(input: &'a Value, key: &str) -> &'a str {
    input["query"]["filters"].get(key).or_else(|| input["options"].get(key)).map(text).unwrap_or("")
}

pub fn numeric(value: &Value) -> Result<String> {
    let value = value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string());
    if value.is_empty() || value.len() > 20 || !value.bytes().all(|v| v.is_ascii_digit()) || value.parse::<u64>().is_err() {
        return Err(error("Invalid server identifier", "invalidIdentifier"));
    }
    Ok(value)
}

pub async fn get(context: &Context, url: &Url) -> Result<Value> {
    let response = context.request(url.as_str(), "GET", json!({"Accept":"application/json"}), None, false).await?;
    successful(&response)?;
    decode_json(&response)
}

pub async fn post(context: &Context, url: &Url, body: Value) -> Result<Value> {
    let bytes = serde_json::to_vec(&body).map_err(|_| error("Invalid server request", "invalidResponse"))?;
    let response = context.request(url.as_str(), "POST", json!({"Accept":"application/json","Content-Type":"application/json"}), Some(bytes), false).await?;
    successful(&response)?;
    decode_json(&response)
}

pub fn page(items: Vec<Value>, next: Option<u64>, total: Option<u64>) -> Value {
    json!({"items":items,"nextPage":next,"totalCount":total})
}

pub fn kind(tags: &[String], fallback: &str) -> String {
    let hints = tags.join(" ").to_lowercase();
    if hints.contains("novel") || hints.contains("epub") { "lightNovel" }
    else if hints.contains("manga") { "manga" }
    else if hints.contains("comic") || hints.contains("cbz") || hints.contains("zip") { "comic" }
    else { fallback }.into()
}

pub fn format(mime: &str, path: &str) -> Option<&'static str> {
    let mime = mime.to_lowercase();
    let path = path.split(['?', '#']).next().unwrap_or("").to_lowercase();
    if mime.contains("epub") || path.ends_with(".epub") { Some("epub") }
    else if mime.contains("pdf") || path.ends_with(".pdf") { Some("pdf") }
    else if mime.contains("comicbook+zip") || path.ends_with(".cbz") || path.ends_with(".zip") { Some("cbz") }
    else { None }
}
