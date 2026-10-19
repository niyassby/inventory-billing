//! HTTPS calls from a counter to its hub. The hub's certificate is self-signed, so it is checked
//! against the fingerprint pinned at pairing instead of a certificate authority; any other
//! certificate (another machine, a man in the middle) fails the connection.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use crate::error::{AppError, AppResult, ErrorCode};
use crate::hub::{tls, Health, DEVICE_HEADER};
use crate::services::auth::ApprovalInput;
use crate::services::devices::{PairInput, Paired};
use crate::services::sync::{CatalogueSnapshot, Lease, PushRequest, PushResponse};

/// Accepts exactly one certificate (by SHA-256), or — for the first contact before pairing —
/// any certificate, remembering which one it saw.
#[derive(Debug)]
struct Pinned {
    expected: Option<String>,
    seen: Mutex<Option<String>>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fingerprint = tls::fingerprint(end_entity.as_ref());
        if let Ok(mut seen) = self.seen.lock() {
            *seen = Some(fingerprint.clone());
        }
        match &self.expected {
            Some(expected) if *expected != fingerprint => Err(rustls::Error::General(
                "the hub's certificate is not the one this counter was paired with".into(),
            )),
            _ => Ok(ServerCertVerified::assertion()),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Why a call to the hub failed.
#[derive(Debug)]
pub enum HubError {
    /// No answer (hub off, network down, timeout): the counter is offline.
    Unreachable(String),
    /// The hub answered with an error: pass it on.
    App(AppError),
}

impl From<HubError> for AppError {
    fn from(e: HubError) -> Self {
        match e {
            HubError::App(e) => e,
            HubError::Unreachable(_) => AppError::hub_offline(),
        }
    }
}

pub type HubResult<T> = Result<T, HubError>;

#[derive(Clone)]
pub struct HubApi {
    base: String,
    device_token: Option<String>,
    http: reqwest::Client,
    verifier: Arc<Pinned>,
}

fn http_client(verifier: Arc<Pinned>, timeout: Duration) -> AppResult<reqwest::Client> {
    let config = rustls::ClientConfig::builder_with_provider(tls::provider())
        .with_safe_default_protocol_versions()
        .map_err(AppError::internal)?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    reqwest::Client::builder()
        .use_preconfigured_tls(config)
        .connect_timeout(Duration::from_secs(5))
        .timeout(timeout)
        .build()
        .map_err(AppError::internal)
}

/// "192.168.1.20" → "https://192.168.1.20:47900"; a full URL is kept (http is upgraded).
pub fn normalise_url(raw: &str, default_port: u16) -> AppResult<String> {
    let raw = raw.trim().trim_end_matches('/');
    if raw.is_empty() {
        return Err(AppError::field("url", "Enter the hub's address."));
    }
    let without_scheme = raw
        .strip_prefix("https://")
        .or_else(|| raw.strip_prefix("http://"))
        .unwrap_or(raw);
    let with_port = if without_scheme.contains(':') && !without_scheme.starts_with('[') {
        without_scheme.to_string()
    } else {
        format!("{without_scheme}:{default_port}")
    };
    let url = format!("https://{with_port}");
    reqwest::Url::parse(&url)
        .map_err(|_| AppError::field("url", "That is not a valid address."))?;
    Ok(url)
}

impl HubApi {
    /// A connection that only trusts the pinned certificate.
    pub fn new(base: &str, fingerprint: &str, device_token: Option<String>) -> AppResult<Self> {
        let verifier = Arc::new(Pinned {
            expected: Some(fingerprint.to_string()),
            seen: Mutex::new(None),
            provider: tls::provider(),
        });
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            device_token,
            http: http_client(verifier.clone(), Duration::from_secs(15))?,
            verifier,
        })
    }

    /// First contact before pairing: trusts any certificate and reports its fingerprint, which
    /// the person compares with the one shown on the hub's screen.
    pub async fn probe(base: &str) -> AppResult<(Health, String)> {
        let verifier = Arc::new(Pinned {
            expected: None,
            seen: Mutex::new(None),
            provider: tls::provider(),
        });
        let api = Self {
            base: base.trim_end_matches('/').to_string(),
            device_token: None,
            http: http_client(verifier.clone(), Duration::from_secs(10))?,
            verifier,
        };
        let health: Health = api.get("/v1/health").await.map_err(|e| match e {
            HubError::Unreachable(m) => AppError::new(
                ErrorCode::HubOffline,
                format!("No hub answered at {base}. ({m})"),
            ),
            HubError::App(e) => e,
        })?;
        let fingerprint = api
            .verifier
            .seen
            .lock()
            .map_err(AppError::internal_lock)?
            .clone()
            .ok_or_else(|| AppError::internal("no certificate seen"))?;
        Ok((health, fingerprint))
    }

    async fn send<T: DeserializeOwned>(&self, request: reqwest::RequestBuilder) -> HubResult<T> {
        let request = match &self.device_token {
            Some(token) => request.header(DEVICE_HEADER, token),
            None => request,
        };
        let response = request
            .send()
            .await
            .map_err(|e| HubError::Unreachable(e.to_string()))?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .map_err(|e| HubError::Unreachable(format!("bad answer from the hub: {e}")))?;
        if status.is_success() {
            serde_json::from_value(body).map_err(|e| HubError::App(AppError::internal(e)))
        } else {
            Err(HubError::App(serde_json::from_value(body).unwrap_or_else(
                |_| AppError::internal(format!("hub answered {status}")),
            )))
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> HubResult<T> {
        self.send(self.http.get(format!("{}{path}", self.base)))
            .await
    }

    async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> HubResult<T> {
        self.send(self.http.post(format!("{}{path}", self.base)).json(body))
            .await
    }

    pub async fn health(&self) -> HubResult<Health> {
        self.get("/v1/health").await
    }

    pub async fn pair(&self, input: &PairInput) -> HubResult<Paired> {
        self.post("/v1/pair", input).await
    }

    pub async fn call(
        &self,
        command: &str,
        token: &str,
        input: &Value,
        approval: Option<&ApprovalInput>,
    ) -> HubResult<Value> {
        self.post(
            &format!("/v1/call/{command}"),
            &json!({ "token": token, "input": input, "approval": approval }),
        )
        .await
    }

    pub async fn catalogue(&self) -> HubResult<CatalogueSnapshot> {
        self.get("/v1/sync/catalogue").await
    }

    pub async fn lease(&self) -> HubResult<Lease> {
        self.post("/v1/sync/lease", &json!({})).await
    }

    pub async fn push(&self, request: &PushRequest) -> HubResult<PushResponse> {
        self.post("/v1/sync/push", request).await
    }
}
