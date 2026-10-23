//! Normalisation et classification des URL du webhook.
//!
//! Reproduit `CSVService._normalize_url`, `_is_dropbox_url`,
//! `_is_dropbox_proxy_url`, `_looks_like_archive_download` et
//! `rewrite_dropbox_to_dl_host` du service Python.

use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

const PATH_ENCODE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'/');

const QUERY_ENCODE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

#[derive(Debug, Clone, Default)]
struct SplitUrl {
    scheme: String,
    userinfo: String,
    host: String,
    port: Option<u16>,
    path: String,
    query: String,
    has_authority: bool,
}

/// Normalise une URL comme le service Python, y compris les variantes
/// doublement encodées (`amp%3Bdl=0`) et le forçage Dropbox `dl=1`.
pub fn normalize_url(url: &str) -> String {
    if url.is_empty() {
        return String::new();
    }
    let trimmed = url.trim();
    let unescaped = html_unescape(trimmed);
    let decoded = decode_double_encoded(&unescaped);
    let Some(parts) = split_url(&decoded) else {
        return trimmed.to_string();
    };

    let mut netloc = String::new();
    if !parts.userinfo.is_empty() {
        netloc.push_str(&parts.userinfo);
        netloc.push('@');
    }
    netloc.push_str(&parts.host);
    if let Some(port) = parts.port {
        let default_port =
            (parts.scheme == "http" && port == 80) || (parts.scheme == "https" && port == 443);
        if !default_port {
            netloc.push(':');
            netloc.push_str(&port.to_string());
        }
    }

    let mut path = percent_decode_str(&parts.path)
        .decode_utf8_lossy()
        .to_string();
    while path.contains("//") {
        path = path.replace("//", "/");
    }
    if path.ends_with('/') && path != "/" {
        path.pop();
    }
    let path = utf8_percent_encode(&path, PATH_ENCODE).to_string();

    let is_dropbox =
        parts.host.ends_with("dropbox.com") || parts.host == "dl.dropboxusercontent.com";
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut seen: Vec<(String, String)> = Vec::new();
    for field in parts.query.split('&') {
        if field.is_empty() {
            continue;
        }
        let (key, value) = match field.split_once('=') {
            Some((key, value)) => (decode_plus(key), decode_plus(value)),
            None => (decode_plus(field), String::new()),
        };
        let key = key.trim().to_string();
        let value = value.trim().to_string();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        let pair = (key.clone(), value.clone());
        if seen.contains(&pair) {
            continue;
        }
        seen.push(pair);
        if is_dropbox && key.eq_ignore_ascii_case("dl") {
            continue;
        }
        pairs.push((key, value));
    }
    if is_dropbox {
        pairs.push(("dl".into(), "1".into()));
    }
    pairs.sort_by(|left, right| {
        left.0
            .to_lowercase()
            .cmp(&right.0.to_lowercase())
            .then_with(|| left.1.cmp(&right.1))
    });
    let query = pairs
        .iter()
        .map(|(key, value)| format!("{}={}", quote_plus(key), quote_plus(value)))
        .collect::<Vec<_>>()
        .join("&");

    let mut output = String::new();
    if !parts.scheme.is_empty() {
        output.push_str(&parts.scheme);
        output.push(':');
    }
    if parts.has_authority {
        output.push_str("//");
        output.push_str(&netloc);
    }
    output.push_str(&path);
    if !query.is_empty() {
        output.push('?');
        output.push_str(&query);
    }
    output
}

fn decode_double_encoded(raw: &str) -> String {
    let mut current = raw.to_string();
    for _ in 0..3 {
        let previous = current.clone();
        current = current.replace("amp%3B", "&").replace("amp%3b", "&");
        if current.contains("%3B") || current.contains("%3b") {
            let decoded = percent_decode_str(&current).decode_utf8_lossy().to_string();
            if decoded.contains("://") {
                current = decoded;
            }
        }
        if current == previous {
            break;
        }
    }
    current
}

fn split_url(raw: &str) -> Option<SplitUrl> {
    let without_fragment = match raw.find('#') {
        Some(index) => &raw[..index],
        None => raw,
    };

    let (scheme, rest) = match without_fragment.find(':') {
        Some(index)
            if without_fragment[..index]
                .chars()
                .enumerate()
                .all(|(position, character)| {
                    character.is_ascii_alphabetic()
                        || (position > 0
                            && (character.is_ascii_digit() || matches!(character, '+' | '-' | '.')))
                })
                && index > 0 =>
        {
            (
                without_fragment[..index].to_ascii_lowercase(),
                &without_fragment[index + 1..],
            )
        }
        _ => (String::new(), without_fragment),
    };

    let (authority, remainder) = if let Some(after) = rest.strip_prefix("//") {
        let end = after.find(['/', '?']).unwrap_or(after.len());
        (&after[..end], &after[end..])
    } else {
        ("", rest)
    };

    let (path, query) = match remainder.split_once('?') {
        Some((path, query)) => (path, query),
        None => (remainder, ""),
    };

    let (userinfo, hostport) = match authority.rfind('@') {
        Some(index) => (authority[..index].to_string(), &authority[index + 1..]),
        None => (String::new(), authority),
    };

    let (host, port) = split_host_port(hostport)?;
    Some(SplitUrl {
        scheme,
        userinfo,
        host,
        port,
        path: path.to_string(),
        query: query.to_string(),
        has_authority: !authority.is_empty() || rest.starts_with("//"),
    })
}

