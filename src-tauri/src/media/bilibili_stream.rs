//! `bilistream://` 协议：在应用自己的 `<video>` 里播放 B 站链接条目。
//!
//! 外链播放器（player.bilibili.com iframe）在应用 WebView 里常一直缓冲：第三方 Cookie 被拦、
//! 风控、UA 识别都不在我们控制之内，也拿不到进度 / 暂停 / 截帧。这里改为：
//! 1. `x/player/playurl`（html5 平台、`fnval=1`）取单文件 MP4 地址（登录后带会话 Cookie），
//!    只接受 B 站视频 CDN 域名，按条目缓存约 50 分钟（地址本身带过期时间）；
//! 2. 媒体元素的 Range 请求转发到 CDN（带 B 站 Referer），每次最多回
//!    [`STREAM_CHUNK_BYTES`]，播放器按 `Content-Range` 继续请求后续区间；
//! 3. CDN 返回 403 / 404 / 410（地址过期）时丢弃缓存重新取一次地址。
//!
//! URL 形如 `bilistream://localhost/{fileId}?qn=64&e=0&s=<session>`（Windows / Android 为
//! `http://bilistream.localhost/...`），`fileId` 必须是 VFS 里的 B 站链接条目；
//! 转发目标只来自 B 站接口，不接受请求方给的地址。
//!
//! 协议契约（见 [`super::http_range`]）：整个 handler（取址 + 刷新 + 轮换 + 读取）共享一个
//! 小于平台等待窗口的绝对预算；每次响应 body 有界；HEAD 报告完整长度；支持
//! `bytes=0-` / 有限区间 / 后缀区间 / `416`；上游忽略 Range 时失败换候选而不是整片缓冲。
//!
//! 诊断：每次播放会话（`s=`）与窗口绑定，失败/阶段事件只 `emit_to` 发起窗口，
//! 只含脱敏字段（stage / code / CDN host / range / 字节数 / 耗时），不含 URL、Cookie、标题。

use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use reqwest::header::{
    HeaderMap, HeaderValue, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, LOCATION, RANGE, REFERER,
};
use serde::{Deserialize, Serialize};
use tauri::http::{Request, Response};
use tauri::{AppHandle, Emitter, EventTarget, Manager};
use url::Url;

use super::bilibili::{read_link_descriptor, BiliClient, BiliLinkDescriptor};
use super::bilibili_cdn::{self, GateState, RefreshDecision, StatusClass};
use super::http_range::{self, ByteSpan, RangeRequest};
use super::MediaError;
use crate::vfs::database::VfsDatabase;

/// 单次响应的最大字节数（约 10 秒 1080P）
pub const STREAM_CHUNK_BYTES: u64 = http_range::DEFAULT_CHUNK_BYTES;
const PLAY_SOURCE_TTL: Duration = Duration::from_secs(50 * 60);
const PLAY_CACHE_MAX_ENTRIES: usize = 32;
/// 整个协议 handler（取址 + 刷新 + 轮换 + 读取）的绝对预算。
/// 平台侧异步协议等待窗口是 30s（Android 上 `10s × 3`），必须留出调度余量。
pub const PROTOCOL_BUDGET: Duration = Duration::from_secs(25);
/// 单个候选的尝试上限（受绝对预算再收窄）
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);
/// 单次 playurl 解析上限
const PLAYURL_TIMEOUT: Duration = Duration::from_secs(8);
/// 重定向最多跟随次数（每次都要重新校验）
const MAX_REDIRECTS: usize = 3;
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0 Safari/537.36";

/// 不带 `qn` 时请求的清晰度（1080P；拿不到时 B 站自动降档）
pub const DEFAULT_QN: u32 = 80;
/// fnval=1 单文件 MP4 能给到的清晰度：240P / 360P / 480P / 720P / 1080P。
/// 720P60（74）、1080P+（112）及以上只有 DASH 分轨，`<video>` 直接放不了，不列出。
pub const MP4_QUALITIES: &[u32] = &[6, 16, 32, 64, 80];

/// 结构化诊断事件（脱敏；只发到发起播放的窗口）
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StreamTrace {
    pub session_id: String,
    pub request_id: u64,
    /// prepare / resolve / connect / headers / first-byte / complete / error / cancel
    pub stage: &'static str,
    pub code: Option<String>,
    /// 只记 CDN 主机名，不记路径 / 签名参数
    pub host: Option<String>,
    pub range: Option<String>,
    pub bytes: Option<u64>,
    pub elapsed_ms: u64,
}

/// 事件名（前端只监听自己窗口里同一 session 的事件）
pub const STREAM_EVENT: &str = "media-bilibili-stream";

pub trait TraceSink: Send + Sync {
    fn emit(&self, event: &StreamTrace);
}

pub struct NoopTrace;

impl TraceSink for NoopTrace {
    fn emit(&self, _event: &StreamTrace) {}
}

/// 只发给发起播放的 webview 窗口
pub struct WindowTrace {
    app: AppHandle,
    label: String,
}

impl WindowTrace {
    pub fn new(app: AppHandle, label: impl Into<String>) -> Self {
        Self {
            app,
            label: label.into(),
        }
    }
}

impl TraceSink for WindowTrace {
    fn emit(&self, event: &StreamTrace) {
        // 只投递到发起播放的那个 webview（不是 app 级广播）
        let target = EventTarget::Webview {
            label: self.label.clone(),
        };
        let _ = self.app.emit_to(target, STREAM_EVENT, event);
    }
}

#[derive(Debug, Clone)]
pub struct Limits {
    pub budget: Duration,
    pub per_attempt: Duration,
    pub chunk: u64,
    pub max_candidates: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            budget: PROTOCOL_BUDGET,
            per_attempt: ATTEMPT_TIMEOUT,
            chunk: STREAM_CHUNK_BYTES,
            max_candidates: bilibili_cdn::MAX_CANDIDATES,
        }
    }
}

/// handler 的结构化失败（code 是稳定、可分享的错误码）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeError {
    pub code: &'static str,
    pub message: String,
}

impl ServeError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

type ResolveFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<Url>, ServeError>> + Send + 'a>>;

/// 播放地址来源（生产实现见 [`LiveProvider`]；测试用合成候选）
pub trait PlayProvider: Send + Sync {
    fn resolve<'a>(&'a self, fresh: bool) -> ResolveFuture<'a>;
}

/// 一次播放请求的关联信息
#[derive(Debug, Clone)]
pub struct ServeCtx<'a> {
    pub file_id: &'a str,
    pub qn: u32,
    pub epoch: u32,
    pub session: Option<&'a str>,
    pub request_id: u64,
}

/// 一档清晰度（`qn` + B 站给的名称，如「高清 720P」）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityOption {
    pub qn: u32,
    pub label: String,
}

/// 一次 playurl 解析的结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlayInfo {
    /// 主地址 + 备用，已按白名单过滤、去重
    urls: Vec<Url>,
    /// B 站实际给的清晰度（可能低于请求的 qn）
    quality: u32,
    /// 本条目可选的清晰度（高 → 低）
    options: Vec<QualityOption>,
}

#[derive(Debug, Clone)]
struct PlaySource {
    info: PlayInfo,
    resolved_at: Instant,
}

static PLAY_CACHE: LazyLock<Mutex<HashMap<String, PlaySource>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static RESOLVE_GATES: LazyLock<Mutex<HashMap<String, GateState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static RESOLVE_FLIGHTS: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// session → 该 session 绑定窗口与条目；未登记的 session 不能触发事件
static SESSIONS: LazyLock<Mutex<HashMap<String, SessionInfo>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// (条目, 账号代次) → 最新 session：旧 session 的延迟结果必须停下，不污染新条目
static LATEST_SESSION: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static REQUEST_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
struct SessionInfo {
    file_id: String,
    epoch: u32,
    webview: Option<String>,
    created: Instant,
}

/// 注册表硬上限（进程内长期运行，不能无界增长）
const MAX_SESSIONS: usize = 256;
const MAX_RESOLVE_KEYS: usize = 256;

fn session_key(file_id: &str, epoch: u32) -> String {
    format!("{}@{}", file_id, epoch)
}

/// 登记播放会话（协议请求是唯一的登记入口；quality 查询不能登记）
fn register_session(session: &str, file_id: &str, epoch: u32, webview: Option<&str>) -> bool {
    if session.is_empty() || session.len() > 64 || !session.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    let Ok(mut sessions) = SESSIONS.lock() else {
        return false;
    };
    if sessions.len() >= MAX_SESSIONS {
        // 先淘汰最老的会话，再插入：硬上限，不随打开次数增长
        if let Some(oldest) = sessions
            .iter()
            .min_by_key(|(_, info)| info.created)
            .map(|(id, _)| id.clone())
        {
            sessions.remove(&oldest);
        }
    }
    sessions.insert(
        session.to_string(),
        SessionInfo {
            file_id: file_id.to_string(),
            epoch,
            webview: webview.map(str::to_string),
            created: Instant::now(),
        },
    );
    if let Ok(mut latest) = LATEST_SESSION.lock() {
        if latest.len() >= MAX_SESSIONS {
            if let Some(oldest) = latest
                .iter()
                .min_by_key(|(_, session_id)| {
                    sessions
                        .get(*session_id)
                        .map(|info| info.created)
                        .unwrap_or(Instant::now())
                })
                .map(|(key, _)| key.clone())
            {
                latest.remove(&oldest);
            }
        }
        latest.insert(session_key(file_id, epoch), session.to_string());
    }
    true
}

