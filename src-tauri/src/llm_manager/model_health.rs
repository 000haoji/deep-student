//! 模型候选静态能力筛查与健康状态缓存
//!
//! 对应 PR #490 方案 §4.7：
//! - **先静态检查配置/渠道/能力**，不合格的候选直接排除，不发起任何请求；
//! - 只有**用户授权的真实请求结果**才写入健康状态（本模块不提供任何探活/预检网络调用，
//!   不会把文档内容或图片发给候选模型做“健康检查”）；
//! - 对认证失败、无效模型 ID、限流（429）、临时服务故障分别记录缓存/退避/熔断；
//! - 冷却中的候选降序到列表尾部而不是删除，全部冷却时仍按原顺序尝试，避免完全卡死。
//!
//! 日志只记录 config_id / 脱敏 model 名 / 错误分类，不记录 api_key、base_url 查询参数
//! 或用户材料。

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::llm_manager::routing::{classify_llm_error, LlmErrorClass};
use crate::llm_manager::ApiConfig;
use crate::models::AppError;

/// 静态筛查拒绝原因（可安全写入日志/诊断）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateRejectReason {
    Disabled,
    NotMultimodal,
    EmbeddingOrReranker,
    MissingModel,
    MissingBaseUrl,
    MissingApiKey,
    Discontinued,
    Blacklisted,
}

impl CandidateRejectReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NotMultimodal => "not_multimodal",
            Self::EmbeddingOrReranker => "embedding_or_reranker",
            Self::MissingModel => "missing_model",
            Self::MissingBaseUrl => "missing_base_url",
            Self::MissingApiKey => "missing_api_key",
            Self::Discontinued => "discontinued",
            Self::Blacklisted => "blacklisted",
        }
    }
}

/// 与 OCR/VLM 质量黑名单保持一致（GLM-4.1V / 4.0V / 4V- 质量差）。
fn is_blacklisted_vlm(model: &str) -> bool {
    let lower = model.to_lowercase();
    lower.contains("glm-4.1v") || lower.contains("glm-4.0v") || lower.contains("glm-4v-")
}

/// 静态能力筛查：不发起任何请求，仅检查本地配置是否具备被调用的条件。
///
/// `auth_mode` 为 `none`/`anonymous` 的自建渠道允许空 key。
pub fn screen_vlm_candidate(config: &ApiConfig) -> Result<(), CandidateRejectReason> {
    if !config.enabled {
        return Err(CandidateRejectReason::Disabled);
    }
    if config.is_embedding || config.is_reranker {
        return Err(CandidateRejectReason::EmbeddingOrReranker);
    }
    if !config.is_multimodal {
        return Err(CandidateRejectReason::NotMultimodal);
    }
    if config.model.trim().is_empty() {
        return Err(CandidateRejectReason::MissingModel);
    }
    if config.base_url.trim().is_empty() {
        return Err(CandidateRejectReason::MissingBaseUrl);
    }
    let keyless = config
        .auth_mode
        .as_deref()
        .map(|mode| {
            let lower = mode.to_ascii_lowercase();
            lower == crate::llm_manager::AUTH_MODE_NONE
                || lower == "anonymous"
                || lower == crate::llm_manager::AUTH_MODE_OPENAI_CODEX_OAUTH
        })
        .unwrap_or(false);
    if !keyless && config.api_key.trim().is_empty() {
        return Err(CandidateRejectReason::MissingApiKey);
    }
    if is_blacklisted_vlm(&config.model) {
        return Err(CandidateRejectReason::Blacklisted);
    }
    if crate::llm_manager::is_discontinued_ocr_model(config) {
        return Err(CandidateRejectReason::Discontinued);
    }
    Ok(())
}

