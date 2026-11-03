//! Network isolation for the unauthenticated, read-only prototype.
use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct Policy {
    pub qemu_forwarding: bool,
    port: u16,
    slots: Arc<Semaphore>,
}
impl Policy {
    pub fn new(qemu_forwarding: bool) -> Self {
        Self {
            qemu_forwarding,
            port: 8080,
            slots: Arc::new(Semaphore::new(64)),
        }
    }
    pub fn runtime(port: u16) -> Self {
        let vendor = std::fs::read_to_string("/sys/class/dmi/id/sys_vendor").unwrap_or_default();
        let mut policy = Self::new(
            vendor.trim() == "QEMU"
                && crate::lab::trusted_runtime_file("network-contract")
                    .is_ok_and(|s| s == crate::lab::NETWORK_CONTRACT),
        );
        policy.port = port;
        policy
    }
    pub fn allows(&self, ip: IpAddr) -> bool {
        ip.is_loopback() || (self.qemu_forwarding && ip == IpAddr::V4(Ipv4Addr::new(10, 0, 2, 2)))
    }
    pub fn bind_ip(&self) -> Ipv4Addr {
        if self.qemu_forwarding {
            Ipv4Addr::UNSPECIFIED
        } else {
            Ipv4Addr::LOCALHOST
        }
    }
    fn authority_allowed(&self, value: &str) -> bool {
        let Ok(authority) = value.parse::<axum::http::uri::Authority>() else {
            return false;
        };
        let host = authority.host();
        let allowed = host.eq_ignore_ascii_case("localhost")
            || host == "127.0.0.1"
            || (self.qemu_forwarding && host == "10.0.2.15");
        allowed && authority.port_u16().unwrap_or(80) == self.port
    }
    fn headers_allowed(&self, request: &Request) -> bool {
        if !request
            .headers()
            .get(axum::http::header::HOST)
            .and_then(|h| h.to_str().ok())
            .is_some_and(|h| self.authority_allowed(h))
        {
            return false;
        }
        match request.headers().get(axum::http::header::ORIGIN) {
            None => true,
            Some(origin)
                if origin == "null"
                    && request.uri().path().starts_with("/app-assets/")
                    && [axum::http::Method::GET, axum::http::Method::HEAD]
                        .contains(request.method()) =>
            {
                true
            }
            Some(origin) => origin
                .to_str()
                .ok()
                .and_then(|s| s.parse::<axum::http::Uri>().ok())
                .is_some_and(|uri| {
                    uri.scheme_str() == Some("http")
                        && uri
                            .authority()
                            .is_some_and(|a| self.authority_allowed(a.as_str()))
                        && uri.path_and_query().is_none_or(|p| p.as_str() == "/")
                }),
        }
    }
}
pub async fn gate(State(policy): State<Policy>, req: Request, next: Next) -> Response {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|p| p.0.ip());
    if !peer.is_some_and(|p| policy.allows(p)) || !policy.headers_allowed(&req) {
        return (
            StatusCode::FORBIDDEN,
            "ZionOS API is restricted to local/QEMU user-network clients",
        )
            .into_response();
    }
    let Ok(_slot) = policy.slots.clone().try_acquire_owned() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "ZionOS request capacity reached",
        )
            .into_response();
    };
    next.run(req).await
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_boundary() {
        for qemu in [false, true] {
            let p = Policy::new(qemu);
            assert!(p.allows("127.0.0.1".parse().unwrap()));
            assert!(p.allows("::1".parse().unwrap()));
            assert_eq!(p.allows("10.0.2.2".parse().unwrap()), qemu);
            for ip in ["192.168.0.1", "8.8.8.8", "10.0.2.3", "::ffff:10.0.2.2"] {
                assert!(!p.allows(ip.parse().unwrap()));
            }
        }
        assert_eq!(Policy::new(false).bind_ip(), Ipv4Addr::LOCALHOST);
    }
}
