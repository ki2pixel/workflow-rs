//! Récupération et protection du `worker-token`.
//!
//! Le token est lu dans le HTML Minijinja de `GET /`. Le GUI ne lit jamais
//! `config/config.toml` et ne met jamais le token brut dans une URL.

use crate::error::ServicesError;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct WorkerToken(String);

impl WorkerToken {
    pub fn new(raw: String) -> Result<Self, ServicesError> {
        let value = raw.trim().to_string();
        if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
            return Err(ServicesError::Token);
        }
        Ok(Self(value))
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn sha256_hex(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.0.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

impl std::fmt::Debug for WorkerToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkerToken([redacted])")
    }
}

#[derive(Clone)]
pub struct Session {
    pub worker_token: WorkerToken,
    pub csrf_token: Option<String>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("worker_token", &self.worker_token)
            .field(
                "csrf_token",
                &self.csrf_token.as_ref().map(|_| "[redacted]"),
            )
            .finish()
    }
}

pub fn parse_session_html(html: &str) -> Result<Session, ServicesError> {
    let document = Html::parse_document(html);
    let token_selector =
        Selector::parse("meta[name='worker-token']").expect("sélecteur worker-token valide");
    let csrf_selector =
        Selector::parse("meta[name='csrf-token']").expect("sélecteur csrf-token valide");

    let token = document
        .select(&token_selector)
        .next()
        .and_then(|element| element.value().attr("content"))
        .ok_or(ServicesError::Token)?;
    let csrf = document
        .select(&csrf_selector)
        .next()
        .and_then(|element| element.value().attr("content"))
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    Ok(Session {
        worker_token: WorkerToken::new(token.to_string())?,
        csrf_token: csrf,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_worker_and_csrf_tokens() {
        let html = r#"
            <html><head>
              <meta name="csrf-token" content="csrf-123">
              <meta name="worker-token" content="worker-456">
            </head></html>
        "#;
        let session = parse_session_html(html).unwrap();
        assert_eq!(session.worker_token.expose(), "worker-456");
        assert_eq!(session.csrf_token.as_deref(), Some("csrf-123"));
    }

    #[test]
    fn parses_reversed_attribute_order_and_html_entities() {
        let html = r#"<meta content="a&amp;b" name="worker-token">"#;
        let session = parse_session_html(html).unwrap();
        assert_eq!(session.worker_token.expose(), "a&b");
    }

    #[test]
    fn rejects_missing_or_empty_token() {
        assert!(matches!(
            parse_session_html("<html><head></head></html>"),
            Err(ServicesError::Token)
        ));
        assert!(matches!(
            parse_session_html(r#"<meta name="worker-token" content="">"#),
            Err(ServicesError::Token)
        ));
        assert!(matches!(
            parse_session_html(r#"<meta name="worker-token" content="   ">"#),
            Err(ServicesError::Token)
        ));
    }

    #[test]
    fn rejects_control_characters() {
        let html = "<meta name=\"worker-token\" content=\"abc\u{0007}def\">";
        assert!(matches!(
            parse_session_html(html),
            Err(ServicesError::Token)
        ));
    }

    #[test]
    fn debug_redacts_token_and_csrf() {
        let token = WorkerToken::new("secret-token".into()).unwrap();
        let session = Session {
            worker_token: token,
            csrf_token: Some("secret-csrf".into()),
        };
        let rendered = format!("{session:?}");
        assert!(!rendered.contains("secret-token"));
        assert!(!rendered.contains("secret-csrf"));
        assert!(rendered.contains("redacted"));
    }

    #[test]
    fn sha256_matches_known_vector() {
        let token = WorkerToken::new("abc".into()).unwrap();
        assert_eq!(
            token.sha256_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