fn split_host_port(hostport: &str) -> Option<(String, Option<u16>)> {
    if hostport.is_empty() {
        return Some((String::new(), None));
    }
    if let Some(stripped) = hostport.strip_prefix('[') {
        let end = stripped.find(']')?;
        let host = stripped[..end].to_ascii_lowercase();
        let after = &stripped[end + 1..];
        let port = if after.is_empty() {
            None
        } else {
            let port = after.strip_prefix(':')?;
            if port.is_empty() {
                None
            } else {
                Some(port.parse::<u16>().ok()?)
            }
        };
        return Some((host, port));
    }
    match hostport.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') => {
            if port.is_empty() {
                Some((host.to_ascii_lowercase(), None))
            } else if port.chars().all(|character| character.is_ascii_digit()) {
                Some((host.to_ascii_lowercase(), Some(port.parse::<u16>().ok()?)))
            } else {
                None
            }
        }
        _ => Some((hostport.to_ascii_lowercase(), None)),
    }
}

fn decode_plus(value: &str) -> String {
    let replaced = value.replace('+', " ");
    percent_decode_str(&replaced)
        .decode_utf8_lossy()
        .to_string()
}

fn quote_plus(value: &str) -> String {
    utf8_percent_encode(value, QUERY_ENCODE)
        .to_string()
        .replace("%20", "+")
}

fn html_unescape(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        let rest = &input[index..];
        if let Some(entity) = rest.strip_prefix('&') {
            if let Some(end) = entity.find(';').filter(|end| *end <= 12) {
                let body = &entity[..end];
                let replacement = match body {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some('\u{a0}'),
                    _ if body.starts_with("#x") || body.starts_with("#X") => {
                        u32::from_str_radix(&body[2..], 16)
                            .ok()
                            .and_then(char::from_u32)
                    }
                    _ if body.starts_with('#') => {
                        body[1..].parse::<u32>().ok().and_then(char::from_u32)
                    }
                    _ => None,
                };
                if let Some(replacement) = replacement {
                    output.push(replacement);
                    index += end + 2;
                    continue;
                }
            }
        }
        let character = rest.chars().next().expect("caractère");
        output.push(character);
        index += character.len_utf8();
    }
    output
}

/// Classification utilisée par le webhook (`_classify_url_type`).
pub fn classify_url_type(url: &str) -> &'static str {
    let lower = url.to_ascii_lowercase();
    if lower.contains("fromsmash.com") {
        "fromsmash"
    } else if lower.contains("swisstransfer.com") {
        "swisstransfer"
    } else if lower.contains("dropbox.com")
        || lower.contains("dl.dropboxusercontent.com")
        || (lower.contains("/dropbox/")
            && (lower.contains("workers.dev") || lower.contains("worker")))
    {
        "dropbox"
    } else {
        "external"
    }
}

/// Schéma d'URL en minuscules, ou chaîne vide si absent.
pub fn url_scheme(url: &str) -> String {
    match url.find(':') {
        Some(index)
            if index > 0
                && url[..index]
                    .chars()
                    .enumerate()
                    .all(|(position, character)| {
                        character.is_ascii_alphabetic()
                            || (position > 0
                                && (character.is_ascii_digit()
                                    || matches!(character, '+' | '-' | '.')))
                    }) =>
        {
            url[..index].to_ascii_lowercase()
        }
        _ => String::new(),
    }
}

pub fn is_dropbox_url(url: &str) -> bool {
    split_url(url.trim())
        .map(|parts| {
            matches!(
                parts.host.as_str(),
                "dropbox.com" | "www.dropbox.com" | "dl.dropboxusercontent.com"
            )
        })
        .unwrap_or(false)
}

pub fn is_dropbox_proxy_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.contains("/dropbox/") && (lower.contains("workers.dev") || lower.contains("worker"))
}

pub fn looks_like_archive_download(url: &str, original_filename: &str) -> bool {
    let url = url.trim().to_ascii_lowercase();
    let filename = original_filename.trim().to_ascii_lowercase();
    filename.ends_with(".zip")
        || url.contains("/scl/fo/")
        || url.ends_with(".zip")
        || url.contains(".zip?")
}

