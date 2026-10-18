// Copyright 2026 manko Extension Contributors; SPDX-License-Identifier: Apache-2.0
// Modified Rust adaptation of keiyoushi MangaFire VrfSigner; see ../NOTICE.
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
const STAGES: &[(&str, &str, u8)] = &[
    ("yINlmUNho8VYJT+ibTIP+9ESiULpVEtMOoD6U6lRE0R/xwXo/Xp9NrUgC4cw/Lmo33vUyjUE40kUoEWIr/fxfNNcq2s79ShQ5NhNrFnJ4hXPwOu/SuXzIbuTQKGFvfm08E9jvCfqAtoDqvQq3dVWPQFmJjgvkISBeXY3BgANR+yVnjGbcxZ47d6kLNfZPIayTq3/YGySb1KuVZodWp/WGNAO5pfMcpaK53Hhs0allBszaMaxuouOwdxbwgxIw6YunSsXjI05Yi0j9j4eHKfSXR8Ifo/Od+8iamRfCXTyvm7NGRGYdcQ0ywcK/u6RXhrbcCm4t2eCtrDgQVecJGkQ+A==", "0Ec58JOY3uBzJK9m3zqIOpdlF7UFiax9DmA=", 90),
    ("IUFltCxD3Oc2cwCgkJffthaOg9cgPUb0LgW6H/VtfcF0kc5F25t+aWj6JH9VOhOaY0rAFdUxlDnl5BLNvwEJvQtP5qcw7vdb/K+chnbwnspSHT8mz5lqwz41TezG0hkO06FTjJZhsyNuFLDpD2ZZxQj/QIRcF90zpmQ7Byu483WsQqUE0C342HL+JXngRB6fRzxRyVTaKu83h7UYTJ0QMt6ixFh6S3F8gqkKwrGTL3jHNBsD45UnifK8+RGtishQV2K3rujLKEkiZxpr2dYcudFW4oFsDKhad3CLBvuyTqsCo4B7mL5IKQ1vXo/MOOvq1I1d8ar9X6Ttu5KF4fZgiA==", "AAdjb1iPY8CiDmq9H34tKTBF8a3oDQ==", 53),
    ("NQHlu1/wVO5EmkwQymF810qqY2xG1k2obcas4Z9mCsPEIFl9pRIjFxbJ7ybMHbBckT5Ton85E0FOeHezbh/mjlEYpmpnlXOS8dgrqeq2KfxImTh1YK9y0PeMNhzA1OQzSY9brYOJq/l2QnE/hwOeZIhPixVSKIUlDb5vLcH6RWKxkIEMuP0bDwIqQ71AJJaEaMJL7A6YtyIwoRT+L5v4aZzodN/0+3nOGsfblFjgxSfPzVDjNFeNl5P26+kEC/8AHgdrpAbt3hHz3HrRN1Y6e+JHgF7ncFWnoF0y3THL1S71WgWGCa6KtSzTCCG58n68nTyj2T3Sshk7utqCtMi/ZQ==", "DELOJgPsVaCcblDtTGMdHzM=", 186),
];
pub fn sign(path: &str) -> String {
    let mut bytes = path.as_bytes().to_vec();
    for (table, key, iv) in STAGES {
        let table = STANDARD.decode(table).expect("static substitution table");
        let key = STANDARD.decode(key).expect("static signing key");
        let mut previous = *iv;
        for (index, byte) in bytes.iter_mut().enumerate() {
            previous = table[(*byte ^ key[index % key.len()] ^ previous) as usize];
            *byte = previous;
        }
    }
    URL_SAFE_NO_PAD.encode(bytes)
}
pub fn request_url(path: &str, mut pairs: Vec<(String, String)>) -> String {
    // Stable key ordering, decoded values, explicit indices for array parameters.
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let mut canonical = path.to_owned();
    let mut previous = "";
    let mut index = 0;
    for (position, (key, value)) in pairs.iter().enumerate() {
        canonical.push(if position == 0 { '?' } else { '&' });
        if let Some(prefix) = key.strip_suffix("[]") {
            if previous != key {
                index = 0;
            }
            canonical.push_str(&format!("{prefix}[{index}]"));
            index += 1;
        } else {
            canonical.push_str(key);
        }
        canonical.push('=');
        canonical.push_str(value);
        previous = key;
    }
    let mut url = url::Url::parse(&format!("https://mangafire.to/api{path}")).unwrap();
    url.query_pairs_mut()
        .extend_pairs(pairs)
        .append_pair("vrf", &sign(&canonical));
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_upstream_signing_vectors() {
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/signing.json")).unwrap();
        for vector in vectors.as_array().unwrap() {
            assert_eq!(
                sign(vector["path"].as_str().unwrap()),
                vector["token"].as_str().unwrap()
            );
        }
    }
    #[test]
    fn canonicalizes_arrays_and_unicode_without_losing_query_values() {
        let url = request_url(
            "/titles",
            vec![
                ("page".into(), "1".into()),
                ("genres_in[]".into(), "1".into()),
                ("keyword".into(), "海 & + story".into()),
                ("genres_in[]".into(), "78".into()),
                ("languages[]".into(), "en".into()),
                ("limit".into(), "30".into()),
                ("order[relevance]".into(), "desc".into()),
            ],
        );
        let parsed = url::Url::parse(&url).unwrap();
        let pairs = parsed.query_pairs().collect::<Vec<_>>();
        assert_eq!(
            pairs.iter().find(|(k, _)| k == "keyword").unwrap().1,
            "海 & + story"
        );
        let vectors: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/signing.json")).unwrap();
        assert_eq!(
            pairs.iter().find(|(k, _)| k == "vrf").unwrap().1,
            vectors[3]["token"].as_str().unwrap()
        );
    }
}
