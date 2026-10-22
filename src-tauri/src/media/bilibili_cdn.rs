//! B 站 CDN 候选、重定向安全校验与刷新 / 冷却预算。
//!
//! 协议约束：
//! - 只接受 `https` + 白名单后缀 + 明确端口；拒绝账号信息、IP 字面量、本机 / 内网主机名；
//! - 每次重定向都重新做同一套校验，拒绝降级到 `http`、协议相对地址和跨白名单主机；
//! - 解析出的目的地址必须是公网地址（预解析后 pin 到连接，避免 TOCTOU）；
//! - `403/404/410` 才刷新 playurl 且有次数上限；`412/429` 尊重 `Retry-After` 冷却。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use url::Url;

/// B 站视频 CDN（含 PCDN / 海外）域名后缀
pub const CDN_HOST_SUFFIXES: &[&str] = &[
    ".bilivideo.com",
    ".bilivideo.cn",
    ".akamaized.net",
    ".szbdyd.com",
    ".hdslb.com",
];

/// 允许的端口：默认（443）/ B 站 PCDN 实测端口
pub const ALLOWED_PORTS: &[u16] = &[443, 4483];

/// 单次播放地址解析后最多保留的候选数（主地址 + 备用）
pub const MAX_CANDIDATES: usize = 3;

/// 从 playurl 的 `url` / `backup_url` 里按顺序挑出安全候选（去重、限个数）
pub fn select_candidates<I, S>(raw: I) -> Vec<Url>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out: Vec<Url> = Vec::new();
    for raw in raw {
        let Some(url) = validate_candidate(raw.as_ref()) else {
            continue;
        };
        if out.iter().any(|existing| existing.as_str() == url.as_str()) {
            continue;
        }
        out.push(url);
        if out.len() >= MAX_CANDIDATES {
            break;
        }
    }
    out
}

/// 播放地址校验：`http` 升级为 `https` 后再走同一套规则。
pub fn validate_candidate(raw: &str) -> Option<Url> {
    let raw = raw.trim();
    if raw.is_empty() || has_unsafe_authority_chars(raw) {
        return None;
    }
    let upgraded = if raw.len() >= 7 && raw[..7].eq_ignore_ascii_case("http://") {
        format!("https://{}", &raw[7..])
    } else {
        raw.to_string()
    };
    let url = Url::parse(&upgraded).ok()?;
    is_allowed_cdn_url(&url).then_some(url)
}

/// 每次重定向都重新校验；相对地址按 base 解析，绝不接受跨主机或明文。
pub fn validate_redirect(base: &Url, location: &str) -> Option<Url> {
    let location = location.trim();
    if location.is_empty() || location.starts_with("//") || location.starts_with('\\') {
        return None;
    }
    if has_unsafe_authority_chars(location) {
        return None;
    }
    let target = base.join(location).ok()?;
    is_allowed_cdn_url(&target).then_some(target)
}

/// scheme / 账号信息 / 端口 / 主机四道门
pub fn is_allowed_cdn_url(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let Some(port) = url.port() else {
        // 默认端口 443
        return check_host(url).is_some();
    };
    if !ALLOWED_PORTS.contains(&port) {
        return false;
    }
    check_host(url).is_some()
}

fn check_host(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    if host.is_empty() || host.parse::<IpAddr>().is_ok() || is_private_host(host) {
        return None;
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    CDN_HOST_SUFFIXES
        .iter()
        .any(|suffix| host.ends_with(suffix))
        .then_some(host)
}

/// 主机名层面的本机 / 内网 / 保留名（目的地址层由 [`ensure_public_addrs`] 兜底）
pub fn is_private_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".home.arpa")
        || host == "0.0.0.0"
        || host == "::1"
        || host == "[::1]"
}