#[cfg(test)]
fn session_registry_len() -> usize {
    SESSIONS.lock().map(|sessions| sessions.len()).unwrap_or(0)
}

/// session 是否由该窗口在本条目上登记过（事件定向的前置条件）
fn session_bound_to(session: &str, file_id: &str, epoch: u32, webview: Option<&str>) -> bool {
    let Ok(sessions) = SESSIONS.lock() else {
        return false;
    };
    sessions
        .get(session)
        .map(|info| {
            info.file_id == file_id && info.epoch == epoch && info.webview.as_deref() == webview
        })
        .unwrap_or(false)
}

fn request_is_current(ctx: &ServeCtx<'_>) -> bool {
    let Some(session) = ctx.session else {
        return true;
    };
    let Ok(latest) = LATEST_SESSION.lock() else {
        return true;
    };
    latest
        .get(&session_key(ctx.file_id, ctx.epoch))
        .map(|current| current == session)
        .unwrap_or(true)
}

/// 播放会话的取消令牌：新 session 登记后，旧 session 正在进行的 send / read 必须中断
#[derive(Debug, Clone)]
struct CancelToken {
    file_id: String,
    epoch: u32,
    session: String,
}

impl CancelToken {
    fn from_ctx(ctx: &ServeCtx<'_>) -> Option<Self> {
        ctx.session.map(|session| Self {
            file_id: ctx.file_id.to_string(),
            epoch: ctx.epoch,
            session: session.to_string(),
        })
    }

    fn is_current(&self) -> bool {
        match LATEST_SESSION.lock() {
            Ok(latest) => latest
                .get(&session_key(&self.file_id, self.epoch))
                .map(|current| current == &self.session)
                .unwrap_or(true),
            Err(_) => true,
        }
    }
}

/// 取消等待：每 100ms 检查一次，配合 `select!` 打断进行中的网络 await
async fn wait_cancelled(token: &Option<CancelToken>) {
    let Some(token) = token else {
        std::future::pending::<()>().await;
        return;
    };
    loop {
        if !token.is_current() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn url_host(url: &Url) -> String {
    url.host_str().unwrap_or("").to_ascii_lowercase()
}

// ============================================================================
// 纯函数（可单测）
// ============================================================================

/// 请求路径 → 条目 id（`/file_xxx`，可能被百分号编码）
fn parse_stream_path(path: &str) -> Option<String> {
    let first = path.trim_start_matches('/').split('/').next()?;
    let decoded = urlencoding::decode(first).ok()?.into_owned();
    let valid = (decoded.starts_with("file_") || decoded.starts_with("att_"))
        && decoded.len() <= 64
        && decoded
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    valid.then_some(decoded)
}

/// 地址查询串里的一个参数
fn query_param<'a>(query: Option<&'a str>, name: &str) -> Option<&'a str> {
    query
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

fn query_u32(query: Option<&str>, name: &str) -> Option<u32> {
    query_param(query, name).and_then(|v| v.parse::<u32>().ok())
}

/// 请求的清晰度；缺省 / 不在 MP4 白名单时用 [`DEFAULT_QN`]
fn normalize_qn(qn: Option<u32>) -> u32 {
    qn.filter(|q| MP4_QUALITIES.contains(q))
        .unwrap_or(DEFAULT_QN)
}

/// 播放地址的 `(qn, 账号代次, session)`：`?qn=64&e=1&s=<hex>`。
/// 账号代次由前端在登录 / 退出后递增：同一个媒体地址始终对应同一个 CDN 文件（换文件会让
/// 播放中的 Range 请求读到另一份字节），登录后能拿到的清晰度变了，就换一个新地址重新取。
fn parse_stream_query(query: Option<&str>) -> (u32, u32, Option<String>) {
    let session = query_param(query, "s")
        .filter(|s| !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_string);
    (
        normalize_qn(query_u32(query, "qn")),
        query_u32(query, "e").unwrap_or(0),
        session,
    )
}

/// 播放地址缓存键：同一条目不同清晰度 / 不同账号代次的地址不同
fn cache_key(file_id: &str, qn: u32, epoch: u32) -> String {
    format!("{}@{}@{}", file_id, qn, epoch)
}

/// B 站没给名称时的兜底文案
fn fallback_quality_label(qn: u32) -> String {
    match qn {
        6 => "240P".into(),
        16 => "360P".into(),
        32 => "480P".into(),
        64 => "720P".into(),
        80 => "1080P".into(),
        other => format!("qn {}", other),
    }
}

/// `accept_quality` / `accept_description` → MP4 能播的清晰度（高 → 低，去重）；
/// 实际返回的 `quality` 总在列表里
fn mp4_quality_options(data: &PlayUrlData) -> Vec<QualityOption> {
    let mut options: Vec<QualityOption> = data
        .accept_quality
        .iter()
        .enumerate()
        .map(|(i, &qn)| (qn, data.accept_description.get(i)))
        .chain(data.quality.map(|q| (q, None)))
        .filter(|(qn, _)| MP4_QUALITIES.contains(qn))
        .map(|(qn, label)| QualityOption {
            qn,
            label: label
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .unwrap_or_else(|| fallback_quality_label(qn)),
        })
        .collect();
    options.sort_by_key(|option| std::cmp::Reverse(option.qn));
    options.dedup_by_key(|o| o.qn);
    options
}

fn parse_play_info(data: &PlayUrlData, requested_qn: u32) -> Result<PlayInfo, MediaError> {
    let urls = pick_play_urls(data)?;
    Ok(PlayInfo {
        urls,
        quality: data.quality.unwrap_or(requested_qn),
        options: mp4_quality_options(data),
    })
}

#[derive(Debug, Deserialize)]
struct PlayUrlData {
    #[serde(default)]
    durl: Vec<PlayUrlSegment>,
    #[serde(default)]
    quality: Option<u32>,
    #[serde(default)]
    accept_quality: Vec<u32>,
    #[serde(default)]
    accept_description: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PlayUrlSegment {
    url: String,
    #[serde(default)]
    backup_url: Option<Vec<String>>,
}

/// 只接受单文件 MP4；主地址 + `backup_url` 按顺序过滤成候选（http 升级为 https）
fn pick_play_urls(data: &PlayUrlData) -> Result<Vec<Url>, MediaError> {
    if data.durl.len() != 1 {
        return Err(MediaError::InvalidInput(format!(
            "B 站给出的是 {} 段的旧格式视频，无法在应用内直接播放",
            data.durl.len()
        )));
    }
    let segment = &data.durl[0];
    let urls = bilibili_cdn::select_candidates(
        std::iter::once(&segment.url).chain(segment.backup_url.iter().flatten()),
    );
    if urls.is_empty() {
        return Err(MediaError::InvalidInput(
            "B 站返回的视频地址不在 B 站 CDN 上".into(),
        ));
    }
    Ok(urls)
}

// ============================================================================
// 取地址
// ============================================================================

pub(crate) async fn resolve_play_info(
    client: &BiliClient,
    descriptor: &BiliLinkDescriptor,
    qn: u32,
) -> Result<PlayInfo, MediaError> {
    let data: PlayUrlData = client
        .get_api(
            "/x/player/playurl",
            &[
                ("bvid", descriptor.bvid.clone()),
                ("cid", descriptor.cid.to_string()),
                ("qn", qn.to_string()),
                ("fnval", "1".into()),
                ("fnver", "0".into()),
                ("fourk", "0".into()),
                ("platform", "html5".into()),
                ("high_quality", "1".into()),
            ],
        )
        .await?
        .ok_or_else(|| MediaError::InvalidInput("B 站没有返回播放地址".into()))?;
    parse_play_info(&data, qn)
}

fn cached_source(key: &str) -> Option<PlayInfo> {
    let cache = PLAY_CACHE.lock().ok()?;
    cache
        .get(key)
        .filter(|s| s.resolved_at.elapsed() < PLAY_SOURCE_TTL)
        .map(|s| s.info.clone())
}

fn remember_source(key: &str, info: &PlayInfo) {
    if let Ok(mut cache) = PLAY_CACHE.lock() {
        cache.retain(|_, s| s.resolved_at.elapsed() < PLAY_SOURCE_TTL);
        if cache.len() >= PLAY_CACHE_MAX_ENTRIES {
            cache.clear();
        }
        cache.insert(
            key.to_string(),
            PlaySource {
                info: info.clone(),
                resolved_at: Instant::now(),
            },
        );
    }
}

fn forget_source(key: &str) {
    if let Ok(mut cache) = PLAY_CACHE.lock() {
        cache.remove(key);
    }
}

/// 同一 key 的解析只跑一次（并发 Range 共享结果与刷新任务，避免刷新风暴）
fn flight_for(key: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut flights = match RESOLVE_FLIGHTS.lock() {
        Ok(flights) => flights,
        Err(poisoned) => poisoned.into_inner(),
    };
    if flights.len() >= MAX_RESOLVE_KEYS && !flights.contains_key(key) {
        // 只回收没有在等锁 / 没有在解析的 key，避免打断进行中的 single-flight
        flights.retain(|_, flight| Arc::strong_count(flight) > 1);
        if flights.len() >= MAX_RESOLVE_KEYS {
            if let Some(victim) = flights.keys().next().cloned() {
                flights.remove(&victim);
            }
        }
    }
    flights
        .entry(key.to_string())
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

fn with_gate<T>(key: &str, f: impl FnOnce(&mut GateState) -> T) -> T {
    let mut gates = match RESOLVE_GATES.lock() {
        Ok(gates) => gates,
        Err(poisoned) => poisoned.into_inner(),
    };
    if gates.len() >= MAX_RESOLVE_KEYS && !gates.contains_key(key) {
        let now = Instant::now();
        // 先回收窗口已过且无冷却的 key，仍有上限时再淘汰任意一个
        gates.retain(|_, gate| {
            gate.cooldown_remaining(now).is_some()
                || !matches!(gate.may_refresh(now), RefreshDecision::Allowed)
        });
        if gates.len() >= MAX_RESOLVE_KEYS {
            if let Some(victim) = gates.keys().next().cloned() {
                gates.remove(&victim);
            }
        }
    }
    f(gates.entry(key.to_string()).or_default())
}

async fn play_info_for(
    app: &AppHandle,
    file_id: &str,
    qn: u32,
    epoch: u32,
    fresh: bool,
) -> Result<PlayInfo, ServeError> {
    let key = cache_key(file_id, qn, epoch);
    if !fresh {
        if let Some(info) = cached_source(&key) {
            return Ok(info);
        }
    }
    let flight = flight_for(&key);
    let _guard = flight.lock().await;
    // 等锁期间别的请求可能已经解析完
    if let Some(info) = cached_source(&key) {
        return Ok(info);
    }
    if fresh {
        let decision = with_gate(&key, |gate| gate.may_refresh(Instant::now()));
        match decision {
            RefreshDecision::Allowed => {}
            RefreshDecision::CoolingDown => {
                return Err(ServeError::new(
                    "cdn-rate-limited",
                    "B 站限流冷却中，请稍后重试",
                ))
            }
            RefreshDecision::BudgetExhausted => {
                return Err(ServeError::new(
                    "playurl-refresh-exhausted",
                    "播放地址刷新次数已达上限，请稍后重试",
                ))
            }
        }
    }
    let db: Arc<VfsDatabase> = Arc::clone(app.state::<Arc<VfsDatabase>>().inner());
    let id = file_id.to_string();
    let descriptor = tokio::task::spawn_blocking(move || read_link_descriptor(&db, &id))
        .await
        .map_err(|e| ServeError::new("descriptor-failed", e.to_string()))?
        .map_err(|e| ServeError::new("descriptor-failed", e.to_string()))?;
    let client = BiliClient::new()
        .map_err(|e| ServeError::new("playurl-failed", e.to_string()))?
        .with_cookie(super::bilibili_auth::load_cookie(app));
    let info =
        match tokio::time::timeout(PLAYURL_TIMEOUT, resolve_play_info(&client, &descriptor, qn))
            .await
        {
            Ok(Ok(info)) => info,
            Ok(Err(e)) => return Err(ServeError::new("playurl-failed", e.to_string())),
            Err(_) => return Err(ServeError::new("playurl-timeout", "获取 B 站播放地址超时")),
        };
    if fresh {
        with_gate(&key, |gate| gate.note_refresh(Instant::now()));
    }
    remember_source(&key, &info);
    Ok(info)
}

struct LiveProvider<'a> {
    app: &'a AppHandle,
    file_id: &'a str,
    qn: u32,
    epoch: u32,
}

impl PlayProvider for LiveProvider<'_> {
    fn resolve<'a>(&'a self, fresh: bool) -> ResolveFuture<'a> {
        Box::pin(async move {
            play_info_for(self.app, self.file_id, self.qn, self.epoch, fresh)
                .await
                .map(|info| info.urls)
        })
    }
}

