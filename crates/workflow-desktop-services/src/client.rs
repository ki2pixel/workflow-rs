//! Client HTTP typé du serveur `workflow-server`.

use crate::auth::{parse_session_html, Session, WorkerToken};
use crate::error::ServicesError;
use crate::models::{
    ActionResponse, PingResponse, SequenceStatus, SpecificLog, StepKey, StepStatus, SystemMonitor,
};
use parking_lot::RwLock;
use reqwest::{Method, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use url::Url;

#[derive(Clone)]
pub struct DesktopClient {
    base: Url,
    http: reqwest::Client,
    token: Arc<RwLock<Option<WorkerToken>>>,
    csrf: Arc<RwLock<Option<String>>>,
}

impl DesktopClient {
    pub fn new(base: Url, http: reqwest::Client) -> Result<Self, ServicesError> {
        validate_base_url(&base)?;
        Ok(Self {
            base,
            http,
            token: Arc::new(RwLock::new(None)),
            csrf: Arc::new(RwLock::new(None)),
        })
    }

    pub fn with_default_http(base: Url) -> Result<Self, ServicesError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(30))
            .build()?;
        Self::new(base, http)
    }

    pub fn base_url(&self) -> &Url {
        &self.base
    }

    pub fn session_token(&self) -> Option<WorkerToken> {
        self.token.read().clone()
    }

    pub async fn bootstrap_session(&self) -> Result<Session, ServicesError> {
        let response = self.http.get(self.base.join("/")?).send().await?;
        let status = response.status();
        let html = response.text().await?;
        if !status.is_success() {
            return Err(ServicesError::HttpStatus { status, body: html });
        }
        let session = parse_session_html(&html)?;
        *self.token.write() = Some(session.worker_token.clone());
        *self.csrf.write() = session.csrf_token.clone();
        Ok(session)
    }

    pub async fn ping(&self) -> Result<PingResponse, ServicesError> {
        self.request_json(Method::GET, "/api/ping", None, true)
            .await
    }

    pub async fn run_step(&self, step: StepKey) -> Result<ActionResponse, ServicesError> {
        self.request_json(
            Method::POST,
            &format!("/run/{}", step.as_str()),
            None,
            false,
        )
        .await
    }

    pub async fn stop_step(&self, step: StepKey) -> Result<ActionResponse, ServicesError> {
        self.request_json(
            Method::POST,
            &format!("/stop/{}", step.as_str()),
            None,
            false,
        )
        .await
    }

    pub async fn cancel_step(&self, step: StepKey) -> Result<ActionResponse, ServicesError> {
        self.request_json(
            Method::POST,
            &format!("/cancel/{}", step.as_str()),
            None,
            false,
        )
        .await
    }

    pub async fn step_status(
        &self,
        step: StepKey,
        include_logs: bool,
    ) -> Result<StepStatus, ServicesError> {
        let path = if include_logs {
            format!("/status/{}", step.as_str())
        } else {
            format!("/api/step_status/{}", step.as_str())
        };
        self.request_json(Method::GET, &path, None, true).await
    }

    pub async fn run_sequence(&self, steps: &[StepKey]) -> Result<ActionResponse, ServicesError> {
        let steps: Vec<&str> = steps.iter().map(|step| step.as_str()).collect();
        let body = json!({ "steps": steps });
        self.request_json(Method::POST, "/run_custom_sequence", Some(&body), false)
            .await
    }

    pub async fn sequence_status(&self) -> Result<SequenceStatus, ServicesError> {
        self.request_json(Method::GET, "/sequence/status", None, true)
            .await
    }

    pub async fn stop_sequence(&self) -> Result<ActionResponse, ServicesError> {
        self.request_json(Method::POST, "/sequence/stop", None, false)
            .await
    }

    pub async fn system_monitor(&self) -> Result<SystemMonitor, ServicesError> {
        self.request_json(Method::GET, "/api/system_monitor", None, true)
            .await
    }

    pub async fn specific_log(
        &self,
        step: StepKey,
        index: usize,
    ) -> Result<SpecificLog, ServicesError> {
        self.request_json(
            Method::GET,
            &format!("/get_specific_log/{}/{}", step.as_str(), index),
            None,
            true,
        )
        .await
    }

    pub async fn fetch_sound(&self, name: &str) -> Result<Vec<u8>, ServicesError> {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(ServicesError::Config(format!(
                "nom de son invalide: {name}"
            )));
        }
        let response = self
            .http
            .get(self.base.join(&format!("/sound-design/{name}"))?)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ServicesError::HttpStatus { status, body });
        }
        Ok(response.bytes().await?.to_vec())
    }

    async fn send_once(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Response, ServicesError> {
        let mut request = self
            .http
            .request(method, self.base.join(path)?)
            .header("Accept", "application/json");

        if let Some(token) = self.token.read().clone() {
            request = request.header("X-Worker-Token", token.expose());
        }
        if let Some(csrf) = self.csrf.read().clone() {
            request = request.header("X-CSRF-Token", csrf);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        Ok(request.send().await?)
    }

    async fn request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        retry_get: bool,
    ) -> Result<T, ServicesError> {
        let mut response = self.send_once(method.clone(), path, body).await?;
        if response.status() == StatusCode::UNAUTHORIZED && retry_get {
            self.bootstrap_session().await?;
            response = self.send_once(method, path, body).await?;
        }

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ServicesError::HttpStatus { status, body });
        }
        let bytes = response.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

pub fn validate_base_url(url: &Url) -> Result<(), ServicesError> {
    if url.scheme() != "http" {
        return Err(ServicesError::Config(
            "A-Strict: seul http:// est autorisé pour le serveur local".into(),
        ));
    }
    match url.host_str() {
        Some("127.0.0.1") | Some("::1") | Some("[::1]") => Ok(()),
        Some(host) => Err(ServicesError::Config(format!(
            "A-Strict: hôte non loopback interdit: {host}"
        ))),
        None => Err(ServicesError::Config("URL sans hôte".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_loopback_http_only() {
        assert!(validate_base_url(&Url::parse("http://127.0.0.1:5005/").unwrap()).is_ok());
        assert!(validate_base_url(&Url::parse("http://[::1]:5005/").unwrap()).is_ok());
        assert!(validate_base_url(&Url::parse("https://127.0.0.1:5005/").unwrap()).is_err());
        assert!(validate_base_url(&Url::parse("http://192.168.1.10:5005/").unwrap()).is_err());
        assert!(validate_base_url(&Url::parse("http://example.com:5005/").unwrap()).is_err());
    }
}