/// 健康失败类别（比 `LlmErrorClass` 更细，用于选择退避时长）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthFailure {
    /// 401/403 invalid key：长期冷却，等用户改配置。
    AuthInvalid,
    /// 404/model not found 等明确无效模型 ID：长冷却。
    InvalidModel,
    /// 429：尊重 Retry-After 的短冷却。
    RateLimited { retry_after_ms: Option<u64> },
    /// 超时/连接失败/5xx：指数退避，多次后熔断。
    Transient,
    /// 参数/内容/协议错误：短冷却，避免每页重复打。
    NonRetryable,
    /// 用户取消：不记录。
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Healthy,
    Backoff,
    AuthInvalid,
    InvalidModel,
    CircuitOpen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateHealthSnapshot {
    pub state: HealthState,
    pub consecutive_failures: u32,
    pub retry_at_ms_since_epoch: Option<u64>,
    pub last_failure: Option<HealthFailure>,
}

#[derive(Debug, Clone, Copy)]
pub struct HealthPolicy {
    pub auth_cooldown: Duration,
    pub invalid_model_cooldown: Duration,
    pub transient_base: Duration,
    pub transient_max: Duration,
    pub circuit_threshold: u32,
    pub circuit_cooldown: Duration,
    pub non_retryable_cooldown: Duration,
}

impl Default for HealthPolicy {
    fn default() -> Self {
        Self {
            auth_cooldown: Duration::from_secs(30 * 60),
            invalid_model_cooldown: Duration::from_secs(6 * 60 * 60),
            transient_base: Duration::from_secs(5),
            transient_max: Duration::from_secs(5 * 60),
            circuit_threshold: 3,
            circuit_cooldown: Duration::from_secs(60),
            non_retryable_cooldown: Duration::from_secs(10 * 60),
        }
    }
}

#[derive(Debug, Clone)]
struct HealthEntry {
    state: HealthState,
    consecutive_failures: u32,
    cooldown_until: Option<Instant>,
    last_failure: Option<HealthFailure>,
}

impl Default for HealthEntry {
    fn default() -> Self {
        Self {
            state: HealthState::Healthy,
            consecutive_failures: 0,
            cooldown_until: None,
            last_failure: None,
        }
    }
}

/// 从供应商错误中提取 429 的 Retry-After（毫秒）。
fn retry_after_ms(err: &AppError) -> Option<u64> {
    let details = err.details.as_ref()?;
    if let Some(ms) = details.get("retry_after_ms").and_then(Value::as_u64) {
        return Some(ms);
    }
    details
        .get("retry_after_seconds")
        .and_then(Value::as_u64)
        .map(|secs| secs.saturating_mul(1000))
        .or_else(|| {
            details
                .get("llm_failover")
                .and_then(|v| v.get("retry_after_ms"))
                .and_then(Value::as_u64)
        })
}

fn http_status(err: &AppError) -> Option<u64> {
    err.details
        .as_ref()
        .and_then(|d| d.get("llm_failover"))
        .and_then(|t| t.get("http_status"))
        .and_then(Value::as_u64)
}

fn message_indicates_invalid_model(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    const NEEDLES: &[&str] = &[
        "model not found",
        "model_not_found",
        "no such model",
        "invalid model",
        "model does not exist",
        "unknown model",
        "模型不存在",
        "无效的模型",
        "模型 id 无效",
    ];
    NEEDLES.iter().any(|needle| lower.contains(needle))
}

/// 在无结构化状态码的消息里识别独立的状态码数字（避免把 5000 命中 500）。
fn message_has_status(message: &str, code: &str) -> bool {
    message.match_indices(code).any(|(index, _)| {
        let before = message[..index].chars().next_back();
        let after = message[index + code.len()..].chars().next();
        !before.is_some_and(|c| c.is_ascii_digit()) && !after.is_some_and(|c| c.is_ascii_digit())
    })
}

