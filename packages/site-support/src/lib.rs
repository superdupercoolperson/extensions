// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
use base64::{engine::general_purpose::STANDARD, Engine};
use manko_sdk::{bytes, content, error, header, https, status, text, Context, Result};
use scraper::{ElementRef, Html, Node, Selector};
use serde_json::{json, Value};
use std::collections::HashMap;

pub use scraper;
thread_local! {
    static SELECTORS: std::cell::RefCell<HashMap<String, Selector>> = std::cell::RefCell::new(HashMap::new());
}
pub fn invalid() -> Value {
    error(
        "The site returned incomplete or malformed content",
        "invalidResponse",
    )
}
pub fn selector(value: &str) -> Selector {
    SELECTORS.with(|cache| {
        cache
            .borrow_mut()
            .entry(value.to_owned())
            .or_insert_with(|| Selector::parse(value).expect("static selector"))
            .clone()
    })
}
pub fn plain(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn field(document: &Html, query: &str) -> String {
    document
        .select(&selector(query))
        .next()
        .map(plain)
        .unwrap_or_default()
}
pub fn attr(document: &Html, query: &str, name: &str) -> String {
    document
        .select(&selector(query))
        .next()
        .and_then(|e| e.value().attr(name))
        .unwrap_or("")
        .to_owned()
}
pub fn slug(value: &str) -> Result<&str> {
    if value.is_empty()
        || value.len() > 512
        || value == "."
        || value == ".."
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err(error("Invalid identifier", "invalidIdentifier"));
    }
    Ok(value)
}
pub fn link(value: &str, base: &str, hosts: &[&str]) -> Result<String> {
    let value = if value.starts_with("//") {
        format!("https:{value}")
    } else {
        url::Url::parse(base)
            .map_err(|_| invalid())?
            .join(value)
            .map_err(|_| invalid())?
            .to_string()
    };
    let url = https(&value, hosts)?;
    if url.port_or_known_default() != Some(443) {
        return Err(error("Unexpected HTTPS port", "hostNotAllowed"));
    }
    Ok(url.to_string())
}
pub async fn request(
    context: &Context,
    url: &str,
    referer: &str,
    hosts: &[&str],
    agent: &str,
    image: bool,
) -> Result<Value> {
    link(url, referer, hosts)?;
    let response = context.request(url,"GET",json!({"User-Agent":agent,"Referer":referer,"Accept":if image {"image/avif,image/webp,image/*"} else {"text/html,application/json"}}),None,image).await?;
    if let Some(final_url) = response["url"].as_str() {
        link(final_url, referer, hosts)?;
    }
    if status(&response) == 429 {
        let mut failure = error("The site is busy; retry after its cooldown", "rateLimited");
        if let Ok(seconds) = header(&response, "retry-after").parse::<u64>() {
            failure["retryAfterSeconds"] = json!(seconds.clamp(1, 3600));
        }
        return Err(failure);
    }
    let challenge = header(&response, "cf-mitigated").eq_ignore_ascii_case("challenge")
        || (!image
            && matches!(status(&response), 403 | 503)
            && bytes(&response).is_ok_and(|b| {
                let s = String::from_utf8_lossy(&b);
                s.contains("cf-chl-") || s.contains("Just a moment...")
            }));
    if challenge {
        return context
            .call(
                "challenge",
                json!({"url":url,"reason":"Complete the site's visible verification"}),
            )
            .await
            .and_then(|_| Err(error("Site verification required", "challengeRequired")));
    }
    manko_sdk::successful(&response)?;
    Ok(response)
}
pub async fn document(
    context: &Context,
    url: &str,
    base: &str,
    hosts: &[&str],
    agent: &str,
) -> Result<String> {
    let response = request(context, url, base, hosts, agent, false).await?;
    context.now().await?;
    let encoded = text(&response["dataBase64"]);
    if encoded.is_empty() || encoded.len() > 4 * 1024 * 1024 * 4 / 3 + 4 {
        return Err(invalid());
    }
    let mut data = Vec::with_capacity(encoded.len() / 4 * 3);
    for chunk in encoded.as_bytes().chunks(64 * 1024) {
        context.now().await?;
        data.extend(STANDARD.decode(chunk).map_err(|_| invalid())?);
    }
    context.now().await?;
    let body = String::from_utf8(data).map_err(|_| invalid())?;
    if body.contains("cf-chl-") || body.contains("<title>Just a moment...") {
        return context
            .call(
                "challenge",
                json!({"url":url,"reason":"Complete the site's visible verification"}),
            )
            .await
            .and_then(|_| Err(error("Site verification required", "challengeRequired")));
    }
    // No delay: return control to the host before parsing a large response.
    context.now().await?;
    Ok(body)
}
pub async fn image(
    context: &Context,
    url: &str,
    referer: &str,
    hosts: &[&str],
    agent: &str,
) -> Result<Value> {
    content::image(&request(context, url, referer, hosts, agent, true).await?)
}
pub fn unique(items: Vec<Value>, key: &str) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(text(&item[key]).to_owned()))
        .collect()
}
/// Locate the site's simple non-nested blocks without building a DOM for
/// unrelated navigation/popups. Quoted greater-than signs stay in attributes.
pub struct Block<'a> {
    pub header: &'a str,
    pub body: &'a str,
    pub whole: &'a str,
}
pub struct Blocks<'a> {
    remaining: &'a str,
    open: String,
    close: String,
}
pub fn blocks<'a>(html: &'a str, tag: &str) -> Blocks<'a> {
    Blocks {
        remaining: html,
        open: format!("<{tag}"),
        close: format!("</{tag}>"),
    }
}
pub fn tag_header<'a>(html: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}");
    let mut remaining = html;
    loop {
        let start = remaining.find(&open)?;
        remaining = &remaining[start..];
        let boundary = *remaining.as_bytes().get(open.len())?;
        if boundary != b'>' && !boundary.is_ascii_whitespace() {
            remaining = &remaining[open.len()..];
            continue;
        }
        let mut quote = 0;
        for (i, b) in remaining.bytes().enumerate().skip(open.len()) {
            if quote != 0 {
                if b == quote {
                    quote = 0;
                }
            } else if matches!(b, b'\'' | b'"') {
                quote = b;
            } else if b == b'>' {
                return Some(&remaining[..i + 1]);
            }
        }
        return None;
    }
}
pub fn attribute(header: &str, name: &str) -> Option<String> {
    let bytes = header.as_bytes();
    let mut i = 1;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
        i += 1;
    }
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && !matches!(bytes[i], b'=' | b'>')
        {
            i += 1;
        }
        if i == start {
            break;
        }
        let key = &header[start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let quote = bytes.get(i).copied().filter(|b| matches!(b, b'\'' | b'"'));
        if quote.is_some() {
            i += 1;
        }
        let value = i;
        while i < bytes.len()
            && if let Some(q) = quote {
                bytes[i] != q
            } else {
                !bytes[i].is_ascii_whitespace() && bytes[i] != b'>'
            }
        {
            i += 1;
        }
        let raw = header.get(value..i)?;
        if quote.is_some() {
            i += 1;
        }
        if key.eq_ignore_ascii_case(name) {
            return Some(html_escape::decode_html_entities(raw).into_owned());
        }
    }
    None
}
impl<'a> Iterator for Blocks<'a> {
    type Item = Block<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let start = self.remaining.find(&self.open)?;
            self.remaining = &self.remaining[start..];
            let boundary = self.remaining.as_bytes().get(self.open.len()).copied()?;
            if boundary != b'>' && !boundary.is_ascii_whitespace() {
                self.remaining = &self.remaining[self.open.len()..];
                continue;
            }
            let mut quote = 0;
            let mut end = None;
            for (i, b) in self.remaining.bytes().enumerate().skip(self.open.len()) {
                if quote != 0 {
                    if b == quote {
                        quote = 0;
                    }
                } else if b == b'\'' || b == b'"' {
                    quote = b;
                } else if b == b'>' {
                    end = Some(i + 1);
                    break;
                }
            }
            let end = end?;
            let close = end + self.remaining[end..].find(&self.close)?;
            let last = close + self.close.len();
            let result = Block {
                header: &self.remaining[..end],
                body: &self.remaining[end..close],
                whole: &self.remaining[..last],
            };
            self.remaining = &self.remaining[end..];
            return Some(result);
        }
    }
}
pub fn card(id: &str, title: &str, image: &str, kind: &str) -> Value {
    json!({"type":"work","workId":id,"title":title,"imageUrl":image,"mediaKind":kind})
}
pub fn details(
    card: Value,
    description: &str,
    author: &str,
    status: &str,
    rating: &str,
    share: &str,
) -> Value {
    let mut value = card;
    value["workInfo"] = json!({"primaryTitle":value["title"],"secondaryTitles":[],"thumbnailUrl":value["imageUrl"],"synopsis":description,"author":author,"status":status,"contentRating":rating,"shareUrl":share,"mediaKind":value["mediaKind"]});
    value
}
pub fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
// Serialize only reading markup. Never pass remote scripts, event handlers, forms,
// styles or source sets to the publication renderer.
pub fn xhtml(element: ElementRef<'_>, title: &str, base: &str, hosts: &[&str]) -> Result<String> {
    fn children(
        element: ElementRef<'_>,
        output: &mut String,
        base: &str,
        hosts: &[&str],
    ) -> Result<()> {
        for node in element.children() {
            match node.value() {
                Node::Text(value) => output.push_str(&escape(value)),
                Node::Element(value) => {
                    let element = ElementRef::wrap(node).unwrap();
                    let tag = value.name();
                    if matches!(
                        tag,
                        "script"
                            | "style"
                            | "form"
                            | "iframe"
                            | "object"
                            | "embed"
                            | "svg"
                            | "noscript"
                    ) {
                        continue;
                    }
                    let permitted = matches!(
                        tag,
                        "section"
                            | "div"
                            | "p"
                            | "span"
                            | "h1"
                            | "h2"
                            | "h3"
                            | "h4"
                            | "h5"
                            | "h6"
                            | "a"
                            | "em"
                            | "strong"
                            | "i"
                            | "b"
                            | "u"
                            | "blockquote"
                            | "ul"
                            | "ol"
                            | "li"
                            | "img"
                            | "br"
                            | "hr"
                            | "table"
                            | "tbody"
                            | "thead"
                            | "tr"
                            | "td"
                            | "th"
                            | "ruby"
                            | "rt"
                            | "rp"
                            | "dl"
                            | "dt"
                            | "dd"
                            | "figure"
                            | "figcaption"
                            | "aside"
                            | "pre"
                            | "code"
                            | "sup"
                            | "sub"
                    );
                    if permitted {
                        output.push('<');
                        output.push_str(tag);
                        for name in ["id", "lang", "title", "alt"] {
                            if let Some(v) = value.attr(name) {
                                output.push_str(&format!(" {name}=\"{}\"", escape(v)));
                            }
                        }
                        if tag == "img" {
                            output.push_str(&format!(
                                " src=\"{}\"",
                                escape(&link(value.attr("src").ok_or_else(invalid)?, base, hosts)?)
                            ));
                        }
                        if tag == "a" {
                            if let Some(v) = value.attr("href").filter(|v| v.starts_with('#')) {
                                output.push_str(&format!(" href=\"{}\"", escape(v)));
                            }
                        }
                        if matches!(tag, "img" | "br" | "hr") {
                            output.push_str(" />");
                            continue;
                        }
                        output.push('>');
                    }
                    children(element, output, base, hosts)?;
                    if permitted {
                        output.push_str(&format!("</{tag}>"));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut output = format!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{}</title></head><body>",
        escape(title)
    );
    children(element, &mut output, base, hosts)?;
    output.push_str("</body></html>");
    Ok(output)
}

/// Sanitize a large, already bounded reading fragment without allocating a DOM.
/// Only reading tags and explicitly selected attributes reach the renderer.
pub async fn reading_fragment(
    context: &Context,
    html: &str,
    base: &str,
    hosts: &[&str],
) -> Result<String> {
    let mut output = String::new();
    let mut position = 0;
    let mut checkpoint = 0;
    let mut stack: Vec<String> = Vec::new();
    while position < html.len() {
        if position >= checkpoint {
            context.now().await?;
            checkpoint = position + 16 * 1024;
        }
        let rest = &html[position..];
        let next = rest.find('<').unwrap_or(rest.len());
        output.push_str(&escape(&html_escape::decode_html_entities(&rest[..next])));
        position += next;
        if position == html.len() {
            break;
        }
        let rest = &html[position..];
        if rest.starts_with("<!--") {
            position += rest.find("-->").map(|n| n + 3).unwrap_or(rest.len());
            continue;
        }
        let mut quote = 0;
        let mut end = None;
        for (i, b) in rest.bytes().enumerate().skip(1) {
            if quote != 0 {
                if b == quote {
                    quote = 0;
                }
            } else if matches!(b, b'\'' | b'"') {
                quote = b;
            } else if b == b'>' {
                end = Some(i + 1);
                break;
            }
        }
        let Some(end) = end else {
            output.push_str(&escape(rest));
            break;
        };
        let header = &rest[..end];
        let closing = header.starts_with("</");
        let begin = if closing { 2 } else { 1 };
        let length = header[begin..]
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric())
            .count();
        if length == 0 {
            output.push_str("&lt;");
            position += 1;
            continue;
        }
        let tag = header[begin..begin + length].to_ascii_lowercase();
        position += end;
        if !closing
            && matches!(
                tag.as_str(),
                "script"
                    | "style"
                    | "form"
                    | "iframe"
                    | "object"
                    | "embed"
                    | "svg"
                    | "noscript"
                    | "textarea"
            )
        {
            let remaining = &html[position..];
            let lower = remaining.to_ascii_lowercase();
            if let Some(close) = lower.find(&format!("</{tag}")) {
                position += close
                    + lower[close..]
                        .find('>')
                        .map(|i| i + 1)
                        .unwrap_or(lower.len() - close);
            } else {
                position = html.len();
            }
            continue;
        }
        let permitted = matches!(
            tag.as_str(),
            "section"
                | "div"
                | "p"
                | "span"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
                | "a"
                | "em"
                | "strong"
                | "i"
                | "b"
                | "u"
                | "blockquote"
                | "ul"
                | "ol"
                | "li"
                | "img"
                | "br"
                | "hr"
                | "table"
                | "tbody"
                | "thead"
                | "tr"
                | "td"
                | "th"
                | "ruby"
                | "rt"
                | "rp"
                | "dl"
                | "dt"
                | "dd"
                | "figure"
                | "figcaption"
                | "aside"
                | "pre"
                | "code"
                | "sup"
                | "sub"
        );
        if !permitted {
            continue;
        }
        if closing {
            if let Some(index) = stack.iter().rposition(|t| t == &tag) {
                while stack.len() > index {
                    output.push_str(&format!("</{}>", stack.pop().unwrap()));
                }
            }
            continue;
        }
        output.push('<');
        output.push_str(&tag);
        for name in ["id", "lang", "title", "alt"] {
            if let Some(value) = attribute(header, name) {
                output.push_str(&format!(" {name}=\"{}\"", escape(&value)));
            }
        }
        if tag == "img" {
            output.push_str(&format!(
                " src=\"{}\"",
                escape(&link(
                    &attribute(header, "src").ok_or_else(invalid)?,
                    base,
                    hosts
                )?)
            ));
        }
        if tag == "a" {
            if let Some(value) = attribute(header, "href").filter(|v| v.starts_with('#')) {
                output.push_str(&format!(" href=\"{}\"", escape(&value)));
            }
        }
        if matches!(tag.as_str(), "img" | "br" | "hr") {
            output.push_str(" />");
        } else {
            output.push('>');
            stack.push(tag);
        }
        if output.len() > 2 * 1024 * 1024 {
            return Err(invalid());
        }
    }
    while let Some(tag) = stack.pop() {
        output.push_str(&format!("</{tag}>"));
    }
    Ok(output)
}

// Next's public server-rendered payload is data, not executable code. Concatenate
// text chunks, decode JSON records and follow only bounded record references.
pub fn flight_object(html: &str, predicate: impl Fn(&Value) -> bool) -> Result<Value> {
    let mut stream = String::new();
    for part in html.split("self.__next_f.push(").skip(1) {
        let Some(Ok(value)) = serde_json::Deserializer::from_str(part)
            .into_iter::<Value>()
            .next()
        else {
            continue;
        };
        if value[0] == 1 {
            if let Some(s) = value[1].as_str() {
                stream.push_str(s);
            }
        }
    }
    let records: HashMap<&str, Value> = stream
        .lines()
        .filter_map(|line| {
            let (id, data) = line.split_once(':')?;
            serde_json::from_str(data).ok().map(|v| (id, v))
        })
        .collect();
    fn expand(value: &Value, records: &HashMap<&str, Value>, depth: usize) -> Value {
        if depth > 24 {
            return Value::Null;
        }
        if let Some(s) = value.as_str().and_then(|s| s.strip_prefix('$')) {
            if let Some(v) = records.get(s) {
                return expand(v, records, depth + 1);
            }
        }
        match value {
            Value::Array(a) => {
                Value::Array(a.iter().map(|v| expand(v, records, depth + 1)).collect())
            }
            Value::Object(o) => Value::Object(
                o.iter()
                    .map(|(k, v)| (k.clone(), expand(v, records, depth + 1)))
                    .collect(),
            ),
            _ => value.clone(),
        }
    }
    fn find(value: &Value, predicate: &impl Fn(&Value) -> bool, depth: usize) -> Option<Value> {
        if depth > 64 {
            return None;
        }
        if predicate(value) {
            return Some(value.clone());
        }
        match value {
            Value::Array(a) => a.iter().find_map(|v| find(v, predicate, depth + 1)),
            Value::Object(o) => o.values().find_map(|v| find(v, predicate, depth + 1)),
            _ => None,
        }
    }
    for record in records.values() {
        if let Some(value) = find(record, &predicate, 0) {
            return Ok(expand(&value, &records, 0));
        }
    }
    Err(invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn block_scanning_preserves_quoted_attributes_and_nested_candidates() {
        let html = r#"<div><div class="pager-list-left" title="A > B"><a>Next</a></div></div>"#;
        let block = blocks(html, "div")
            .find(|b| b.header.contains("pager-list-left"))
            .unwrap();
        assert!(block.header.contains("A > B"));
        assert_eq!(block.body, "<a>Next</a>");
    }
    #[test]
    fn attributes_handle_entities_quotes_and_tag_boundaries() {
        let html = r#"<aside>Ignore</aside><a disabled HREF='/book/1/name' title="A &amp; B > C &#x26; D" data-x=one><img src=//good.test/a.jpg></a>"#;
        let anchor = tag_header(html, "a").unwrap();
        assert_eq!(attribute(anchor, "href").as_deref(), Some("/book/1/name"));
        assert_eq!(attribute(anchor, "title").as_deref(), Some("A & B > C & D"));
        assert_eq!(attribute(anchor, "data-x").as_deref(), Some("one"));
        assert_eq!(attribute(anchor, "missing"), None);
        assert_eq!(
            attribute(tag_header(html, "img").unwrap(), "src").as_deref(),
            Some("//good.test/a.jpg")
        );
    }
    #[test]
    fn rejects_credentials_ports_and_bad_ids() {
        for value in [
            "https://good.test:444/a",
            "https://name@good.test/a",
            "https://evil.test/a",
            "http://good.test/a",
        ] {
            assert!(link(value, "https://good.test/", &["good.test"]).is_err());
        }
        for value in ["", "..", "a/b", "%2e%2e", "x?y"] {
            assert!(slug(value).is_err());
        }
    }
    #[test]
    fn flight_supports_split_records_and_references() {
        let html = r#"<script>self.__next_f.push([1,"a:{\"series_slug\":\"sample\",\"tags\":\"$b\"}\n"])</script><script>self.__next_f.push([1,"b:[{\"name\":\"Adventure\"}]\n"])</script>"#;
        assert_eq!(
            flight_object(html, |v| v["series_slug"] == "sample").unwrap()["tags"][0]["name"],
            "Adventure"
        );
        assert!(flight_object("<script>danger()</script>", |_| true).is_err());
    }
    #[test]
    fn reading_markup_is_safe_and_preserves_images() {
        let html = Html::parse_fragment(
            r#"<p onclick="evil()">A &amp; B<script>evil()</script><img src="//good.test/page.jpg" onerror="evil()"></p>"#,
        );
        let output = xhtml(
            html.root_element(),
            "Title",
            "https://good.test/",
            &["good.test"],
        )
        .unwrap();
        assert!(output.contains("A &amp; B"));
        assert!(output.contains("https://good.test/page.jpg"));
        assert!(!output.contains("evil"));
    }
}