/// 前端清晰度菜单需要的信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamQualityView {
    /// 请求的清晰度（与 `bilistream://` 地址里的 qn 一致）
    pub requested: u32,
    /// B 站实际给的清晰度（降档时低于 requested）
    pub current: u32,
    /// 可选清晰度（高 → 低）
    pub options: Vec<QualityOption>,
}

/// 链接条目在应用内播放时的可选清晰度与实际清晰度。
/// 与 `bilistream://` 共用按 `(条目, qn, 账号代次)` 的地址缓存，播放器随后的请求不会再取一次地址。
/// `session` 只是关联键：只有已由协议请求登记的 session 才能触发窗口事件，查询本身不授权。
#[tauri::command]
pub async fn media_bilibili_stream_quality(
    app: AppHandle,
    file_id: String,
    qn: Option<u32>,
    epoch: Option<u32>,
    session: Option<String>,
) -> Result<StreamQualityView, String> {
    let file_id = parse_stream_path(&file_id).ok_or_else(|| "无效的 B 站链接条目".to_string())?;
    let requested = normalize_qn(qn);
    let _ = session; // 仅用于前端日志关联；未登记不影响查询，也不产生事件
    let info = play_info_for(&app, &file_id, requested, epoch.unwrap_or(0), false)
        .await
        .map_err(|e| format!("{}: {}", e.code, e.message))?;
    Ok(StreamQualityView {
        requested,
        current: info.quality,
        options: info.options,
    })
}

// ============================================================================
// 协议处理
// ============================================================================

fn response(
    request: &Request<Vec<u8>>,
    status: u16,
    headers: &[(&str, String)],
    body: Vec<u8>,
) -> Response<Vec<u8>> {
    let origin = crate::file_stream_protocol::cors_origin_for_request(request);
    let mut builder = Response::builder()
        .status(status)
        .header("Access-Control-Allow-Origin", origin)
        .header("Access-Control-Allow-Methods", "GET, HEAD, OPTIONS")
        .header("Access-Control-Allow-Headers", "Range")
        .header(
            "Access-Control-Expose-Headers",
            "Content-Range, Content-Length, Accept-Ranges, X-Media-Error-Code",
        )
        .header("Vary", "Origin")
        .header("Cache-Control", "no-store");
    for (name, value) in headers {
        builder = builder.header(*name, value.as_str());
    }
    builder
        .body(body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn error_response(
    request: &Request<Vec<u8>>,
    status: u16,
    error: &ServeError,
) -> Response<Vec<u8>> {
    response(
        request,
        status,
        &[
            ("Content-Type", "text/plain; charset=utf-8".into()),
            ("X-Media-Error-Code", error.code.to_string()),
        ],
        error.message.as_bytes().to_vec(),
    )
}

enum Upstream {
    Ok {
        status: u16,
        content_range: Option<String>,
        content_type: Option<String>,
        /// 探测到的完整长度（`Content-Range: bytes S-E/T` 的 T）
        total: Option<u64>,
        body: Vec<u8>,
    },
    /// 地址过期 / 被拒（可刷新一次 playurl）
    Expired(u16),
    /// 限流：冷却，不立即重试
    RateLimited(Option<Duration>),
    /// 5xx：换候选
    Retryable(u16),
    /// 上游 416
    Unsatisfiable(Option<String>),
    /// 请求所属 session 已被新的播放取代
    Cancelled,
    Failed(ServeError),
}

/// 有界读取响应体：最多读 `max` 字节，超出即判错（绝不整片缓冲）。
/// 每一段读取都受绝对 deadline 约束，并且可被新 session 取消。
async fn read_bounded(
    resp: reqwest::Response,
    max: u64,
    deadline: Instant,
    token: &Option<CancelToken>,
) -> Result<Vec<u8>, ServeError> {
    let mut out: Vec<u8> = Vec::new();
    let mut resp = resp;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ServeError::new("budget-exceeded", "B 站视频加载超时"));
        }
        let next = tokio::select! {
            biased;
            _ = wait_cancelled(token) => {
                return Err(ServeError::new("session-cancelled", "播放已被新的请求取代"))
            }
            result = tokio::time::timeout(remaining, resp.chunk()) => result,
        };
        match next {
            Err(_) => return Err(ServeError::new("budget-exceeded", "B 站视频加载超时")),
            Ok(Err(error)) => return Err(reqwest_error("read", &error)),
            Ok(Ok(None)) => return Ok(out),
            Ok(Ok(Some(chunk))) => {
                if out.len() as u64 + chunk.len() as u64 > max {
                    return Err(ServeError::new(
                        "cdn-body-too-long",
                        "B 站视频响应超过请求区间",
                    ));
                }
                out.extend_from_slice(&chunk);
            }
        }
    }
}

/// reqwest 错误 → 稳定错误码 + 脱敏文案（Display 可能包含签名 URL，不能进日志 / 响应）。
fn reqwest_error(stage: &'static str, e: &reqwest::Error) -> ServeError {
    let (code, message): (&'static str, &'static str) = match stage {
        "headers" => {
            if e.is_timeout() {
                ("cdn-connect-timeout", "连接 B 站视频服务器超时")
            } else if e.is_connect() {
                ("cdn-connect-failed", "无法连接 B 站视频服务器")
            } else {
                ("cdn-request-failed", "B 站视频请求失败")
            }
        }
        _ => {
            if e.is_timeout() {
                ("cdn-read-timeout", "读取 B 站视频数据超时")
            } else {
                ("cdn-read-failed", "读取 B 站视频数据失败")
            }
        }
    };
    ServeError::new(code, message)
}

fn header_u64(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<u64> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
}

