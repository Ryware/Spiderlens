//! URL normalization shared by start URLs and List mode (research R5).
//! Spec 003 adds query-parameter rules here.

use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct NormError(pub String);

/// Parses user input into a normalized http(s) URL.
///
/// - A missing scheme becomes `https://`.
/// - The WHATWG parser lowercases scheme and host, converts IDN hosts to
///   punycode, removes default ports and resolves `.` and `..` segments.
/// - The fragment is removed.
pub fn normalize_url(input: &str) -> Result<Url, NormError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(NormError("the address is empty".into()));
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };
    let mut url = Url::parse(&candidate).map_err(|e| NormError(format!("not a valid web address ({e})")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(NormError("only http and https addresses are supported".into()));
    }
    match url.host() {
        None => return Err(NormError("the address has no host name".into())),
        Some(url::Host::Domain(d)) if !is_plausible_domain(d) => {
            return Err(NormError(format!("\"{d}\" is not a valid host name")));
        }
        Some(_) => {}
    }
    url.set_fragment(None);
    Ok(url)
}

/// Host names are letters, digits, hyphens and underscores in dot-separated
/// labels (IDN hosts are already punycode here). The WHATWG parser accepts
/// more, such as `ht!tp`, which no site can use.
fn is_plausible_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.trim_end_matches('.').split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn duplicates_collapse() {
        let a = normalize_url("HTTPS://Example.COM:443/a#top").unwrap();
        let b = normalize_url("https://example.com/a").unwrap();
        assert_eq!(a, b);
        assert_eq!(
            normalize_url("example.com").unwrap().as_str(),
            "https://example.com/"
        );
    }

    #[test]
    fn rejects_bad_input() {
        for bad in [
            "",
            "ht!tp//bad",
            "ftp://x.test",
            "http://",
            "https://exa mple.com",
        ] {
            assert!(normalize_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn idn_host_becomes_punycode() {
        assert_eq!(
            normalize_url("https://bücher.de/").unwrap().host_str(),
            Some("xn--bcher-kva.de")
        );
    }

    proptest! {
        #[test]
        fn normalization_is_idempotent(s in "[a-z]{1,10}\\.[a-z]{2,3}(/[a-zA-Z0-9]{0,8}){0,3}") {
            if let Ok(once) = normalize_url(&s) {
                let twice = normalize_url(once.as_str()).unwrap();
                prop_assert_eq!(once, twice);
            }
        }
    }
}