/// 权威段里出现转义 / 反斜杠 / 多余 `@` / 非 ASCII 一律拒绝
/// （避免 `%61.bilivideo.com`、`a.bilivideo.com\@evil.com` 这类绕过）
fn has_unsafe_authority_chars(raw: &str) -> bool {
    let authority = match raw.split_once("://") {
        Some((_, rest)) => rest,
        None => raw,
    };
    let authority = authority.split(['/', '?', '#']).next().unwrap_or(authority);
    authority.chars().any(|c| {
        c == '%' || c == '\\' || !c.is_ascii() || (c == '@' && authority.matches('@').count() > 0)
    })
}

/// 解析结果必须全部是公网地址；任何一个命中内网区间都拒绝（不挑一个能用的）。
pub fn ensure_public_addrs(addrs: &[SocketAddr]) -> Result<(), DnsReject> {
    if addrs.is_empty() {
        return Err(DnsReject::NoAddress);
    }
    if addrs.iter().any(|addr| !is_public_ip(addr.ip())) {
        return Err(DnsReject::PrivateAddress);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsReject {
    NoAddress,
    PrivateAddress,
}

pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_v4(v4);
            }
            is_public_v6(v6)
        }
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || ip.is_documentation()
        || octets[0] == 0
        // 100.64.0.0/10 CGNAT
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        // 198.18.0.0/15 benchmarking
        || (octets[0] == 198 && (18..=19).contains(&octets[1]))
        // 192.0.0.0/24 保留
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0))
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_unique_local()
        || ip.is_unicast_link_local())
}

/// 上游状态码分类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusClass {
    /// 200 / 206 / 416：交给 Range 逻辑处理
    Range,
    /// 地址过期 / 被拒，可以刷新一次 playurl
    Expired,
    /// 限流：尊重 `Retry-After` 冷却
    RateLimited,
    /// 上游鉴权 / 风控拒绝
    Auth,
    /// 5xx：可换候选重试
    Retryable,
    /// 其它 4xx：直接失败
    Fatal,
}

pub fn classify_status(status: u16) -> StatusClass {
    match status {
        200 | 206 | 416 => StatusClass::Range,
        403 | 404 | 410 => StatusClass::Expired,
        412 | 429 => StatusClass::RateLimited,
        401 | 407 => StatusClass::Auth,
        s if (500..600).contains(&s) => StatusClass::Retryable,
        _ => StatusClass::Fatal,
    }
}

/// `Retry-After` 只接受秒数形式（B 站 / CDN 用秒），并封顶防止长时间挂死。
pub fn parse_retry_after(raw: Option<&str>) -> Option<Duration> {
    let secs: u64 = raw?.trim().parse().ok()?;
    Some(Duration::from_secs(secs.min(600)))
}

/// 刷新 playurl 的预算与冷却
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshDecision {
    Allowed,
    /// 本窗口内刷新次数用尽
    BudgetExhausted,
    /// 还在 412/429 冷却期
    CoolingDown,
}

#[derive(Debug, Clone, Default)]
pub struct GateState {
    refreshes: Vec<Instant>,
    cooldown_until: Option<Instant>,
}

impl GateState {
    pub const REFRESH_WINDOW: Duration = Duration::from_secs(10 * 60);
    pub const MAX_REFRESHES: usize = 2;
    pub const DEFAULT_COOLDOWN: Duration = Duration::from_secs(60);

    pub fn may_refresh(&self, now: Instant) -> RefreshDecision {
        if let Some(until) = self.cooldown_until {
            if now < until {
                return RefreshDecision::CoolingDown;
            }
        }
        let recent = self
            .refreshes
            .iter()
            .filter(|at| now.duration_since(**at) < Self::REFRESH_WINDOW)
            .count();
        if recent >= Self::MAX_REFRESHES {
            RefreshDecision::BudgetExhausted
        } else {
            RefreshDecision::Allowed
        }
    }

    pub fn note_refresh(&mut self, now: Instant) {
        self.refreshes
            .retain(|at| now.duration_since(*at) < Self::REFRESH_WINDOW);
        self.refreshes.push(now);
    }

    pub fn note_block(&mut self, now: Instant, retry_after: Option<Duration>) {
        let cooldown = retry_after.unwrap_or(Self::DEFAULT_COOLDOWN);
        let until = now + cooldown;
        self.cooldown_until = Some(match self.cooldown_until {
            Some(existing) if existing > until => existing,
            _ => until,
        });
    }

