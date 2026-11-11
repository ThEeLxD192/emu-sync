use crate::{OAuth, OAuthError, REDIRECT_URI, loopback_code};
use emusync_domain::{
    config::{AppConfig, CloudAuthorization, CredentialSnapshot, CredentialStore},
    execution::BrowserLauncher,
};
use reqwest::{Client, Url};
use std::{fmt, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;

/// Composition of OAuth, loopback and a platform browser launcher.
#[derive(Clone)]
pub struct InteractiveOAuth<S, B> {
    oauth: OAuth<S>,
    browser: Arc<B>,
    authorization_endpoint: Url,
    address: SocketAddr,
    redirect_uri: String,
}
impl<S: CredentialStore, B> InteractiveOAuth<S, B>
where
    S::Error: fmt::Display,
{
    pub fn new(client: Client, store: S, browser: Arc<B>) -> Self {
        Self::with_endpoints(
            client,
            store,
            browser,
            Url::parse("https://oauth2.googleapis.com/token").expect("static URL"),
            Url::parse("https://accounts.google.com/o/oauth2/v2/auth").expect("static URL"),
            "127.0.0.1:8089".parse().expect("static address"),
            REDIRECT_URI.into(),
        )
    }
    pub fn with_endpoints(
        client: Client,
        store: S,
        browser: Arc<B>,
        token_endpoint: Url,
        authorization_endpoint: Url,
        address: SocketAddr,
        redirect_uri: String,
    ) -> Self {
        Self {
            oauth: OAuth::new(client, store, token_endpoint)
                .with_redirect_uri(redirect_uri.clone()),
            browser,
            authorization_endpoint,
            address,
            redirect_uri,
        }
    }
}
impl<S, B> InteractiveOAuth<S, B>
where
    S: CredentialStore + Sync,
    S::Error: fmt::Display,
    B: BrowserLauncher + Send + Sync + 'static,
    B::Error: fmt::Display,
{
    pub async fn authorize_scoped(
        &self,
        config: &AppConfig,
        allow_interactive: bool,
    ) -> Result<(String, CredentialSnapshot), OAuthError> {
        self.oauth
            .authorize_scoped(config, allow_interactive, |client_id| {
                let client_id = client_id.to_owned();
                let browser = self.browser.clone();
                let endpoint = self.authorization_endpoint.clone();
                let address = self.address;
                let redirect = self.redirect_uri.clone();
                async move {
                    if !address.ip().is_loopback() {
                        return Err(OAuthError::Provider(
                            "OAuth listener must be loopback".into(),
                        ));
                    }
                    let listener = TcpListener::bind(address)
                        .await
                        .map_err(|e| OAuthError::Provider(e.to_string()))?;
                    loopback_code(
                        listener,
                        &client_id,
                        endpoint,
                        &redirect,
                        move |url| async move {
                            tokio::task::spawn_blocking(move || {
                                browser.open(url.as_str()).map_err(|e| e.to_string())
                            })
                            .await
                            .map_err(|_| OAuthError::Provider("browser worker failed".into()))?
                            .map_err(OAuthError::Provider)
                        },
                    )
                    .await
                }
            })
            .await
    }
}

impl<S, B> CloudAuthorization for InteractiveOAuth<S, B>
where
    S: CredentialStore + Sync,
    S::Error: fmt::Display,
    B: BrowserLauncher + Send + Sync + 'static,
    B::Error: fmt::Display,
{
    type Error = OAuthError;
    async fn authorize(
        &self,
        config: &AppConfig,
        allow_interactive: bool,
    ) -> Result<String, Self::Error> {
        self.authorize_scoped(config, allow_interactive)
            .await
            .map(|(token, _)| token)
    }
}
