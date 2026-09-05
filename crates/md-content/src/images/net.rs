use super::{HTTP_TIMEOUT_SECS, MAX_FETCH};
use gpui::http_client::{self, AsyncBody, HttpClient, Request, Response};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

const MAX_REDIRECTS: usize = 5;

static CLIENT: OnceLock<Arc<dyn HttpClient>> = OnceLock::new();

pub fn http_client() -> Arc<dyn HttpClient> {
    Arc::clone(CLIENT.get_or_init(|| {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("md-image-http")
            .build()
            .expect("image http runtime");
        Arc::new(CappedHttpClient {
            runtime: Arc::new(runtime),
        })
    }))
}

struct CappedHttpClient {
    runtime: Arc<tokio::runtime::Runtime>,
}

impl HttpClient for CappedHttpClient {
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn user_agent(&self) -> Option<&http_client::http::HeaderValue> {
        None
    }

    fn send(
        &self,
        req: Request<AsyncBody>,
    ) -> Pin<Box<dyn Future<Output = http_client::Result<Response<AsyncBody>>> + Send>> {
        let task = self.runtime.spawn(fetch_capped(req));
        Box::pin(async move {
            task.await
                .map_err(|err| http_client::anyhow!("image request task failed: {err}"))?
        })
    }

    fn proxy(&self) -> Option<&http_client::Url> {
        None
    }
}

async fn fetch_capped(req: Request<AsyncBody>) -> http_client::Result<Response<AsyncBody>> {
    if req.method() != http_client::http::Method::GET {
        return Err(http_client::anyhow!("blocked method"));
    }
    let mut url = reqwest::Url::parse(&req.uri().to_string())?;
    for redirects in 0..=MAX_REDIRECTS {
        let response = send_public(&url).await?;
        if response.status().is_redirection() {
            if redirects == MAX_REDIRECTS {
                return Err(http_client::anyhow!("too many redirects"));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .ok_or_else(|| http_client::anyhow!("redirect without location"))?
                .to_str()?;
            url = url.join(location)?;
            continue;
        }
        return response_to_gpui(response).await;
    }
    Err(http_client::anyhow!("too many redirects"))
}

async fn send_public(url: &reqwest::Url) -> http_client::Result<reqwest::Response> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(http_client::anyhow!("blocked scheme"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| http_client::anyhow!("missing host"))?;
    if blocked_hostname(host) {
        return Err(http_client::anyhow!("blocked host"));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| http_client::anyhow!("missing port"))?;
    let addrs = public_addresses(host, port).await?;
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy();
    if host.parse::<IpAddr>().is_err() {
        builder = builder.resolve_to_addrs(host, &addrs);
    }
    builder
        .build()?
        .get(url.clone())
        .send()
        .await
        .map_err(Into::into)
}

async fn public_addresses(host: &str, port: u16) -> http_client::Result<Vec<SocketAddr>> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            return Err(http_client::anyhow!("blocked address"));
        }
        return Ok(vec![SocketAddr::new(ip, port)]);
    }
    let mut addrs: Vec<_> = tokio::net::lookup_host((host, port)).await?.collect();
    addrs.sort_unstable();
    addrs.dedup();
    if addrs.is_empty() || addrs.iter().any(|addr| !is_public_ip(addr.ip())) {
        return Err(http_client::anyhow!("blocked address"));
    }
    Ok(addrs)
}

async fn response_to_gpui(
    mut response: reqwest::Response,
) -> http_client::Result<Response<AsyncBody>> {
    let status = response.status().as_u16();
    if let Some(len) = response.content_length()
        && len > MAX_FETCH as u64
    {
        return Err(http_client::anyhow!("too large"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len().saturating_add(chunk.len()) > MAX_FETCH {
            return Err(http_client::anyhow!("too large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Response::builder()
        .status(status)
        .body(AsyncBody::from(bytes))?)
}

fn blocked_hostname(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local")
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => is_public_v6(ip),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let segments = ip.segments();
    (segments[0] & 0xe000) == 0x2000 && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
}

#[cfg(test)]
mod tests {
    use super::{blocked_hostname, http_client, is_public_ip};
    use std::net::IpAddr;
    use std::sync::Arc;

    #[test]
    fn http_client_is_a_process_wide_singleton() {
        assert!(Arc::ptr_eq(&http_client(), &http_client()));
    }

    #[test]
    fn private_and_local_targets_are_blocked() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "100.64.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "192.168.1.1",
            "198.51.100.1",
            "203.0.113.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            assert!(
                !is_public_ip(address.parse::<IpAddr>().expect("ip")),
                "{address}"
            );
        }
        assert!(blocked_hostname("localhost"));
        assert!(blocked_hostname("printer.local"));

        assert!(blocked_hostname("LOCALHOST."));
    }

    #[test]
    fn public_targets_are_allowed() {
        for address in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(
                is_public_ip(address.parse::<IpAddr>().expect("ip")),
                "{address}"
            );
        }
        assert!(!blocked_hostname("example.com"));
    }
}