    pub fn cooldown_remaining(&self, now: Instant) -> Option<Duration> {
        self.cooldown_until
            .filter(|until| now < *until)
            .map(|until| until - now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_ordered_deduped_candidates_and_upgrades_http() {
        let urls: Vec<String> = select_candidates([
            "https://a.bilivideo.com/1.mp4",
            "http://a.bilivideo.com/1.mp4",
            "https://b.bilivideo.com/2.mp4",
            "https://evil.example.com/3.mp4",
            "https://c.bilivideo.cn/4.mp4",
            "https://d.bilivideo.com/5.mp4",
        ])
        .into_iter()
        .map(|u| u.to_string())
        .collect();
        // http 升级后与主地址等价 → 去重；evil 被拒；只保留前 3 个
        assert_eq!(
            urls,
            vec![
                "https://a.bilivideo.com/1.mp4",
                "https://b.bilivideo.com/2.mp4",
                "https://c.bilivideo.cn/4.mp4",
            ]
        );
        assert_eq!(urls.len(), MAX_CANDIDATES);
    }

    #[test]
    fn accepts_only_https_bilibili_cdn_hosts() {
        assert!(
            validate_candidate("https://cn-bj-cc-03-01.bilivideo.com/upgcxcode/1.mp4?e=1")
                .is_some()
        );
        assert!(validate_candidate("https://xy1x2x3x4xy.mcdn.bilivideo.cn:4483/v.mp4").is_some());
        assert!(validate_candidate("https://upos-hz-mirrorakam.akamaized.net/a.mp4").is_some());
        assert!(validate_candidate("https://A.BILIVIDEO.COM:443/a.mp4").is_some());
        assert!(validate_candidate("https://a.bilivideo.com./a.mp4").is_some());
    }

    #[test]
    fn rejects_loopholes_that_are_not_bilibili_cdn() {
        for raw in [
            "https://evil.com/bilivideo.com.mp4",
            "https://bilivideo.com.evil.com/a.mp4",
            "http://evil.com/a.mp4",
            "https://user:pw@a.bilivideo.com/a.mp4",
            "https://user@a.bilivideo.com/a.mp4",
            "https://127.0.0.1/a.mp4",
            "https://[::1]/a.mp4",
            "https://localhost/a.mp4",
            "https://10.0.0.1/a.mp4",
            "https://192.168.1.1/a.mp4",
            "https://169.254.169.254/a.mp4",
            "https://a.bilivideo.com:8443/a.mp4",
            "https://%61.bilivideo.com/a.mp4",
            "https://a%2ebilivideo.com/a.mp4",
            "https://a.bilivideo.com\\@evil.com/a.mp4",
            "https://ｅvil.com/a.mp4",
            "not a url",
            "",
        ] {
            assert!(validate_candidate(raw).is_none(), "should reject: {raw}");
        }
    }

    #[test]
    fn every_redirect_is_revalidated() {
        let base = Url::parse("https://a.bilivideo.com/x/y.mp4?sig=1").unwrap();
        assert_eq!(
            validate_redirect(&base, "https://b.bilivideo.com/z.mp4")
                .unwrap()
                .as_str(),
            "https://b.bilivideo.com/z.mp4"
        );
        assert_eq!(
            validate_redirect(&base, "/z.mp4").unwrap().as_str(),
            "https://a.bilivideo.com/z.mp4"
        );
        assert_eq!(
            validate_redirect(&base, "z2.mp4").unwrap().as_str(),
            "https://a.bilivideo.com/x/z2.mp4"
        );
        for bad in [
            "https://evil.com/z.mp4",
            "http://a.bilivideo.com/z.mp4",
            "//evil.com/z.mp4",
            "https://127.0.0.1/z.mp4",
            "https://localhost/z.mp4",
            "\\\\evil.com\\z.mp4",
            "https://a.bilivideo.com:8443/z.mp4",
            "https://user:pw@a.bilivideo.com/z.mp4",
            "",
        ] {
            assert!(
                validate_redirect(&base, bad).is_none(),
                "redirect should be rejected: {bad}"
            );
        }
    }

    #[test]
    fn resolution_must_be_public() {
        assert!(ensure_public_addrs(&["1.1.1.1:443".parse().unwrap()]).is_ok());
        assert!(ensure_public_addrs(&["[2606:4700:4700::1111]:443".parse().unwrap()]).is_ok());
        assert_eq!(ensure_public_addrs(&[]), Err(DnsReject::NoAddress));
        for private in [
            "127.0.0.1:443",
            "10.1.2.3:443",
            "192.168.0.1:443",
            "172.16.0.1:443",
            "169.254.169.254:443",
            "100.64.0.1:443",
            "0.0.0.0:443",
            "[::1]:443",
            "[fe80::1]:443",
            "[fd00::1]:443",
            "[::ffff:127.0.0.1]:443",
        ] {
            assert_eq!(
                ensure_public_addrs(&[private.parse().unwrap()]),
                Err(DnsReject::PrivateAddress),
                "should reject {private}"
            );
        }
        assert_eq!(
            ensure_public_addrs(&[
                "1.1.1.1:443".parse().unwrap(),
                "10.0.0.1:443".parse().unwrap()
            ]),
            Err(DnsReject::PrivateAddress)
        );
        assert!(!is_public_ip(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(!is_public_ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(is_public_ip(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))));
    }

    #[test]
    fn classifies_status_codes() {
        assert_eq!(classify_status(206), StatusClass::Range);
        assert_eq!(classify_status(416), StatusClass::Range);
        assert_eq!(classify_status(403), StatusClass::Expired);
        assert_eq!(classify_status(410), StatusClass::Expired);
        assert_eq!(classify_status(429), StatusClass::RateLimited);
        assert_eq!(classify_status(412), StatusClass::RateLimited);
        assert_eq!(classify_status(401), StatusClass::Auth);
        assert_eq!(classify_status(503), StatusClass::Retryable);
        assert_eq!(classify_status(400), StatusClass::Fatal);
    }

    #[test]
    fn parses_retry_after_seconds_and_caps_it() {
        assert_eq!(
            parse_retry_after(Some("120")),
            Some(Duration::from_secs(120))
        );
        assert_eq!(parse_retry_after(Some(" 5 ")), Some(Duration::from_secs(5)));
        assert_eq!(parse_retry_after(Some("0")), Some(Duration::from_secs(0)));
        assert_eq!(
            parse_retry_after(Some("99999")),
            Some(Duration::from_secs(600))
        );
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            None
        );
        assert_eq!(parse_retry_after(None), None);
    }

    #[test]
    fn refresh_budget_is_bounded_and_412_enters_cooldown() {
        let now = Instant::now();
        let mut gate = GateState::default();
        assert_eq!(gate.may_refresh(now), RefreshDecision::Allowed);
        gate.note_refresh(now);
        assert_eq!(gate.may_refresh(now), RefreshDecision::Allowed);
        gate.note_refresh(now);
        assert_eq!(gate.may_refresh(now), RefreshDecision::BudgetExhausted);
        assert_eq!(
            gate.may_refresh(now + GateState::REFRESH_WINDOW + Duration::from_secs(1)),
            RefreshDecision::Allowed
        );

        let mut gate = GateState::default();
        gate.note_block(now, Some(Duration::from_secs(30)));
        assert_eq!(gate.may_refresh(now), RefreshDecision::CoolingDown);
        assert_eq!(gate.cooldown_remaining(now), Some(Duration::from_secs(30)));
        assert_eq!(
            gate.may_refresh(now + Duration::from_secs(31)),
            RefreshDecision::Allowed
        );

        let mut gate = GateState::default();
        gate.note_block(now, None);
        assert_eq!(
            gate.cooldown_remaining(now),
            Some(GateState::DEFAULT_COOLDOWN)
        );
    }
}