/// 未打 `llm_failover` 标记的路径（如 single-shot OCR/VLM）按消息兜底分类，
/// 否则所有失败都会被当成 NonRetryable，健康缓存失去意义。
fn classify_untagged_message(message: &str) -> HealthFailure {
    let lower = message.to_ascii_lowercase();
    if message_has_status(message, "429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("速率限制")
    {
        return HealthFailure::RateLimited {
            retry_after_ms: None,
        };
    }
    if message_has_status(message, "401")
        || lower.contains("unauthorized")
        || lower.contains("invalid api key")
        || lower.contains("invalid_api_key")
        || lower.contains("authentication failed")
        || lower.contains("鉴权失败")
    {
        return HealthFailure::AuthInvalid;
    }
    if message_has_status(message, "404") || message_indicates_invalid_model(message) {
        return HealthFailure::InvalidModel;
    }
    if message_has_status(message, "408")
        || message_has_status(message, "500")
        || message_has_status(message, "502")
        || message_has_status(message, "503")
        || message_has_status(message, "504")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("超时")
        || lower.contains("connection reset")
        || lower.contains("connection refused")
        || lower.contains("connection closed")
        || lower.contains("error sending request")
        || lower.contains("dns")
    {
        return HealthFailure::Transient;
    }
    HealthFailure::NonRetryable
}

/// 把供应商错误映射为健康类别；纯函数便于回归测试。
pub fn classify_health_failure(err: &AppError) -> HealthFailure {
    match classify_llm_error(err) {
        LlmErrorClass::Cancelled => HealthFailure::Cancelled,
        LlmErrorClass::AuthFailed => HealthFailure::AuthInvalid,
        LlmErrorClass::RateLimited => HealthFailure::RateLimited {
            retry_after_ms: retry_after_ms(err),
        },
        LlmErrorClass::RetryableTransient => HealthFailure::Transient,
        LlmErrorClass::NonRetryable => {
            if http_status(err) == Some(404) || message_indicates_invalid_model(&err.message) {
                HealthFailure::InvalidModel
            } else if http_status(err).is_some() {
                HealthFailure::NonRetryable
            } else {
                // 未打标路径（single-shot）没有 http_status，按消息兜底。
                classify_untagged_message(&err.message)
            }
        }
    }
}

/// 进程内健康缓存。重启即清空（不持久化用户渠道状态）。
pub struct ModelHealthCache {
    inner: Mutex<HashMap<String, HealthEntry>>,
    policy: HealthPolicy,
}

impl Default for ModelHealthCache {
    fn default() -> Self {
        Self::new(HealthPolicy::default())
    }
}

/// 渠道指纹：config.id + 模型/地址/凭据/供应商/鉴权方式的哈希。
///
/// 健康缓存以指纹为键：用户修改 key、模型名或 base_url 后指纹变化，旧冷却立即失效，
/// 不会在“已经修好配置”后继续屏蔽 30 分钟。指纹只用于内存键，不写日志、不持久化。
pub fn config_fingerprint(config: &ApiConfig) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    config.id.hash(&mut hasher);
    config.model.hash(&mut hasher);
    config.base_url.hash(&mut hasher);
    config.api_key.hash(&mut hasher);
    config.vendor_id.hash(&mut hasher);
    config.auth_mode.hash(&mut hasher);
    config.provider_type.hash(&mut hasher);
    format!("{}:{:016x}", config.id, hasher.finish())
}

