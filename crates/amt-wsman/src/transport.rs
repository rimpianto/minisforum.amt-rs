use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{header, Method, Request, StatusCode, Uri};
use hyper_openssl::client::legacy::HttpsConnector;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use openssl::error::ErrorStack;
use openssl::ssl::{
    SslConnector, SslConnectorBuilder, SslMethod, SslOptions, SslVerifyMode, SslVersion,
};

pub struct WsManResponse {
    pub status: StatusCode,
    pub www_authenticate: Option<String>,
    pub body: String,
}

pub struct WsManClient {
    client: Client<HttpsConnector<HttpConnector>, Full<Bytes>>,
    timeout: Duration,
}

fn openssl_connector() -> Result<SslConnectorBuilder, ErrorStack> {
    let mut ssl = SslConnector::builder(SslMethod::tls_client())?;
    ssl.set_min_proto_version(Some(SslVersion::TLS1_2))?;
    ssl.set_max_proto_version(Some(SslVersion::TLS1_2))?;
    ssl.set_verify(SslVerifyMode::NONE);
    ssl.set_alpn_protos(b"\x08http/1.1")?;

    // ME 16 does not support RFC 5746 secure renegotiation. Allow only the
    // initial connection to those unpatched servers, not unsafe renegotiation
    // with arbitrary peers. Modern firmware continues to use secure TLS 1.2.
    ssl.set_options(SslOptions::from_bits_retain(
        openssl_sys::SSL_OP_LEGACY_SERVER_CONNECT,
    ));
    Ok(ssl)
}

impl WsManClient {
    pub fn new(timeout: Duration) -> Result<Self> {
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        let mut https = HttpsConnector::with_connector(http, openssl_connector()?)
            .context("failed to build WS-Man TLS connector")?;
        https.set_callback(|config, _uri| {
            config.set_use_server_name_indication(false);
            config.set_verify_hostname(false);
            Ok(())
        });
        let client = Client::builder(TokioExecutor::new()).build(https);
        Ok(Self { client, timeout })
    }

    pub async fn post(
        &self,
        url: Uri,
        envelope: String,
        authorization: Option<String>,
    ) -> Result<WsManResponse> {
        let mut request = Request::builder()
            .method(Method::POST)
            .uri(url)
            .header(header::CONTENT_TYPE, "application/soap+xml; charset=UTF-8");
        if let Some(authorization) = authorization {
            request = request.header(header::AUTHORIZATION, authorization);
        }
        let request = request
            .body(Full::new(Bytes::from(envelope)))
            .context("failed to build WS-Man HTTP request")?;

        let (status, www_authenticate, body) = tokio::time::timeout(self.timeout, async {
            let response = self
                .client
                .request(request)
                .await
                .context("WS-Man HTTP request failed")?;
            let status = response.status();
            let www_authenticate = response
                .headers()
                .get(header::WWW_AUTHENTICATE)
                .map(|value| value.to_str().map(str::to_owned))
                .transpose()
                .context("invalid WWW-Authenticate header")?;
            let bytes = response
                .into_body()
                .collect()
                .await
                .context("failed to read WS-Man HTTP response")?
                .to_bytes();
            let body = String::from_utf8_lossy(&bytes).into_owned();
            Ok::<_, anyhow::Error>((status, www_authenticate, body))
        })
        .await
        .context("WS-Man request timed out")??;

        Ok(WsManResponse {
            status,
            www_authenticate,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use openssl::ssl::SslOptions;

    #[test]
    fn legacy_compatibility_is_limited_to_server_connections() {
        let builder = super::openssl_connector().expect("OpenSSL connector");
        let options = builder.options();
        let legacy_server = SslOptions::from_bits_retain(openssl_sys::SSL_OP_LEGACY_SERVER_CONNECT);

        assert!(options.contains(legacy_server));
        assert!(!options.contains(SslOptions::ALLOW_UNSAFE_LEGACY_RENEGOTIATION));
    }
}
