//! Whether the HTTP clients the coordinator builds use the environment's proxy.

/// Whether the HTTP clients the coordinator builds use the proxy set in the environment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProxyPolicy {
    /// Use `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY`, as users behind a
    /// proxy need.
    #[default]
    FromEnvironment,
    /// Connect directly, for tests whose servers listen on this machine.
    Direct,
}

impl ProxyPolicy {
    pub(crate) fn apply(self, builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
        match self {
            Self::FromEnvironment => builder,
            Self::Direct => builder.no_proxy(),
        }
    }
}