/// 解析目的地址并 pin：域名解析结果必须全是公网地址
async fn pinned_client(url: &Url) -> Result<reqwest::Client, ServeError> {
    let mut builder = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers({
            let mut headers = HeaderMap::new();
            headers.insert(
                REFERER,
                HeaderValue::from_static("https://www.bilibili.com/"),
            );
            headers
        })
        .redirect(reqwest::redirect::Policy::none());
    let host = url.host_str().unwrap_or_default();
    if host.parse::<std::net::IpAddr>().is_err() {
        let port = url.port_or_known_default().unwrap_or(443);
        let addrs: Vec<SocketAddr> = tokio::time::timeout(
            Duration::from_secs(3),
            tokio::net::lookup_host((host, port)),
        )
        .await
        .map_err(|_| ServeError::new("cdn-dns-failed", "解析 B 站视频地址超时"))?
        .map_err(|_| ServeError::new("cdn-dns-failed", "解析 B 站视频地址失败"))?
        .collect();
        bilibili_cdn::ensure_public_addrs(&addrs)
            .map_err(|_| ServeError::new("cdn-dns-blocked", "B 站视频地址解析到非公网地址"))?;
        builder = builder.resolve_to_addrs(host, &addrs);
    }
    builder
        .build()
        .map_err(|_| ServeError::new("cdn-client-failed", "初始化视频请求失败"))
}

/// 单次候选尝试：跟随有限次重定向（每次都校验），有界读取 body
async fn fetch_span(
    url: &Url,
    span: ByteSpan,
    deadline: Instant,
    limits: &Limits,
    token: &Option<CancelToken>,
) -> Upstream {
    let mut current = url.clone();
    for hop in 0..=MAX_REDIRECTS {
        if Instant::now() >= deadline {
            return Upstream::Failed(ServeError::new("budget-exceeded", "B 站视频加载超时"));
        }
        let client = match pinned_client(&current).await {
            Ok(client) => client,
            Err(error) => return Upstream::Failed(error),
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        let attempt_timeout = remaining.min(limits.per_attempt);
        if attempt_timeout.is_zero() {
            return Upstream::Failed(ServeError::new("budget-exceeded", "B 站视频加载超时"));
        }
        let request = client
            .get(current.clone())
            .header(RANGE, format!("bytes={}-{}", span.start, span.end));
        let send_result = tokio::select! {
            biased;
            _ = wait_cancelled(token) => return Upstream::Cancelled,
            response = tokio::time::timeout(attempt_timeout, request.send()) => response,
        };
        let resp = match send_result {
            Err(_) => {
                return Upstream::Failed(ServeError::new(
                    "cdn-connect-timeout",
                    "连接 B 站视频服务器超时",
                ))
            }
            Ok(Err(e)) => return Upstream::Failed(reqwest_error("headers", &e)),
            Ok(Ok(resp)) => resp,
        };
        let status = resp.status().as_u16();
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let Some(location) = resp.headers().get(LOCATION).and_then(|v| v.to_str().ok()) else {
                return Upstream::Failed(ServeError::new(
                    "cdn-redirect-invalid",
                    "B 站视频地址跳转无效",
                ));
            };
            let Some(next) = bilibili_cdn::validate_redirect(&current, location) else {
                return Upstream::Failed(ServeError::new(
                    "cdn-redirect-blocked",
                    "B 站视频地址跳转到未授权地址",
                ));
            };
            if hop == MAX_REDIRECTS {
                return Upstream::Failed(ServeError::new(
                    "cdn-redirect-loop",
                    "B 站视频地址跳转过多",
                ));
            }
            current = next;
            continue;
        }
        match bilibili_cdn::classify_status(status) {
            StatusClass::Expired => return Upstream::Expired(status),
            StatusClass::RateLimited => {
                let retry_after = bilibili_cdn::parse_retry_after(
                    resp.headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok()),
                );
                return Upstream::RateLimited(retry_after);
            }
            StatusClass::Auth => {
                return Upstream::Failed(ServeError::new(
                    "cdn-forbidden",
                    "B 站拒绝了视频请求（鉴权或风控）",
                ))
            }
            StatusClass::Retryable => return Upstream::Retryable(status),
            StatusClass::Fatal => {
                return Upstream::Failed(ServeError::new(
                    "cdn-http-error",
                    format!("B 站视频服务器返回 HTTP {status}"),
                ))
            }
            StatusClass::Range => {}
        }
        if status == 416 {
            return Upstream::Unsatisfiable(
                resp.headers()
                    .get(CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string),
            );
        }
        let content_type = resp
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let content_length = header_u64(resp.headers(), CONTENT_LENGTH);
        if status == 200 {
            // 上游忽略 Range：只有从 0 开始的请求可以截前段，绝不整片缓冲 / 冒充局部字节
            let total = http_range::total_from_probe(status, None, content_length);
            match http_range::plan_ignored_range(span.start, span.len(), total, limits.chunk) {
                Ok(plan) => match read_bounded(resp, plan.span.len(), deadline, token).await {
                    Ok(body) => {
                        return Upstream::Ok {
                            status: 206,
                            content_range: Some(http_range::content_range(plan.span, plan.total)),
                            content_type,
                            total: plan.total,
                            body,
                        }
                    }
                    Err(error) => return Upstream::Failed(error),
                },
                Err(_) => {
                    return Upstream::Failed(ServeError::new(
                        "range-not-honored",
                        "B 站视频服务器忽略了区间请求",
                    ))
                }
            }
        }
        // 206
        let content_range = resp
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let max_bytes = some_min(span.len(), limits.chunk);
        let body = match read_bounded(resp, max_bytes, deadline, token).await {
            Ok(body) => body,
            Err(error) => return Upstream::Failed(error),
        };
        match http_range::validate_partial_response(span, content_range.as_deref(), body.len()) {
            Ok(actual) => {
                let total = content_range
                    .as_deref()
                    .and_then(http_range::parse_content_range)
                    .and_then(|cr| cr.total);
                return Upstream::Ok {
                    status: 206,
                    content_range: Some(http_range::content_range(actual, total)),
                    content_type,
                    total,
                    body,
                };
            }
            Err(_) => {
                return Upstream::Failed(ServeError::new(
                    "cdn-range-invalid",
                    "B 站视频服务器返回的区间与请求不一致",
                ))
            }
        }
    }
    Upstream::Failed(ServeError::new("cdn-redirect-loop", "B 站视频地址跳转过多"))
}

fn some_min(a: u64, b: u64) -> u64 {
    if a == 0 {
        b
    } else {
        a.min(b)
    }
}

/// 探测完整资源长度（HEAD 与后缀请求需要）。`bytes=0-0` 的 `Content-Range` 才是总长来源。
async fn probe_total(
    provider: &dyn PlayProvider,
    ctx: &ServeCtx<'_>,
    deadline: Instant,
    limits: &Limits,
    token: &Option<CancelToken>,
) -> Result<Option<u64>, ServeError> {
    let mut refreshed = false;
    let mut last_error: Option<ServeError> = None;
    for round in 0..=1 {
        let fresh = round == 1;
        if fresh && refreshed {
            break;
        }
        let urls = match provider.resolve(fresh).await {
            Ok(urls) if !urls.is_empty() => urls,
            Ok(_) => return Err(ServeError::new("no-candidate", "没有可用的视频地址")),
            Err(error) => return Err(error),
        };
        if fresh {
            refreshed = true;
        }
        let mut saw_expired = false;
        for url in urls.iter().take(limits.max_candidates) {
            if Instant::now() >= deadline {
                return Err(ServeError::new("budget-exceeded", "B 站视频加载超时"));
            }
            // HEAD / 后缀探测与普通 GET 走同一套策略：过期刷新一次、429 冷却、可取消
            match fetch_span(url, ByteSpan::new(0, 0), deadline, limits, token).await {
                Upstream::Ok { total, .. } => return Ok(total),
                Upstream::Unsatisfiable(content_range) => {
                    return Ok(content_range
                        .as_deref()
                        .and_then(http_range::parse_content_range)
                        .and_then(|cr| cr.total))
                }
                Upstream::Expired(_) => saw_expired = true,
                Upstream::RateLimited(retry_after) => {
                    let key = cache_key(ctx.file_id, ctx.qn, ctx.epoch);
                    with_gate(&key, |gate| gate.note_block(Instant::now(), retry_after));
                    forget_source(&key);
                    return Err(ServeError::new("cdn-rate-limited", "B 站限流，请稍后重试"));
                }
                Upstream::Cancelled => {
                    return Err(ServeError::new("session-cancelled", "播放已被新的请求取代"))
                }
                Upstream::Retryable(_) => {}
                Upstream::Failed(error) => last_error = Some(error),
            }
        }
        if !saw_expired || fresh {
            break;
        }
        let key = cache_key(ctx.file_id, ctx.qn, ctx.epoch);
        forget_source(&key);
        if !matches!(
            with_gate(&key, |gate| gate.may_refresh(Instant::now())),
            RefreshDecision::Allowed
        ) {
            break;
        }
    }
    if Instant::now() >= deadline {
        return Err(ServeError::new("budget-exceeded", "B 站视频加载超时"));
    }
    match last_error {
        Some(error) => Err(error),
        None => Ok(None),
    }
}

/// 一次请求的 trace 上下文
#[derive(Clone)]
struct TraceBuilder {
    session: String,
    request_id: u64,
    started: Instant,
    host: Option<String>,
    range: Option<String>,
}

