//! Cookie values never reach the crawl file (constitution: secrets are never
//! stored; spec 006 FR-034; spec 025 FR-026).
//!
//! A `Set-Cookie` response header can carry a session token, so stored
//! headers keep the cookie's name and attributes and replace its value with
//! [`REMOVED`]. Migration step 4 applies the same rule to files written
//! before it, and [`header_pairs`] applies it again when an older file is
//! only viewed (viewing never modifies a file).

use std::borrow::Cow;

use serde_json::Value;

/// What a removed cookie value is replaced with.
pub const REMOVED: &str = "[removed]";

/// Whether a response header carries cookie values.
pub fn is_cookie_header(name: &str) -> bool {
    name.eq_ignore_ascii_case("set-cookie") || name.eq_ignore_ascii_case("set-cookie2")
}

/// `id=abc; Path=/; HttpOnly` becomes `id=[removed]; Path=/; HttpOnly`.
/// A cookie without a name (no `=` before the first `;`) loses its whole
/// first part. Keep in step with the SQL of migration step 4.
pub fn set_cookie(value: &str) -> String {
    let (pair, rest) = match value.find(';') {
        Some(i) => value.split_at(i),
        None => (value, ""),
    };
    match pair.find('=') {
        Some(i) => format!("{}={REMOVED}{rest}", &pair[..i]),
        None => format!("{REMOVED}{rest}"),
    }
}

/// Response headers as they are stored: cookie values removed.
pub fn response_headers(headers: &[(String, String)]) -> Vec<(&str, Cow<'_, str>)> {
    headers
        .iter()
        .map(|(name, value)| {
            let value = if is_cookie_header(name) {
                Cow::Owned(set_cookie(value))
            } else {
                Cow::Borrowed(value.as_str())
            };
            (name.as_str(), value)
        })
        .collect()
}

/// Removes cookie values from stored headers read back as JSON pairs
/// (older files that were viewed but never upgraded). Already removed values
/// are left as they are.
pub fn header_pairs(headers: &mut Value) {
    let Some(pairs) = headers.as_array_mut() else {
        return;
    };
    for pair in pairs {
        let Some([name, value]) = pair.as_array_mut().map(Vec::as_mut_slice) else {
            continue;
        };
        if let (Some(n), Some(v)) = (name.as_str(), value.as_str())
            && is_cookie_header(n)
            && !is_removed(v)
        {
            *value = Value::String(set_cookie(v));
        }
    }
}

/// [`header_pairs`] for headers already parsed into pairs.
pub fn scrub_pairs(headers: &mut [(String, String)]) {
    for (name, value) in headers {
        if is_cookie_header(name) && !is_removed(value) {
            *value = set_cookie(value);
        }
    }
}

fn is_removed(value: &str) -> bool {
    let pair = value.split(';').next().unwrap_or_default();
    pair == REMOVED || pair.split_once('=').is_some_and(|(_, v)| v == REMOVED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_name_and_attributes() {
        assert_eq!(
            set_cookie("sid=abc123; Path=/; Secure; HttpOnly; SameSite=Lax"),
            "sid=[removed]; Path=/; Secure; HttpOnly; SameSite=Lax"
        );
        assert_eq!(set_cookie("sid=abc"), "sid=[removed]");
        assert_eq!(set_cookie("sid="), "sid=[removed]");
        assert_eq!(set_cookie("token=a=b=c; Path=/"), "token=[removed]; Path=/");
    }

    #[test]
    fn nameless_cookie_loses_its_value() {
        assert_eq!(set_cookie("abc123; Path=/"), "[removed]; Path=/");
        assert_eq!(set_cookie("abc123"), "[removed]");
    }

    #[test]
    fn only_cookie_headers_change() {
        let headers = vec![
            ("Content-Type".to_string(), "text/html".to_string()),
            ("set-cookie".to_string(), "a=1".to_string()),
            ("Set-Cookie2".to_string(), "b=2; Version=1".to_string()),
        ];
        let stored: Vec<(&str, String)> = response_headers(&headers)
            .into_iter()
            .map(|(n, v)| (n, v.into_owned()))
            .collect();
        assert_eq!(
            stored,
            [
                ("Content-Type", "text/html".to_string()),
                ("set-cookie", "a=[removed]".to_string()),
                ("Set-Cookie2", "b=[removed]; Version=1".to_string()),
            ]
        );
    }

    #[test]
    fn parsed_pairs_are_scrubbed_once() {
        let mut h = vec![("set-cookie".to_string(), "a=1; Path=/".to_string())];
        scrub_pairs(&mut h);
        scrub_pairs(&mut h);
        assert_eq!(h[0].1, "a=[removed]; Path=/");
    }

    #[test]
    fn json_pairs_are_scrubbed_once() {
        let mut v = serde_json::json!([["Set-Cookie", "sid=abc; Path=/"], ["Server", "x"]]);
        header_pairs(&mut v);
        header_pairs(&mut v);
        assert_eq!(
            v,
            serde_json::json!([["Set-Cookie", "sid=[removed]; Path=/"], ["Server", "x"]])
        );
    }
}