impl ModelHealthCache {
    pub fn new(policy: HealthPolicy) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            policy,
        }
    }

    fn is_available(entry: &HealthEntry, now: Instant) -> bool {
        match entry.cooldown_until {
            Some(until) => until <= now,
            None => true,
        }
    }

    /// 记录一次真实请求成功：清除退避与熔断。
    pub fn record_success(&self, config: &ApiConfig, now: Instant) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = guard
            .entry(config_fingerprint(config))
            .or_insert_with(HealthEntry::default);
        entry.state = HealthState::Healthy;
        entry.consecutive_failures = 0;
        entry.cooldown_until = None;
        entry.last_failure = None;
        let _ = now;
    }

    /// 记录一次真实请求失败（只有被授权的实际调用结果才能进入这里）。
    pub fn record_failure(
        &self,
        config: &ApiConfig,
        err: &AppError,
        now: Instant,
    ) -> HealthFailure {
        let failure = classify_health_failure(err);
        if failure == HealthFailure::Cancelled {
            return failure;
        }
        let fingerprint = config_fingerprint(config);
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = guard
            .entry(fingerprint)
            .or_insert_with(HealthEntry::default);
        entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
        entry.last_failure = Some(failure);

        let (state, cooldown) = match failure {
            HealthFailure::AuthInvalid => (HealthState::AuthInvalid, self.policy.auth_cooldown),
            HealthFailure::InvalidModel => (
                HealthState::InvalidModel,
                self.policy.invalid_model_cooldown,
            ),
            HealthFailure::RateLimited { retry_after_ms } => {
                let cooldown = retry_after_ms
                    .map(Duration::from_millis)
                    .unwrap_or_else(|| self.policy.transient_base * entry.consecutive_failures)
                    .clamp(Duration::from_secs(1), self.policy.transient_max);
                (HealthState::Backoff, cooldown)
            }
            HealthFailure::Transient => {
                if entry.consecutive_failures >= self.policy.circuit_threshold {
                    (HealthState::CircuitOpen, self.policy.circuit_cooldown)
                } else {
                    let exp = entry.consecutive_failures.saturating_sub(1).min(10);
                    let cooldown =
                        (self.policy.transient_base * (1u32 << exp)).min(self.policy.transient_max);
                    (HealthState::Backoff, cooldown)
                }
            }
            HealthFailure::NonRetryable => {
                (HealthState::Backoff, self.policy.non_retryable_cooldown)
            }
            HealthFailure::Cancelled => unreachable!("cancelled handled above"),
        };

        entry.state = state;
        entry.cooldown_until = Some(now + cooldown);
        tracing::warn!(
            "[ModelHealth] {} -> {:?} (cooldown {}s, consecutive={})",
            config.id,
            state,
            cooldown.as_secs(),
            entry.consecutive_failures
        );
        failure
    }

    /// 排序候选：可用候选保持原优先级在前，冷却中的候选降到尾部（不删除）。
    ///
    /// 若全部候选都在冷却，返回原顺序，保证仍有有限次尝试而不是直接失败。
    pub fn order_candidates(&self, candidates: Vec<ApiConfig>, now: Instant) -> Vec<ApiConfig> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let (mut ready, mut cooling): (Vec<ApiConfig>, Vec<ApiConfig>) = (Vec::new(), Vec::new());
        for config in candidates {
            match guard.get(&config_fingerprint(&config)) {
                Some(entry) if !Self::is_available(entry, now) => cooling.push(config),
                _ => ready.push(config),
            }
        }
        if ready.is_empty() {
            cooling
        } else {
            ready.extend(cooling);
            ready
        }
    }

    /// 静态筛查 + 健康排序 + 去重，供 #490 候选循环直接使用。
    pub fn prepare_candidates(
        &self,
        candidates: Vec<ApiConfig>,
        now: Instant,
    ) -> (Vec<ApiConfig>, Vec<(String, CandidateRejectReason)>) {
        let mut rejected = Vec::new();
        let mut accepted = Vec::new();
        for config in candidates {
            match screen_vlm_candidate(&config) {
                Ok(()) => accepted.push(config),
                Err(reason) => {
                    tracing::debug!(
                        "[ModelHealth] skip candidate {} ({}) : {}",
                        config.id,
                        config.model,
                        reason.as_str()
                    );
                    rejected.push((config.id, reason));
                }
            }
        }
        (self.order_candidates(accepted, now), rejected)
    }

    pub fn snapshot(&self, config: &ApiConfig, now: Instant) -> Option<CandidateHealthSnapshot> {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = guard.get(&config_fingerprint(config))?;
        let retry_at_ms_since_epoch = entry.cooldown_until.map(|until| {
            let remaining = until.saturating_duration_since(now);
            earlier_epoch_ms() + remaining.as_millis() as u64
        });
        Some(CandidateHealthSnapshot {
            state: entry.state,
            consecutive_failures: entry.consecutive_failures,
            retry_at_ms_since_epoch,
            last_failure: entry.last_failure,
        })
    }

    pub fn reset(&self, config: &ApiConfig) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.remove(&config_fingerprint(config));
    }

    #[cfg(test)]
    pub fn reset_all(&self) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.clear();
    }
}