impl TraceBuilder {
    fn new(ctx: &ServeCtx<'_>, started: Instant) -> Option<Self> {
        let session = ctx.session?.to_string();
        Some(Self {
            session,
            request_id: ctx.request_id,
            started,
            host: None,
            range: None,
        })
    }

    fn finish(&self, stage: &'static str, code: Option<&str>, bytes: Option<u64>) -> StreamTrace {
        StreamTrace {
            session_id: self.session.clone(),
            request_id: self.request_id,
            stage,
            code: code.map(str::to_string),
            host: self.host.clone(),
            range: self.range.clone(),
            bytes,
            elapsed_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        }
    }
}

/// 协议请求主体（入口解析后调用；provider / trace / limits 可注入，便于合成服务器回归）
pub async fn serve(
    request: &Request<Vec<u8>>,
    ctx: &ServeCtx<'_>,
    provider: &dyn PlayProvider,
    trace: &dyn TraceSink,
    limits: Limits,
) -> Response<Vec<u8>> {
    let started = Instant::now();
    let deadline = started + limits.budget;
    let trace_builder = TraceBuilder::new(ctx, started);
    let emit = |stage: &'static str, code: Option<&str>, bytes: Option<u64>| {
        if let Some(builder) = trace_builder.as_ref() {
            trace.emit(&builder.finish(stage, code, bytes));
        }
    };
    emit("prepare", None, None);

    let fail = |status: u16, error: ServeError| -> Response<Vec<u8>> {
        emit("error", Some(error.code), None);
        error_response(request, status, &error)
    };
    // 结构化失败 → 恰当的 HTTP 状态（前端据此区分可重试 / 已取消 / 限流）
    let fail_from = |error: ServeError| -> Response<Vec<u8>> {
        let status = match error.code {
            "session-cancelled" => 499,
            "budget-exceeded" => 504,
            "cdn-rate-limited" => 429,
            _ => 502,
        };
        fail(status, error)
    };
    let token = CancelToken::from_ctx(ctx);

    let method = request.method().as_str().to_ascii_uppercase();
    if method == "OPTIONS" {
        return response(request, 204, &[], Vec::new());
    }
    if method != "GET" && method != "HEAD" {
        return fail(
            405,
            ServeError::new("method-not-allowed", "Method Not Allowed"),
        );
    }
    let is_head = method == "HEAD";
    let range_header = request
        .headers()
        .get("range")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let parsed = match http_range::parse_range_header(range_header.as_deref()) {
        Ok(parsed) => parsed,
        Err(http_range::RangeError::Unsupported) => {
            return fail(
                416,
                ServeError::new("range-unsupported", "不支持的 Range 请求"),
            )
        }
        Err(http_range::RangeError::Unsatisfiable) => {
            return fail(
                416,
                ServeError::new("range-unsatisfiable", "Range 请求超出视频大小"),
            )
        }
    };

    if is_head && matches!(parsed, RangeRequest::None) {
        let total = match probe_total(provider, ctx, deadline, &limits, &token).await {
            Ok(Some(total)) => total,
            Ok(None) => return fail(502, ServeError::new("unknown-length", "无法确定视频大小")),
            Err(error) => return fail_from(error),
        };
        emit("complete", None, Some(0));
        return response(
            request,
            200,
            &[
                ("Accept-Ranges", "bytes".to_string()),
                ("Content-Type", "video/mp4".to_string()),
                ("Content-Length", total.to_string()),
            ],
            Vec::new(),
        );
    }

    // 先确定本次要取的区间
    let span = match parsed {
        RangeRequest::Bounded { start, end } => {
            match http_range::clamp_to_chunk(start, end, limits.chunk) {
                Ok(span) => span,
                Err(_) => {
                    return fail(
                        416,
                        ServeError::new("range-unsatisfiable", "Range 请求超出视频大小"),
                    )
                }
            }
        }
        RangeRequest::None => ByteSpan::new(0, limits.chunk.saturating_sub(1)),
        RangeRequest::Suffix { len } => {
            let total = match probe_total(provider, ctx, deadline, &limits, &token).await {
                Ok(Some(total)) => total,
                Ok(None) => {
                    return fail(502, ServeError::new("unknown-length", "无法确定视频大小"))
                }
                Err(error) => return fail_from(error),
            };
            match http_range::suffix_span(len, total) {
                Some(span) => span,
                None => {
                    let mut headers: Vec<(&str, String)> = Vec::new();
                    if let Some(value) = http_range::unsatisfiable_range(Some(total)) {
                        headers.push(("Content-Range", value));
                    }
                    emit("error", Some("range-unsatisfiable"), None);
                    return response(request, 416, &headers, Vec::new());
                }
            }
        }
    };

    let mut refreshed = false;
    let mut last_error: Option<ServeError> = None;
    for round in 0..=1 {
        let fresh = round == 1;
        if fresh && refreshed {
            break;
        }
        if !request_is_current(ctx) {
            return fail(
                499,
                ServeError::new("session-cancelled", "播放已被新的请求取代"),
            );
        }
        if Instant::now() >= deadline {
            return fail(504, ServeError::new("budget-exceeded", "B 站视频加载超时"));
        }
        let urls = match provider.resolve(fresh).await {
            Ok(urls) if !urls.is_empty() => urls,
            Ok(_) => return fail(502, ServeError::new("no-candidate", "没有可用的视频地址")),
            Err(error) => return fail(502, error),
        };
        if fresh {
            refreshed = true;
        }
        let mut saw_expired = false;
        for url in urls.iter().take(limits.max_candidates) {
            if !request_is_current(ctx) {
                return fail(
                    499,
                    ServeError::new("session-cancelled", "播放已被新的请求取代"),
                );
            }
            if Instant::now() >= deadline {
                return fail(504, ServeError::new("budget-exceeded", "B 站视频加载超时"));
            }
            if let Some(builder) = trace_builder.as_ref() {
                let mut event = builder.clone();
                event.host = Some(url_host(url));
                event.range = Some(format!("bytes={}-{}", span.start, span.end));
                trace.emit(&event.finish("connect", None, None));
            }
            match fetch_span(url, span, deadline, &limits, &token).await {
                Upstream::Ok {
                    status,
                    content_range,
                    content_type,
                    body,
                    ..
                } => {
                    let mut headers = vec![
                        ("Accept-Ranges", "bytes".to_string()),
                        (
                            "Content-Type",
                            content_type
                                .filter(|t| t.starts_with("video/") || t.starts_with("audio/"))
                                .unwrap_or_else(|| "video/mp4".into()),
                        ),
                    ];
                    if let Some(range) = content_range {
                        headers.push(("Content-Range", range));
                    }
                    let represented = if is_head {
                        // HEAD 描述的是区间长度，不是空 body 的 0
                        headers
                            .iter()
                            .find(|(name, _)| *name == "Content-Range")
                            .and_then(|(_, value)| http_range::parse_content_range(value))
                            .map(|cr| cr.end - cr.start + 1)
                            .unwrap_or(body.len() as u64)
                    } else {
                        body.len() as u64
                    };
                    headers.push(("Content-Length", represented.to_string()));
                    emit("complete", None, Some(represented));
                    let body = if is_head { Vec::new() } else { body };
                    return response(request, status, &headers, body);
                }
                Upstream::Unsatisfiable(content_range) => {
                    let total = content_range
                        .as_deref()
                        .and_then(http_range::parse_content_range)
                        .and_then(|cr| cr.total);
                    let mut headers: Vec<(&str, String)> = Vec::new();
                    if let Some(value) = http_range::unsatisfiable_range(total) {
                        headers.push(("Content-Range", value));
                    }
                    emit("error", Some("range-unsatisfiable"), None);
                    return response(request, 416, &headers, Vec::new());
                }
                Upstream::Expired(status) => {
                    saw_expired = true;
                    last_error = Some(ServeError::new(
                        "cdn-expired",
                        format!("B 站视频服务器拒绝了请求（HTTP {status}）"),
                    ));
                }
                Upstream::RateLimited(retry_after) => {
                    let key = cache_key(ctx.file_id, ctx.qn, ctx.epoch);
                    with_gate(&key, |gate| gate.note_block(Instant::now(), retry_after));
                    forget_source(&key);
                    return fail(
                        429,
                        ServeError::new("cdn-rate-limited", "B 站限流，请稍后重试"),
                    );
                }
                Upstream::Retryable(status) => {
                    last_error = Some(ServeError::new(
                        "cdn-http-error",
                        format!("B 站视频服务器返回 HTTP {status}"),
                    ));
                }
                Upstream::Cancelled => {
                    return fail(
                        499,
                        ServeError::new("session-cancelled", "播放已被新的请求取代"),
                    )
                }
                Upstream::Failed(error) => {
                    if matches!(error.code, "budget-exceeded" | "session-cancelled") {
                        return fail_from(error);
                    }
                    last_error = Some(error);
                }
            }
        }
        if !saw_expired || fresh {
            break;
        }
        // 地址过期：只在预算内刷新一次，且受 gate 限制
        let key = cache_key(ctx.file_id, ctx.qn, ctx.epoch);
        forget_source(&key);
        if !matches!(
            with_gate(&key, |gate| gate.may_refresh(Instant::now())),
            RefreshDecision::Allowed
        ) {
            break;
        }
    }
    // 绝对预算耗尽：不再让平台等待窗口超时，明确给出可重试的失败原因
    if Instant::now() >= deadline {
        return fail(504, ServeError::new("budget-exceeded", "B 站视频加载超时"));
    }
    match last_error {
        Some(error) => fail(502, error),
        None => fail(
            502,
            ServeError::new("cdn-unavailable", "B 站视频暂时无法播放"),
        ),
    }
}

