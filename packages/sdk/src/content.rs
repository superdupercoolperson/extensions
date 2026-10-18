// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
use crate::{error, header, text, Result};
use serde_json::{json, Value};

pub fn configuration(
    id: &str,
    title: &str,
    fields: &[(&str, &str)],
    sorts: &[(&str, &str)],
    languages: &[&str],
) -> Value {
    let fields: Vec<Value> = fields.iter().map(|(id, title)| {
        let numeric = matches!(*id, "pages" | "favorites");
        let free = matches!(*id, "uploaded" | "title" | "jtitle");
        let options: Vec<Value> = if *id == "language" { languages.iter().map(|value| json!({"id":value,"title":value})).collect() } else { vec![] };
        json!({"id":id,"title":title,"queryPrefix":format!("{id}:"),"placeholder":format!("Filter by {}", title.to_lowercase()),
            "supportsExclusion":!numeric && !free && !(id == &"language" && languages.len() > 3),
            "maximumSelections":if numeric || free || *id == "language" {1} else {24},
            "inputKind":if numeric {"number"} else if free {"text"} else if *id == "language" {"choice"} else {"lookup"},"options":options})
    }).collect();
    json!({"id":id,"title":title,"fields":fields,"sortOptions":sorts.iter().map(|(id,title)| json!({"id":id,"title":title})).collect::<Vec<_>>(),"defaultSortID":sorts[0].0})
}
pub fn validate_search(config: &Value, input: &Value) -> Result<()> {
    if let Some(sort) = input.get("sort").filter(|value| !value.is_null()) {
        if !config["sortOptions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|option| option["id"] == *sort)
        {
            return Err(error("Unknown search order", "invalidSearchTerm"));
        }
    }
    let Some(selections) = input.get("selections").filter(|value| !value.is_null()) else {
        return Ok(());
    };
    let selections = selections
        .as_array()
        .ok_or_else(|| error("Malformed search selections", "invalidSearchTerm"))?;
    if selections.len() > 24 {
        return Err(error("Too many search selections", "invalidSearchTerm"));
    }
    for selection in selections {
        let field = config["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| field["id"] == selection["fieldID"])
            .ok_or_else(|| error("Unknown search field", "invalidSearchTerm"))?;
        let value = text(&selection["value"]);
        if value.trim().is_empty()
            || value.len() > 200
            || !matches!(text(&selection["polarity"]), "" | "include" | "exclude")
            || (selection["polarity"] == "exclude" && field["supportsExclusion"] == false)
            || selections
                .iter()
                .filter(|s| s["fieldID"] == field["id"])
                .count()
                > field["maximumSelections"].as_u64().unwrap() as usize
        {
            return Err(error("Invalid search selection", "invalidSearchTerm"));
        }
        if field["id"] == "language"
            && !field["options"]
                .as_array()
                .unwrap()
                .iter()
                .any(|option| option["id"] == selection["value"])
        {
            return Err(error("Unsupported language", "invalidSearchTerm"));
        }
        if field["inputKind"] == "number" && value.parse::<u64>().is_err() {
            return Err(error("Choose a whole number", "invalidSearchTerm"));
        }
    }
    Ok(())
}
pub fn image(response: &Value) -> Result<Value> {
    let mime = response["mimeType"]
        .as_str()
        .unwrap_or_else(|| header(response, "content-type"))
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    if !matches!(
        mime.as_str(),
        "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "image/avif"
    ) {
        return Err(error(
            "Response is not a supported image",
            "invalidResponse",
        ));
    }
    if let Some(resource) = response.get("resourceID").filter(|v| !v.is_null()) {
        Ok(json!({"resourceID":resource,"mimeType":mime}))
    } else {
        Ok(json!({"dataBase64":response["dataBase64"],"mimeType":mime}))
    }
}
pub fn suggestions(input: &Value, mut values: Vec<Value>) -> Value {
    let query = text(&input["query"]).trim().to_lowercase();
    let field = text(&input["fieldID"]);
    values.retain(|value| {
        (field.is_empty() || field == text(&value["fieldID"]))
            && (text(&value["value"]).to_lowercase().contains(&query)
                || edit_distance(&text(&value["value"]).to_lowercase(), &query) <= 2)
    });
    values.sort_by_key(|v| {
        (
            !text(&v["value"]).to_lowercase().starts_with(&query),
            text(&v["value"]).to_lowercase(),
        )
    });
    values.dedup_by(|a, b| a["fieldID"] == b["fieldID"] && a["value"] == b["value"]);
    values.truncate(input["limit"].as_u64().unwrap_or(20).clamp(1, 30) as usize);
    Value::Array(values)
}
fn edit_distance(left: &str, right: &str) -> usize {
    if left.len() > 200 || right.len() > 200 {
        return usize::MAX;
    }
    let right: Vec<char> = right.chars().collect();
    let mut costs: Vec<usize> = (0..=right.len()).collect();
    for (i, l) in left.chars().enumerate() {
        let mut previous = costs[0];
        costs[0] = i + 1;
        for (j, r) in right.iter().enumerate() {
            let current = costs[j + 1];
            costs[j + 1] = (previous + usize::from(l != *r))
                .min(costs[j] + 1)
                .min(current + 1);
            previous = current;
        }
    }
    costs[right.len()]
}
pub fn encode_segment(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}