fn earlier_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 全局实例：VLM/LLM 候选循环共享，重启清空。
pub static GLOBAL_MODEL_HEALTH: LazyLock<ModelHealthCache> =
    LazyLock::new(ModelHealthCache::default);

pub fn model_health() -> &'static ModelHealthCache {
    &GLOBAL_MODEL_HEALTH
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config(id: &str, model: &str) -> ApiConfig {
        let mut config = ApiConfig::default();
        config.id = id.to_string();
        config.name = id.to_string();
        config.model = model.to_string();
        config.base_url = "https://example.invalid/v1".to_string();
        config.api_key = "test-key".to_string();
        config.is_multimodal = true;
        config.enabled = true;
        config
    }

    fn tagged_error(status: u16, message: &str) -> AppError {
        let mut err = AppError::llm(message);
        err.details = Some(json!({
            "llm_failover": { "phase": "establish", "http_status": status }
        }));
        err
    }

    #[test]
    fn static_screen_rejects_unusable_without_any_request() {
        let mut disabled = config("c1", "glm-4.6v");
        disabled.enabled = false;
        assert_eq!(
            screen_vlm_candidate(&disabled),
            Err(CandidateRejectReason::Disabled)
        );

        let mut text_only = config("c2", "glm-4.6");
        text_only.is_multimodal = false;
        assert_eq!(
            screen_vlm_candidate(&text_only),
            Err(CandidateRejectReason::NotMultimodal)
        );

        let mut no_key = config("c3", "glm-4.6v");
        no_key.api_key.clear();
        assert_eq!(
            screen_vlm_candidate(&no_key),
            Err(CandidateRejectReason::MissingApiKey)
        );

        let mut keyless = config("c4", "glm-4.6v");
        keyless.api_key.clear();
        keyless.auth_mode = Some("none".to_string());
        assert!(screen_vlm_candidate(&keyless).is_ok());

        assert_eq!(
            screen_vlm_candidate(&config("c5", "zai-org/GLM-4.1V")),
            Err(CandidateRejectReason::Blacklisted)
        );
        assert!(screen_vlm_candidate(&config("c6", "glm-4.6v")).is_ok());
    }

    #[test]
    fn failure_classes_are_distinguished() {
        assert_eq!(
            classify_health_failure(&tagged_error(401, "unauthorized")),
            HealthFailure::AuthInvalid
        );
        assert_eq!(
            classify_health_failure(&tagged_error(404, "model not found")),
            HealthFailure::InvalidModel
        );
        let mut limited = tagged_error(429, "rate limited");
        limited.details.as_mut().unwrap()["retry_after_ms"] = json!(2500);
        assert_eq!(
            classify_health_failure(&limited),
            HealthFailure::RateLimited {
                retry_after_ms: Some(2500)
            }
        );
        assert_eq!(
            classify_health_failure(&tagged_error(503, "upstream down")),
            HealthFailure::Transient
        );
        assert_eq!(
            classify_health_failure(&tagged_error(400, "invalid parameter")),
            HealthFailure::NonRetryable
        );
        assert_eq!(
            classify_health_failure(&AppError::llm("用户取消")),
            HealthFailure::Cancelled
        );
    }

    #[test]
    fn untagged_single_shot_errors_are_classified_by_message() {
        assert_eq!(
            classify_health_failure(&AppError::llm("API 请求失败: 速率限制(429)")),
            HealthFailure::RateLimited {
                retry_after_ms: None
            }
        );
        assert_eq!(
            classify_health_failure(&AppError::llm("HTTP 401 Unauthorized")),
            HealthFailure::AuthInvalid
        );
        assert_eq!(
            classify_health_failure(&AppError::llm("request timed out after 30s")),
            HealthFailure::Transient
        );
        assert_eq!(
            classify_health_failure(&AppError::llm("model not found: glm-x")),
            HealthFailure::InvalidModel
        );
        assert_eq!(
            classify_health_failure(&AppError::llm("invalid parameter: temperature")),
            HealthFailure::NonRetryable
        );
    }

    #[test]
    fn transient_failures_open_circuit_and_success_resets() {
        let cache = ModelHealthCache::new(HealthPolicy {
            transient_base: Duration::from_secs(5),
            circuit_threshold: 3,
            circuit_cooldown: Duration::from_secs(60),
            ..HealthPolicy::default()
        });
        let now = Instant::now();
        let err = tagged_error(500, "boom");
        let target = config("c1", "glm-4.6v");
        cache.record_failure(&target, &err, now);
        cache.record_failure(&target, &err, now);
        cache.record_failure(&target, &err, now);
        let snapshot = cache.snapshot(&target, now).unwrap();
        assert_eq!(snapshot.state, HealthState::CircuitOpen);
        assert_eq!(snapshot.consecutive_failures, 3);

        // 冷却期内不可用；冷却后可再次尝试。
        let ordered = cache.order_candidates(vec![config("c1", "glm-4.6v")], now);
        assert_eq!(ordered.len(), 1, "全部冷却时仍保留有限尝试");
        cache.record_success(&target, now + Duration::from_secs(61));
        assert_eq!(
            cache.snapshot(&target, now).unwrap().state,
            HealthState::Healthy
        );
    }

    #[test]
    fn order_puts_cooling_candidates_last() {
        let cache = ModelHealthCache::new(HealthPolicy::default());
        let now = Instant::now();
        let c1 = config("c1", "glm-4.6v");
        cache.record_failure(&c1, &tagged_error(401, "bad key"), now);
        let ordered = cache.order_candidates(
            vec![c1, config("c2", "qwen3-vl"), config("c3", "glm-4.5v")],
            now,
        );
        assert_eq!(
            ordered.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            vec!["c2", "c3", "c1"]
        );
    }

    #[test]
    fn rate_limit_respects_retry_after_and_cancelled_does_not_poison() {
        let cache = ModelHealthCache::new(HealthPolicy::default());
        let now = Instant::now();
        let c1 = config("c1", "glm-4.6v");
        let c2 = config("c2", "glm-4.6v");
        let mut limited = tagged_error(429, "slow down");
        limited.details.as_mut().unwrap()["retry_after_ms"] = json!(5_000);
        cache.record_failure(&c1, &limited, now);
        let snapshot = cache.snapshot(&c1, now).unwrap();
        assert_eq!(snapshot.state, HealthState::Backoff);
        assert!(snapshot.retry_at_ms_since_epoch.is_some());

        // 取消不改变健康状态。
        cache.record_failure(&c2, &AppError::llm("用户取消"), now);
        assert!(cache.snapshot(&c2, now).is_none());
    }

    #[test]
    fn config_change_invalidates_old_cooldown() {
        let cache = ModelHealthCache::new(HealthPolicy::default());
        let now = Instant::now();
        let broken = config("c1", "glm-4.6v");
        cache.record_failure(&broken, &tagged_error(401, "invalid api key"), now);
        assert!(cache.snapshot(&broken, now).is_some());

        // 用户修正 key 后同一 config.id 的指纹变化，旧冷却不再命中。
        let mut fixed = broken.clone();
        fixed.api_key = "new-key".to_string();
        assert!(cache.snapshot(&fixed, now).is_none());
        let ordered = cache.order_candidates(vec![fixed.clone()], now);
        assert_eq!(ordered.len(), 1);
        assert_eq!(ordered[0].id, "c1");
    }

    #[test]
    fn prepare_candidates_filters_static_invalid_before_ordering() {
        let cache = ModelHealthCache::new(HealthPolicy::default());
        let now = Instant::now();
        let mut disabled = config("bad", "glm-4.6v");
        disabled.enabled = false;
        let (ready, rejected) =
            cache.prepare_candidates(vec![config("good", "glm-4.6v"), disabled], now);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "good");
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].1, CandidateRejectReason::Disabled);
    }
}