/// Réécrit un lien Dropbox de fichier vers `dl.dropboxusercontent.com`.
/// Les liens de dossier `/scl/fo/` ne sont jamais réécrits.
pub fn rewrite_dropbox_to_dl_host(url: &str) -> String {
    let Some(parts) = split_url(url) else {
        return url.to_string();
    };
    if parts.host.ends_with("dropbox.com")
        && parts.host != "dl.dropboxusercontent.com"
        && !parts.path.to_ascii_lowercase().starts_with("/scl/fo/")
    {
        let mut netloc = "dl.dropboxusercontent.com".to_string();
        if let Some(port) = parts.port {
            if port != 80 && port != 443 {
                netloc.push(':');
                netloc.push_str(&port.to_string());
            }
        }
        let scheme = if parts.scheme.is_empty() {
            "https"
        } else {
            &parts.scheme
        };
        let mut output = format!("{scheme}://{netloc}{}", parts.path);
        if !parts.query.is_empty() {
            output.push('?');
            output.push_str(&parts.query);
        }
        return output;
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_dropbox_variants_to_one_url() {
        let base = "https://www.dropbox.com/scl/fo/abc123/SomeFolder?rlkey=XYZ";
        let variants = [
            format!("{base}&dl=1"),
            format!("{base}&dl=1&dl=1"),
            base.replace("rlkey=XYZ&dl=1", "dl=1&rlkey=XYZ"),
            format!("{base}&dl=1#fragment"),
            base.replace("www.dropbox.com", "WWW.DROPBOX.COM") + "&dl=1",
        ];
        let normalized: Vec<String> = variants.iter().map(|url| normalize_url(url)).collect();
        assert!(normalized.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(normalized[0].matches("dl=1").count(), 1);
    }

    #[test]
    fn normalizes_double_encoded_ampersand() {
        let base = "https://www.dropbox.com/scl/fo/oy6vg46u1vrdhsk080zs9/ALlPIDw5bpPaOMSZRVMZVUs";
        let malformed = format!("{base}?amp%3Bdl=0&dl=1&rlkey=itwttqb1mwptzr8zyenucbj93");
        let clean = format!("{base}?dl=1&rlkey=itwttqb1mwptzr8zyenucbj93");
        assert_eq!(normalize_url(&malformed), normalize_url(&clean));
        assert!(!normalize_url(&malformed).contains("amp%3B"));
    }

    #[test]
    fn normalizes_basic_url_and_sorts_parameters() {
        assert_eq!(
            normalize_url("https://example.com/file.zip"),
            "https://example.com/file.zip"
        );
        assert_eq!(
            normalize_url("https://example.com/file.zip?z=1&a=2&m=3"),
            "https://example.com/file.zip?a=2&m=3&z=1"
        );
        assert_eq!(
            normalize_url("https://example.com/file.zip?foo=&bar=value"),
            "https://example.com/file.zip?bar=value"
        );
        assert_eq!(
            normalize_url("https://example.com/path/"),
            "https://example.com/path"
        );
    }

    #[test]
    fn unescapes_html_entities() {
        assert_eq!(
            normalize_url("https://example.com/file.zip?foo=bar&amp;baz=qux"),
            "https://example.com/file.zip?baz=qux&foo=bar"
        );
    }

    #[test]
    fn keeps_non_dropbox_query_untouched_apart_from_sorting() {
        assert_eq!(
            normalize_url("https://example.com/a%20b?q=a+b"),
            "https://example.com/a%20b?q=a+b"
        );
    }

    #[test]
    fn classifies_providers_like_python() {
        assert_eq!(classify_url_type("https://fromsmash.com/x"), "fromsmash");
        assert_eq!(
            classify_url_type("https://www.swisstransfer.com/d/x"),
            "swisstransfer"
        );
        assert_eq!(
            classify_url_type("https://dl.dropboxusercontent.com/x"),
            "dropbox"
        );
        assert_eq!(
            classify_url_type("https://server.kidpixel.workers.dev/dropbox/a/b/file"),
            "dropbox"
        );
        assert_eq!(
            classify_url_type("https://example.com/file.zip"),
            "external"
        );
        assert_eq!(classify_url_type(""), "external");
    }

    #[test]
    fn detects_archive_links() {
        assert!(looks_like_archive_download(
            "https://www.dropbox.com/scl/fo/abc",
            ""
        ));
        assert!(looks_like_archive_download(
            "https://example.com/file.zip?dl=1",
            ""
        ));
        assert!(looks_like_archive_download("", "153 Camille.zip"));
        assert!(!looks_like_archive_download(
            "https://www.dropbox.com/s/abc/preview.png",
            "preview.png"
        ));
    }

    #[test]
    fn rewrites_file_links_but_not_folder_links() {
        let rewritten = rewrite_dropbox_to_dl_host("https://www.dropbox.com/s/abc/file.zip?dl=0");
        assert!(rewritten.contains("dl.dropboxusercontent.com"));
        let folder = rewrite_dropbox_to_dl_host("https://www.dropbox.com/scl/fo/abc?rlkey=x");
        assert_eq!(folder, "https://www.dropbox.com/scl/fo/abc?rlkey=x");
    }

    #[test]
    fn detects_dropbox_hosts_and_proxy() {
        assert!(is_dropbox_url("https://www.dropbox.com/s/abc"));
        assert!(is_dropbox_url("https://dl.dropboxusercontent.com/s/abc"));
        assert!(!is_dropbox_url("https://example.com/dropbox/s/abc"));
        assert!(is_dropbox_proxy_url(
            "https://server.kidpixel.workers.dev/dropbox/a/b/file"
        ));
    }
}