/// 协议入口（由 lib.rs 的异步协议注册调用）。`webview_label` 用于把诊断事件只发给发起窗口。
pub async fn handle_with_webview(
    app: &AppHandle,
    webview_label: Option<&str>,
    request: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let Some(file_id) = parse_stream_path(request.uri().path()) else {
        return text_error(&request, 400, "无效的 B 站播放地址");
    };
    let (qn, epoch, session) = parse_stream_query(request.uri().query());
    let session = session.filter(|s| {
        register_session(s, &file_id, epoch, webview_label)
            && session_bound_to(s, &file_id, epoch, webview_label)
    });
    let ctx = ServeCtx {
        file_id: file_id.as_str(),
        qn,
        epoch,
        session: session.as_deref(),
        request_id: REQUEST_SEQ.fetch_add(1, Ordering::Relaxed),
    };
    let provider = LiveProvider {
        app,
        file_id: file_id.as_str(),
        qn,
        epoch,
    };
    let window_trace = match (webview_label, session.as_deref()) {
        (Some(label), Some(_)) => Some(WindowTrace::new(app.clone(), label)),
        _ => None,
    };
    let trace: &dyn TraceSink = match window_trace.as_ref() {
        Some(trace) => trace,
        None => &NoopTrace,
    };
    serve(&request, &ctx, &provider, trace, Limits::default()).await
}

/// 无窗口信息的入口（保留给旧调用点；不发送事件）
pub async fn handle(app: &AppHandle, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    handle_with_webview(app, None, request).await
}

fn text_error(request: &Request<Vec<u8>>, status: u16, message: &str) -> Response<Vec<u8>> {
    response(
        request,
        status,
        &[("Content-Type", "text/plain; charset=utf-8".into())],
        message.as_bytes().to_vec(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct StaticProvider {
        urls: Vec<Url>,
        calls: AtomicUsize,
    }

    impl StaticProvider {
        fn new(urls: Vec<String>) -> Self {
            Self {
                urls: urls.into_iter().map(|u| Url::parse(&u).unwrap()).collect(),
                calls: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl PlayProvider for StaticProvider {
        fn resolve<'a>(&'a self, _fresh: bool) -> ResolveFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Ok(self.urls.clone())
            })
        }
    }

    struct TestTrace;

    impl TraceSink for TestTrace {
        fn emit(&self, _event: &StreamTrace) {}
    }

    fn ctx<'a>(file_id: &'a str, session: &'a Option<String>) -> ServeCtx<'a> {
        ServeCtx {
            file_id,
            qn: DEFAULT_QN,
            epoch: 0,
            session: session.as_deref(),
            request_id: 1,
        }
    }

    fn get(range: Option<&str>) -> Request<Vec<u8>> {
        let mut builder = Request::builder().method("GET").uri("/file_test");
        if let Some(range) = range {
            builder = builder.header("range", range);
        }
        builder.body(Vec::new()).unwrap()
    }

    fn head() -> Request<Vec<u8>> {
        Request::builder()
            .method("HEAD")
            .uri("/file_test")
            .body(Vec::new())
            .unwrap()
    }

    fn limits() -> Limits {
        Limits {
            budget: Duration::from_secs(5),
            per_attempt: Duration::from_secs(2),
            chunk: 1024,
            max_candidates: 3,
        }
    }

    fn body_of(response: &Response<Vec<u8>>) -> &[u8] {
        response.body()
    }

    /// 会话注册表是全局的：涉及 session 的用例串行执行，避免硬上限用例淘汰别人的会话
    static SESSION_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// 极简 HTTP 服务器：按连接序号返回固定响应，用于“先 403 后 206”这类序列
    async fn raw_http_server<F>(handler: F) -> (String, Arc<AtomicUsize>)
    where
        F: Fn(usize) -> Vec<u8> + Send + Sync + 'static,
    {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let counter = count.clone();
        let handler = Arc::new(handler);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let counter = counter.clone();
                let handler = handler.clone();
                tokio::spawn(async move {
                    use tokio::io::AsyncReadExt;
                    let mut buf = [0u8; 2048];
                    let _ = socket.read(&mut buf).await;
                    let index = counter.fetch_add(1, Ordering::SeqCst);
                    let response = handler(index);
                    let _ = socket.write_all(&response).await;
                });
            }
        });
        (format!("http://{addr}"), count)
    }

    fn http_response(status: &str, headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {status}\r\n");
        for (name, value) in headers {
            out.push_str(&format!("{name}: {value}\r\n"));
        }
        out.push_str(&format!("content-length: {}\r\n\r\n", body.len()));
        let mut bytes = out.into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }

    /// 先发响应头，再延迟发 body：模拟“头已经回了、body 挂住”
    async fn headers_then_slow_body(body_delay: Duration) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 2048];
                    let _ = socket.read(&mut buf).await;
                    let headers = "HTTP/1.1 206 Partial Content\r\ncontent-range: bytes 0-9/10000\r\ncontent-type: video/mp4\r\ncontent-length: 10\r\n\r\n";
                    if socket.write_all(headers.as_bytes()).await.is_err() {
                        return;
                    }
                    let _ = socket.flush().await;
                    tokio::time::sleep(body_delay).await;
                    let _ = socket.write_all(b"0123456789").await;
                });
            }
        });
        format!("http://{addr}")
    }

    // ---------------------------------------------------------------- 契约回归

    #[tokio::test]
    async fn suffix_range_is_served_from_the_tail_not_416() {
        let mut server = mockito::Server::new_async().await;
        let payload: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let _m = server
            .mock("GET", "/v.mp4")
            .match_header("range", "bytes=0-0")
            .with_status(206)
            .with_header("content-range", "bytes 0-0/10000")
            .with_header("content-type", "video/mp4")
            .with_body(&payload[..1])
            .create_async()
            .await;
        let _tail = server
            .mock("GET", "/v.mp4")
            .match_header("range", "bytes=9500-9999")
            .with_status(206)
            .with_header("content-range", "bytes 9500-9999/10000")
            .with_header("content-type", "video/mp4")
            .with_body(&payload[9500..])
            .create_async()
            .await;

        let provider = StaticProvider::new(vec![format!("{}/v.mp4", server.url())]);
        let session = None;
        let response = serve(
            &get(Some("bytes=-500")),
            &ctx("file_test", &session),
            &provider,
            &TestTrace,
            limits(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 206);
        assert_eq!(
            response
                .headers()
                .get("content-range")
                .unwrap()
                .to_str()
                .unwrap(),
            "bytes 9500-9999/10000"
        );
        assert_eq!(body_of(&response), &payload[9500..]);
    }

    #[tokio::test]
    async fn head_without_range_reports_full_length_without_body() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/v.mp4")
            .match_header("range", "bytes=0-0")
            .with_status(206)
            .with_header("content-range", "bytes 0-0/1048576")
            .with_header("content-type", "video/mp4")
            .with_body("x")
            .create_async()
            .await;
        let provider = StaticProvider::new(vec![format!("{}/v.mp4", server.url())]);
        let session = None;
        let response = serve(
            &head(),
            &ctx("file_test", &session),
            &provider,
            &TestTrace,
            limits(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(
            response
                .headers()
                .get("content-length")
                .unwrap()
                .to_str()
                .unwrap(),
            "1048576"
        );
        assert!(body_of(&response).is_empty());
    }

    #[tokio::test]
    async fn wrong_partial_range_is_rejected() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/v.mp4")
            .with_status(206)
            .with_header("content-range", "bytes 0-99/10000")
            .with_header("content-type", "video/mp4")
            .with_body(vec![0u8; 100])
            .create_async()
            .await;
        let provider = StaticProvider::new(vec![format!("{}/v.mp4", server.url())]);
        let session = None;
        let response = serve(
            &get(Some("bytes=100-199")),
            &ctx("file_test", &session),
            &provider,
            &TestTrace,
            limits(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 502);
        assert_eq!(
            response
                .headers()
                .get("x-media-error-code")
                .unwrap()
                .to_str()
                .unwrap(),
            "cdn-range-invalid"
        );
    }

    #[tokio::test]
    async fn ignored_range_is_not_sliced_from_offset_nor_buffered_whole() {
        let mut server = mockito::Server::new_async().await;
        let payload = vec![7u8; 8 * 1024 * 1024];
        let _m = server
            .mock("GET", "/v.mp4")
            .with_status(200)
            .with_header("content-type", "video/mp4")
            .with_body(payload)
            .create_async()
            .await;
        let provider = StaticProvider::new(vec![format!("{}/v.mp4", server.url())]);
        let session = None;
        let response = serve(
            &get(Some("bytes=1000000-1000999")),
            &ctx("file_test", &session),
            &provider,
            &TestTrace,
            limits(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 502);
        assert_eq!(
            response
                .headers()
                .get("x-media-error-code")
                .unwrap()
                .to_str()
                .unwrap(),
            "range-not-honored"
        );
    }

    #[tokio::test]
    async fn expired_url_refreshes_at_most_once() {
        let mut server = mockito::Server::new_async().await;
        let _first = server
            .mock("GET", "/v.mp4")
            .with_status(403)
            .expect(2)
            .create_async()
            .await;
        let provider = StaticProvider::new(vec![format!("{}/v.mp4", server.url())]);
        let session = None;
        let response = serve(
            &get(Some("bytes=0-1023")),
            &ctx("file_expired_once", &session),
            &provider,
            &TestTrace,
            limits(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 502);
        assert_eq!(
            provider.calls(),
            2,
            "应只在地址过期时刷新一次，不应无限重试"
        );
        _first.assert_async().await;
    }

    #[tokio::test]
    async fn rate_limited_is_not_retried_immediately() {
        let mut server = mockito::Server::new_async().await;
        let m = server
            .mock("GET", "/v.mp4")
            .with_status(429)
            .with_header("retry-after", "120")
            .expect(1)
            .create_async()
            .await;
        let provider = StaticProvider::new(vec![format!("{}/v.mp4", server.url())]);
        let session = None;
        let response = serve(
            &get(Some("bytes=0-1023")),
            &ctx("file_rate_limited", &session),
            &provider,
            &TestTrace,
            limits(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 429);
        assert_eq!(
            response
                .headers()
                .get("x-media-error-code")
                .unwrap()
                .to_str()
                .unwrap(),
            "cdn-rate-limited"
        );
        assert_eq!(provider.calls(), 1, "429 之后不应刷新 / 轮换");
        m.assert_async().await;
    }

    #[tokio::test]
    async fn absolute_budget_bounds_slow_upstream() {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    // 永远不回应：模拟挂在连接上的慢 CDN
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    let _ = socket
                        .write_all(b"HTTP/1.1 206 Partial Content\r\ncontent-range: bytes 0-9/10000\r\ncontent-length: 10\r\n\r\n0123456789")
                        .await;
                });
            }
        });
        let provider = StaticProvider::new(vec![format!("http://{addr}/v.mp4")]);
        let session = None;
        let started = Instant::now();
        let response = serve(
            &get(Some("bytes=0-1023")),
            &ctx("file_slow", &session),
            &provider,
            &TestTrace,
            Limits {
                budget: Duration::from_millis(300),
                per_attempt: Duration::from_secs(10),
                chunk: 1024,
                max_candidates: 3,
            },
        )
        .await;
        assert_eq!(response.status().as_u16(), 504);
        assert_eq!(
            response
                .headers()
                .get("x-media-error-code")
                .unwrap()
                .to_str()
                .unwrap(),
            "budget-exceeded"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "绝对预算必须在 ~300ms 内结束，实际 {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn flight_is_shared_per_key_and_isolated_across_keys() {
        let first = flight_for("flight-key-1");
        let second = flight_for("flight-key-1");
        let other = flight_for("flight-key-2");
        assert!(Arc::ptr_eq(&first, &second), "同一 key 必须共享一次解析");
        assert!(!Arc::ptr_eq(&first, &other));
    }

    #[test]
    fn session_ids_must_be_bounded_hex() {
        assert!(!register_session("../../x", "file_session", 0, None));
        assert!(!register_session("", "file_session", 0, None));
        assert!(!register_session(&"a".repeat(80), "file_session", 0, None));
        assert!(register_session("0a1b2c", "file_session", 0, None));
    }

    struct RecordingTrace(Mutex<Vec<StreamTrace>>);

    impl TraceSink for RecordingTrace {
        fn emit(&self, event: &StreamTrace) {
            if let Ok(mut events) = self.0.lock() {
                events.push(event.clone());
            }
        }
    }

    #[tokio::test]
    async fn stale_session_stops_without_touching_the_new_one() {
        let _serial = SESSION_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(register_session("aa11", "file_stale", 0, Some("main")));
        assert!(register_session("bb22", "file_stale", 0, Some("main")));
        let provider = StaticProvider::new(vec!["http://127.0.0.1:9/v.mp4".to_string()]);
        let session = Some("aa11".to_string());
        let response = serve(
            &get(Some("bytes=0-1023")),
            &ctx("file_stale", &session),
            &provider,
            &TestTrace,
            limits(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 499);
        assert_eq!(
            response
                .headers()
                .get("x-media-error-code")
                .unwrap()
                .to_str()
                .unwrap(),
            "session-cancelled"
        );
    }

    #[tokio::test]
    async fn trace_events_are_sanitized_and_session_scoped() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/v.mp4")
            // 地址带签名查询串：mockito 默认按 "path?query" 精确匹配，必须放宽
            .match_query(mockito::Matcher::Any)
            .with_status(206)
            .with_header("content-range", "bytes 0-9/10000")
            .with_header("content-type", "video/mp4")
            .with_body(vec![1u8; 10])
            .create_async()
            .await;
        let url = format!("{}/v.mp4?e=1&signature=SECRET", server.url());
        let provider = StaticProvider::new(vec![url.clone()]);
        let trace = RecordingTrace(Mutex::new(Vec::new()));
        let session = Some("cafe01".to_string());
        let response = serve(
            &get(Some("bytes=0-9")),
            &ctx("file_trace", &session),
            &provider,
            &trace,
            limits(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 206);
        let events = trace.0.lock().unwrap().clone();
        assert!(!events.is_empty());
        for event in &events {
            assert_eq!(event.session_id, "cafe01");
            if let Some(host) = event.host.as_deref() {
                assert!(host.ends_with("127.0.0.1"), "host 只记主机名");
            }
            // 签名 / 路径不能出现在任何诊断字段里
            let serialized = serde_json::to_string(event).unwrap();
            assert!(!serialized.contains("SECRET"));
            assert!(!serialized.contains("/v.mp4"));
        }

        // 没有 session 时不产生事件
        let quiet = RecordingTrace(Mutex::new(Vec::new()));
        let no_session = None;
        let _ = serve(
            &get(Some("bytes=0-9")),
            &ctx("file_trace", &no_session),
            &provider,
            &quiet,
            limits(),
        )
        .await;
        assert!(quiet.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn slow_body_after_headers_is_bounded_by_budget() {
        let url = headers_then_slow_body(Duration::from_secs(2)).await;
        let provider = StaticProvider::new(vec![format!("{url}/v.mp4")]);
        let session = None;
        let started = Instant::now();
        let response = serve(
            &get(Some("bytes=0-9")),
            &ctx("file_slow_body", &session),
            &provider,
            &TestTrace,
            Limits {
                budget: Duration::from_millis(400),
                per_attempt: Duration::from_secs(30),
                chunk: 1024,
                max_candidates: 3,
            },
        )
        .await;
        assert_eq!(response.status().as_u16(), 504);
        assert_eq!(
            response
                .headers()
                .get("x-media-error-code")
                .unwrap()
                .to_str()
                .unwrap(),
            "budget-exceeded"
        );
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "body 读取也必须受绝对预算约束，实际 {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn head_expired_url_refreshes_once() {
        let (url, count) = raw_http_server(|index| {
            if index == 0 {
                http_response("403 Forbidden", &[], b"")
            } else {
                http_response(
                    "206 Partial Content",
                    &[
                        ("content-range", "bytes 0-0/5000"),
                        ("content-type", "video/mp4"),
                    ],
                    b"x",
                )
            }
        })
        .await;
        let provider = StaticProvider::new(vec![format!("{url}/v.mp4")]);
        let session = None;
        let response = serve(
            &head(),
            &ctx("file_head_expired", &session),
            &provider,
            &TestTrace,
            limits(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(
            response
                .headers()
                .get("content-length")
                .unwrap()
                .to_str()
                .unwrap(),
            "5000"
        );
        assert_eq!(provider.calls(), 2, "HEAD 过期也只能刷新一次");
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn head_rate_limited_respects_cooldown_without_retry() {
        let (url, count) = raw_http_server(|_| {
            http_response("429 Too Many Requests", &[("retry-after", "120")], b"")
        })
        .await;
        let provider = StaticProvider::new(vec![format!("{url}/v.mp4")]);
        let session = None;
        let response = serve(
            &head(),
            &ctx("file_head_limited", &session),
            &provider,
            &TestTrace,
            limits(),
        )
        .await;
        assert_eq!(response.status().as_u16(), 429);
        assert_eq!(
            response
                .headers()
                .get("x-media-error-code")
                .unwrap()
                .to_str()
                .unwrap(),
            "cdn-rate-limited"
        );
        assert_eq!(count.load(Ordering::SeqCst), 1, "429 之后不得立即再请求");
    }

    #[tokio::test]
    async fn superseding_session_cancels_in_flight_read() {
        let _serial = SESSION_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        assert!(register_session("feed01", "file_inflight", 0, Some("main")));
        let url = headers_then_slow_body(Duration::from_secs(2)).await;
        let provider = StaticProvider::new(vec![format!("{url}/v.mp4")]);
        let session = Some("feed01".to_string());
        tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(120)).await;
            // 换条目的新会话：旧请求必须立刻收手
            register_session("feed02", "file_inflight", 0, Some("main"));
        });
        let started = Instant::now();
        let response = serve(
            &get(Some("bytes=0-9")),
            &ctx("file_inflight", &session),
            &provider,
            &TestTrace,
            Limits {
                budget: Duration::from_secs(5),
                per_attempt: Duration::from_secs(5),
                chunk: 1024,
                max_candidates: 3,
            },
        )
        .await;
        assert_eq!(response.status().as_u16(), 499);
        assert_eq!(
            response
                .headers()
                .get("x-media-error-code")
                .unwrap()
                .to_str()
                .unwrap(),
            "session-cancelled"
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "取消必须打断进行中的 body 读取，实际 {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn session_registry_has_a_hard_cap() {
        let _serial = SESSION_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for i in 0..(MAX_SESSIONS + 40) {
            let id = format!("{:016x}", i);
            assert!(register_session(&id, "file_cap", 0, Some("main")));
        }
        assert!(
            session_registry_len() <= MAX_SESSIONS,
            "会话注册表必须有硬上限"
        );
    }

    // ---------------------------------------------------------------- 既有单测

    #[test]
    fn stream_path_accepts_vfs_file_ids_only() {
        assert_eq!(
            parse_stream_path("/file_AbC-12_x").as_deref(),
            Some("file_AbC-12_x")
        );
        assert_eq!(
            parse_stream_path("/file_a%5Fb").as_deref(),
            Some("file_a_b")
        );
        assert_eq!(
            parse_stream_path("/att_x/anything").as_deref(),
            Some("att_x")
        );
        assert!(parse_stream_path("/../etc/passwd").is_none());
        assert!(parse_stream_path("/https%3A%2F%2Fevil.com").is_none());
        assert!(parse_stream_path("/res_abc").is_none());
        assert!(parse_stream_path("/").is_none());
    }

    #[test]
    fn stream_qn_comes_from_query_and_falls_back_to_default() {
        assert_eq!(parse_stream_query(None), (DEFAULT_QN, 0, None));
        assert_eq!(parse_stream_query(Some("")), (DEFAULT_QN, 0, None));
        assert_eq!(parse_stream_query(Some("qn=64")), (64, 0, None));
        assert_eq!(parse_stream_query(Some("t=1&qn=16&e=3")), (16, 3, None));
        assert_eq!(parse_stream_query(Some("qn=112")).0, DEFAULT_QN);
        assert_eq!(parse_stream_query(Some("qn=120")).0, DEFAULT_QN);
        assert_eq!(
            parse_stream_query(Some("qn=abc&e=x")),
            (DEFAULT_QN, 0, None)
        );
        assert_eq!(parse_stream_query(Some("xqn=64")).0, DEFAULT_QN);
        assert_eq!(
            parse_stream_query(Some("qn=64&e=1&s=deadBEEF0123"))
                .2
                .as_deref(),
            Some("deadBEEF0123")
        );
        // 非十六进制 / 过长的 session 丢弃
        assert_eq!(parse_stream_query(Some("s=../../etc")).2, None);
        assert_eq!(
            parse_stream_query(Some(&format!("s={}", "a".repeat(80)))).2,
            None
        );
        assert_eq!(normalize_qn(Some(32)), 32);
        assert_eq!(normalize_qn(Some(116)), DEFAULT_QN);
        assert_eq!(normalize_qn(None), DEFAULT_QN);
    }

    #[test]
    fn cache_key_separates_qualities() {
        assert_ne!(cache_key("file_a", 80, 0), cache_key("file_a", 64, 0));
        assert_ne!(cache_key("file_a", 64, 0), cache_key("file_b", 64, 0));
        assert_ne!(cache_key("file_a", 80, 0), cache_key("file_a", 80, 1));
        let info = PlayInfo {
            urls: vec![Url::parse("https://a.bilivideo.com/720.mp4").unwrap()],
            quality: 64,
            options: vec![],
        };
        remember_source(&cache_key("file_cache_test", 64, 0), &info);
        assert_eq!(
            cached_source(&cache_key("file_cache_test", 64, 0)),
            Some(info)
        );
        assert_eq!(cached_source(&cache_key("file_cache_test", 80, 0)), None);
        forget_source(&cache_key("file_cache_test", 64, 0));
        assert_eq!(cached_source(&cache_key("file_cache_test", 64, 0)), None);
    }

    #[test]
    fn quality_options_keep_only_mp4_levels_from_accept_quality() {
        let data: PlayUrlData = serde_json::from_str(
            r#"{"quality":80,"accept_quality":[120,116,112,80,74,64,32,16],
                "accept_description":["超清 4K","高清 1080P60","高清 1080P+","高清 1080P","高清 720P60","高清 720P","清晰 480P","流畅 360P"],
                "durl":[{"url":"https://a.bilivideo.com/1080.mp4"}]}"#,
        )
        .unwrap();
        let info = parse_play_info(&data, 80).unwrap();
        assert_eq!(info.quality, 80);
        assert_eq!(
            info.options.iter().map(|o| o.qn).collect::<Vec<_>>(),
            vec![80, 64, 32, 16]
        );

        let guest: PlayUrlData = serde_json::from_str(
            r#"{"quality":64,"format":"mp4720","accept_quality":[64,16],"accept_description":["高清 720P","流畅 360P"],
                "durl":[{"url":"https://a.bilivideo.com/720.mp4"}]}"#,
        )
        .unwrap();
        let qns: Vec<u32> = mp4_quality_options(&guest).iter().map(|o| o.qn).collect();
        assert_eq!(qns, vec![64, 16]);

        let sparse: PlayUrlData = serde_json::from_str(
            r#"{"quality":32,"accept_quality":[64,16],"accept_description":["高清 720P"],"durl":[]}"#,
        )
        .unwrap();
        assert_eq!(
            mp4_quality_options(&sparse)
                .iter()
                .map(|o| o.qn)
                .collect::<Vec<_>>(),
            vec![64, 32, 16]
        );
    }

    #[test]
    fn picks_single_mp4_and_keeps_ordered_backup_candidates() {
        let data: PlayUrlData = serde_json::from_str(
            r#"{"durl":[{"url":"https://evil.example.com/a.mp4","backup_url":["http://cn-x.bilivideo.com/b.mp4","https://cn-y.bilivideo.com/c.mp4"]}]}"#,
        )
        .unwrap();
        let urls: Vec<String> = pick_play_urls(&data)
            .unwrap()
            .into_iter()
            .map(|u| u.to_string())
            .collect();
        assert_eq!(
            urls,
            vec![
                "https://cn-x.bilivideo.com/b.mp4",
                "https://cn-y.bilivideo.com/c.mp4"
            ]
        );

        let multi: PlayUrlData = serde_json::from_str(
            r#"{"durl":[{"url":"https://a.bilivideo.com/1"},{"url":"https://a.bilivideo.com/2"}]}"#,
        )
        .unwrap();
        assert!(pick_play_urls(&multi).is_err());
        let none: PlayUrlData = serde_json::from_str(r#"{"durl":[]}"#).unwrap();
        assert!(pick_play_urls(&none).is_err());
    }

    #[tokio::test]
    async fn resolves_play_url_through_the_api_with_cookie() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/x/player/playurl")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("bvid".into(), "BV1Ss4y1W7KB".into()),
                mockito::Matcher::UrlEncoded("cid".into(), "995381097".into()),
                mockito::Matcher::UrlEncoded("platform".into(), "html5".into()),
            ]))
            .match_header("cookie", "SESSDATA=s")
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"code":0,"message":"0","data":{"format":"mp4","durl":[{"url":"https://cn-bj.bilivideo.com/v.mp4?e=1","size":10}]}}"#,
            )
            .create_async()
            .await;
        let client = BiliClient::for_test(&server.url()).with_cookie(Some("SESSDATA=s".into()));
        let info = resolve_play_info(&client, &descriptor(), DEFAULT_QN)
            .await
            .unwrap();
        assert_eq!(
            info.urls.first().map(|u| u.to_string()).unwrap(),
            "https://cn-bj.bilivideo.com/v.mp4?e=1"
        );
        assert_eq!(info.quality, DEFAULT_QN);
    }

    fn descriptor() -> BiliLinkDescriptor {
        serde_json::from_value(serde_json::json!({
            "kind": "bilibili", "version": 1, "bvid": "BV1Ss4y1W7KB", "aid": 1, "cid": 995381097u64,
            "page": 1, "pageCount": 1, "title": "t", "durationMs": 1000, "url": "https://www.bilibili.com/video/BV1Ss4y1W7KB"
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn requests_selected_qn_and_reports_downgraded_quality() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("GET", "/x/player/playurl")
            .match_query(mockito::Matcher::AllOf(vec![
                mockito::Matcher::UrlEncoded("qn".into(), "80".into()),
                mockito::Matcher::UrlEncoded("fnval".into(), "1".into()),
            ]))
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"code":0,"message":"0","data":{"quality":64,"format":"mp4720","accept_quality":[64,16],
                    "accept_description":["高清 720P","流畅 360P"],
                    "durl":[{"url":"https://cn-bj.bilivideo.com/720.mp4?e=1"}]}}"#,
            )
            .create_async()
            .await;
        let _low = server
            .mock("GET", "/x/player/playurl")
            .match_query(mockito::Matcher::UrlEncoded("qn".into(), "16".into()))
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"code":0,"message":"0","data":{"quality":16,"format":"mp4","accept_quality":[64,16],
                    "accept_description":["高清 720P","流畅 360P"],
                    "durl":[{"url":"https://cn-bj.bilivideo.com/360.mp4?e=1"}]}}"#,
            )
            .create_async()
            .await;
        let client = BiliClient::for_test(&server.url());
        let high = resolve_play_info(&client, &descriptor(), 80).await.unwrap();
        assert_eq!(high.quality, 64);
        let low = resolve_play_info(&client, &descriptor(), 16).await.unwrap();
        assert_eq!(low.quality, 16);
        assert!(low.urls[0].as_str().ends_with("360.mp4?e=1"));
    }
}
