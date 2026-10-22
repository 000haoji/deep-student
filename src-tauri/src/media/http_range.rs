//! HTTP Range 请求 / 响应契约（纯函数，无网络依赖，可单测）。
//!
//! `bilistream://` 代理必须严格实现浏览器媒体栈依赖的 Range 语义：
//!
//! - `HEAD`：返回**完整资源长度**（`Content-Length`），不能拿 1 字节探测 body 当总长；
//! - `bytes=0-` / `bytes=N-M` / 后缀 `bytes=-N`（MP4 尾部 moov 与 seek 必需）；
//! - 越界 → `416`；总长已知时带 `Content-Range: bytes */TOTAL`，未知时不得伪造该头；
//! - 上游 `206` 的 `Content-Range` 与 body 长度必须与实际请求区间一致；
//! - 上游忽略 Range 返回 `200` 时**不能**整片缓冲，也不能冒充请求的局部字节。
//!
//! 本模块只做纯计算，调用方负责网络与日志。所有字节区间都是闭区间。

use std::fmt;

/// 单次响应的默认上限（约 10 秒 1080P）
pub const DEFAULT_CHUNK_BYTES: u64 = 4 * 1024 * 1024;

/// 闭区间字节范围 `start..=end`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteSpan {
    pub start: u64,
    pub end: u64,
}

#[allow(clippy::len_without_is_empty)] // 闭区间构造方保证 start ≤ end，不存在“空区间”
impl ByteSpan {
    pub fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    /// 区间长度（字节数）。0..=u64::MAX 的真实长度超出 u64，按饱和值返回。
    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start).saturating_add(1)
    }
}

/// 请求方（浏览器 / WebView 媒体栈）的 Range 语义
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeRequest {
    /// 没有 Range 头（或空值）
    None,
    /// `bytes=start-` / `bytes=start-end`
    Bounded { start: u64, end: Option<u64> },
    /// `bytes=-N`：末尾 N 字节
    Suffix { len: u64 },
}

/// Range 头无法按契约处理
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeError {
    /// 语法不认识 / 多段（本代理不支持）
    Unsupported,
    /// 合法语法但区间不可满足（越界、end < start、`bytes=-0`）
    Unsatisfiable,
}

impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RangeError::Unsupported => f.write_str("unsupported-range"),
            RangeError::Unsatisfiable => f.write_str("range-not-satisfiable"),
        }
    }
}

/// 上游 `Content-Range` 头
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentRange {
    pub start: u64,
    pub end: u64,
    /// `*` / 缺失时为 None
    pub total: Option<u64>,
}

/// 上游 206 响应不符合请求契约
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartialError {
    /// 206 但没有 Content-Range
    MissingHeader,
    /// Content-Range 无法解析
    Malformed,
    /// 区间起点不是请求的起点（错段）
    StartMismatch,
    /// 区间终点或 body 长度与 Content-Range 不一致（截断 / 超长）
    LengthMismatch,
}

/// 上游忽略 Range（回 200）时，能否安全截取一段作为局部响应
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IgnoredRange {
    pub span: ByteSpan,
    /// 上游给的总长（未知则 None，只能回 `bytes S-E/*`）
    pub total: Option<u64>,
}

/// 解析请求的 `Range` 头。
pub fn parse_range_header(raw: Option<&str>) -> Result<RangeRequest, RangeError> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(RangeRequest::None);
    };
    let lower = raw.to_ascii_lowercase();
    let Some(spec) = lower.strip_prefix("bytes=") else {
        return Err(RangeError::Unsupported);
    };
    let spec = spec.trim();
    if spec.contains(',') {
        // 多段 range 不实现（浏览器媒体栈只用单段）
        return Err(RangeError::Unsupported);
    }
    let Some((start, end)) = spec.split_once('-') else {
        return Err(RangeError::Unsupported);
    };
    let (start, end) = (start.trim(), end.trim());
    if start.is_empty() {
        if end.is_empty() {
            return Err(RangeError::Unsupported);
        }
        let len: u64 = end.parse().map_err(|_| RangeError::Unsupported)?;
        return if len == 0 {
            Err(RangeError::Unsatisfiable)
        } else {
            Ok(RangeRequest::Suffix { len })
        };
    }
    let start: u64 = start.parse().map_err(|_| RangeError::Unsupported)?;
    if end.is_empty() {
        return Ok(RangeRequest::Bounded { start, end: None });
    }
    let end: u64 = end.parse().map_err(|_| RangeError::Unsupported)?;
    if end < start {
        return Err(RangeError::Unsatisfiable);
    }
    Ok(RangeRequest::Bounded {
        start,
        end: Some(end),
    })
}

/// 把开放式 / 有限区间收窄到单次响应上限 [`DEFAULT_CHUNK_BYTES`] 一类的大小。
pub fn clamp_to_chunk(start: u64, end: Option<u64>, chunk: u64) -> Result<ByteSpan, RangeError> {
    if chunk == 0 {
        return Err(RangeError::Unsatisfiable);
    }
    let wanted_end = end.unwrap_or(u64::MAX);
    if wanted_end < start {
        return Err(RangeError::Unsatisfiable);
    }
    let max_end = start.saturating_add(chunk - 1);
    Ok(ByteSpan::new(start, wanted_end.min(max_end)))
}

/// 后缀请求 → 具体区间；总长为 0 或后缀长度为 0 时不可满足。
pub fn suffix_span(len: u64, total: u64) -> Option<ByteSpan> {
    if len == 0 || total == 0 {
        return None;
    }
    let take = len.min(total);
    Some(ByteSpan::new(total - take, total - 1))
}

/// 生成响应的 `Content-Range`（总长未知时用 `*`，不伪造）。
pub fn content_range(span: ByteSpan, total: Option<u64>) -> String {
    match total {
        Some(total) => format!("bytes {}-{}/{}", span.start, span.end, total),
        None => format!("bytes {}-{}/*", span.start, span.end),
    }
}

/// 生成 `416` 响应的 `Content-Range`：总长未知时**不发**该头（`bytes */*` 不是合法语法）。
pub fn unsatisfiable_range(total: Option<u64>) -> Option<String> {
    total.map(|total| format!("bytes */{}", total))
}

/// 解析上游 `Content-Range`。`bytes */TOTAL`（不可满足标记）返回 None。
pub fn parse_content_range(raw: &str) -> Option<ContentRange> {
    let raw = raw.trim();
    let (unit, rest) = raw.split_once(' ')?;
    if !unit.eq_ignore_ascii_case("bytes") {
        return None;
    }
    let (range, total) = rest.trim().split_once('/')?;
    let total = match total.trim() {
        "*" => None,
        value => Some(value.parse::<u64>().ok()?),
    };
    let range = range.trim();
    if range == "*" {
        return None;
    }
    let (start, end) = range.split_once('-')?;
    let start: u64 = start.trim().parse().ok()?;
    let end: u64 = end.trim().parse().ok()?;
    if end < start {
        return None;
    }
    if let Some(total) = total {
        if total == 0 || end >= total {
            return None;
        }
    }
    Some(ContentRange { start, end, total })
}

/// 校验上游 `206` 响应：起点一致、终点/长度一致、不超出请求区间（除非已到文件末尾）。
///
/// 返回上游实际覆盖的区间（可能因文件末尾比请求的短）。
pub fn validate_partial_response(
    requested: ByteSpan,
    header: Option<&str>,
    body_len: usize,
) -> Result<ByteSpan, PartialError> {
    let header = header.ok_or(PartialError::MissingHeader)?;
    let cr = parse_content_range(header).ok_or(PartialError::Malformed)?;
    if cr.start != requested.start {
        return Err(PartialError::StartMismatch);
    }
    // 超长区间直接拒绝，不先读 body 再判断
    if cr.end > requested.end {
        return Err(PartialError::LengthMismatch);
    }
    if cr.end < requested.end {
        // 短于请求：只有到文件末尾才合法
        match cr.total {
            Some(total) if cr.end + 1 == total => {}
            _ => return Err(PartialError::LengthMismatch),
        }
    }
    let expected = cr.end - cr.start + 1;
    if body_len as u64 != expected {
        return Err(PartialError::LengthMismatch);
    }
    Ok(ByteSpan::new(cr.start, cr.end))
}

/// 上游忽略 Range 返回 `200` 时：只允许从 0 开始的请求截取前段，且**不整片缓冲**。
///
/// `start > 0` 时必须失败换候选（否则就是把整片前缀当成请求的局部字节）。
pub fn plan_ignored_range(
    start: u64,
    requested_len: u64,
    total: Option<u64>,
    chunk: u64,
) -> Result<IgnoredRange, RangeError> {
    if chunk == 0 || requested_len == 0 {
        return Err(RangeError::Unsatisfiable);
    }
    if start > 0 {
        return Err(RangeError::Unsupported);
    }
    let mut take = requested_len.min(chunk);
    if let Some(total) = total {
        if total == 0 {
            return Err(RangeError::Unsatisfiable);
        }
        take = take.min(total);
    }
    Ok(IgnoredRange {
        span: ByteSpan::new(0, take - 1),
        total,
    })
}

/// 探测总长（HEAD / 后缀请求需要）：`206` 取 `Content-Range` 的 total，
/// `200` 取完整 `Content-Length`。
pub fn total_from_probe(
    status: u16,
    content_range: Option<&str>,
    content_length: Option<u64>,
) -> Option<u64> {
    match status {
        206 => content_range
            .and_then(parse_content_range)
            .and_then(|c| c.total),
        200 => content_length,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHUNK: u64 = 1024;

    #[test]
    fn parses_open_and_finite_ranges() {
        assert_eq!(parse_range_header(None), Ok(RangeRequest::None));
        assert_eq!(parse_range_header(Some("   ")), Ok(RangeRequest::None));
        assert_eq!(
            parse_range_header(Some("bytes=0-")),
            Ok(RangeRequest::Bounded {
                start: 0,
                end: None
            })
        );
        assert_eq!(
            parse_range_header(Some("bytes=100-199")),
            Ok(RangeRequest::Bounded {
                start: 100,
                end: Some(199)
            })
        );
        // 大小写不敏感的单位
        assert_eq!(
            parse_range_header(Some("Bytes=7-7")),
            Ok(RangeRequest::Bounded {
                start: 7,
                end: Some(7)
            })
        );
    }

    #[test]
    fn parses_suffix_range_which_mp4_moov_and_seek_need() {
        assert_eq!(
            parse_range_header(Some("bytes=-500")),
            Ok(RangeRequest::Suffix { len: 500 })
        );
        assert_eq!(
            parse_range_header(Some("bytes=-0")),
            Err(RangeError::Unsatisfiable)
        );
    }

    #[test]
    fn rejects_multi_range_and_malformed_units() {
        assert_eq!(
            parse_range_header(Some("bytes=0-1,5-6")),
            Err(RangeError::Unsupported)
        );
        assert_eq!(
            parse_range_header(Some("items=0-1")),
            Err(RangeError::Unsupported)
        );
        assert_eq!(
            parse_range_header(Some("bytes=9-3")),
            Err(RangeError::Unsatisfiable)
        );
        assert_eq!(
            parse_range_header(Some("bytes=abc-1")),
            Err(RangeError::Unsupported)
        );
        assert_eq!(
            parse_range_header(Some("bytes=99999999999999999999999-")),
            Err(RangeError::Unsupported)
        );
        assert_eq!(
            parse_range_header(Some("bytes=-")),
            Err(RangeError::Unsupported)
        );
    }

    #[test]
    fn clamps_to_one_chunk_and_never_overflows() {
        assert_eq!(
            clamp_to_chunk(0, None, CHUNK),
            Ok(ByteSpan::new(0, CHUNK - 1))
        );
        assert_eq!(
            clamp_to_chunk(10, Some(999_999), CHUNK),
            Ok(ByteSpan::new(10, 10 + CHUNK - 1))
        );
        assert_eq!(clamp_to_chunk(5, Some(9), CHUNK), Ok(ByteSpan::new(5, 9)));
        assert_eq!(
            clamp_to_chunk(9, Some(3), CHUNK),
            Err(RangeError::Unsatisfiable)
        );
        assert_eq!(
            clamp_to_chunk(u64::MAX - 2, None, CHUNK),
            Ok(ByteSpan::new(u64::MAX - 2, u64::MAX))
        );
        assert_eq!(clamp_to_chunk(0, None, 0), Err(RangeError::Unsatisfiable));
    }

    #[test]
    fn suffix_span_clamps_to_total_and_rejects_empty_resource() {
        assert_eq!(suffix_span(500, 10_000), Some(ByteSpan::new(9_500, 9_999)));
        // 后缀长度超过总长：整个资源
        assert_eq!(suffix_span(20_000, 10_000), Some(ByteSpan::new(0, 9_999)));
        assert_eq!(suffix_span(0, 10_000), None);
        assert_eq!(suffix_span(10, 0), None);
    }

    #[test]
    fn span_len_saturates_at_extremes() {
        assert_eq!(ByteSpan::new(0, 0).len(), 1);
        assert_eq!(ByteSpan::new(0, u64::MAX).len(), u64::MAX);
        assert_eq!(ByteSpan::new(u64::MAX, u64::MAX).len(), 1);
    }

    #[test]
    fn content_range_does_not_invent_total() {
        assert_eq!(
            content_range(ByteSpan::new(0, 99), Some(1000)),
            "bytes 0-99/1000"
        );
        assert_eq!(content_range(ByteSpan::new(0, 99), None), "bytes 0-99/*");
    }

    #[test]
    fn unsatisfiable_range_reports_known_total_only() {
        assert_eq!(
            unsatisfiable_range(Some(1000)).as_deref(),
            Some("bytes */1000")
        );
        // 总长未知：不发无效的 `bytes */*`
        assert_eq!(unsatisfiable_range(None), None);
    }

    #[test]
    fn parses_content_range_forms() {
        assert_eq!(
            parse_content_range("bytes 0-99/1000"),
            Some(ContentRange {
                start: 0,
                end: 99,
                total: Some(1000)
            })
        );
        assert_eq!(
            parse_content_range("bytes 100-199/*"),
            Some(ContentRange {
                start: 100,
                end: 199,
                total: None
            })
        );
        // 不可满足标记不是区间
        assert_eq!(parse_content_range("bytes */1000"), None);
        assert_eq!(parse_content_range("bytes 9-3/1000"), None);
        // 终点越出总长
        assert_eq!(parse_content_range("bytes 0-1000/1000"), None);
        assert_eq!(parse_content_range("garbage"), None);
    }

    #[test]
    fn validates_partial_body_length_and_span() {
        let requested = ByteSpan::new(100, 199);
        assert_eq!(
            validate_partial_response(requested, Some("bytes 100-199/1000"), 100),
            Ok(ByteSpan::new(100, 199))
        );
        // 错段
        assert_eq!(
            validate_partial_response(requested, Some("bytes 0-99/1000"), 100),
            Err(PartialError::StartMismatch)
        );
        // 截断的 body
        assert_eq!(
            validate_partial_response(requested, Some("bytes 100-199/1000"), 42),
            Err(PartialError::LengthMismatch)
        );
        // 超长的 body
        assert_eq!(
            validate_partial_response(requested, Some("bytes 100-199/1000"), 101),
            Err(PartialError::LengthMismatch)
        );
        // 上游给超长区间：不读 body 直接拒绝
        assert_eq!(
            validate_partial_response(requested, Some("bytes 100-299/1000"), 200),
            Err(PartialError::LengthMismatch)
        );
        // 缺头 / 头坏
        assert_eq!(
            validate_partial_response(requested, None, 100),
            Err(PartialError::MissingHeader)
        );
        assert_eq!(
            validate_partial_response(requested, Some("nonsense"), 100),
            Err(PartialError::Malformed)
        );
        // 到文件末尾的短响应是合法的
        assert_eq!(
            validate_partial_response(ByteSpan::new(950, 1199), Some("bytes 950-999/1000"), 50),
            Ok(ByteSpan::new(950, 999))
        );
        // 短响应但不是文件末尾 → 不一致
        assert_eq!(
            validate_partial_response(ByteSpan::new(0, 199), Some("bytes 0-99/1000"), 100),
            Err(PartialError::LengthMismatch)
        );
    }

    #[test]
    fn ignored_range_only_allows_prefix_from_zero() {
        let plan = plan_ignored_range(0, 1024, Some(10_485_760), CHUNK).unwrap();
        assert_eq!(plan.span, ByteSpan::new(0, 1023));
        assert_eq!(plan.total, Some(10_485_760));
        // 请求超过 chunk：仍然只截 chunk
        let plan = plan_ignored_range(0, 999_999, Some(10_485_760), CHUNK).unwrap();
        assert_eq!(plan.span, ByteSpan::new(0, 1023));
        // 总长未知：允许回 `bytes 0-1023/*`，但不伪造 total
        let plan = plan_ignored_range(0, 1024, None, CHUNK).unwrap();
        assert_eq!(plan.span, ByteSpan::new(0, 1023));
        assert_eq!(plan.total, None);
        // 短文件：截到实际长度
        let plan = plan_ignored_range(0, 1024, Some(10), CHUNK).unwrap();
        assert_eq!(plan.span, ByteSpan::new(0, 9));
        // 非 0 起点：不能把整片前缀冒充成局部字节
        assert_eq!(
            plan_ignored_range(100, 1024, Some(10_485_760), CHUNK),
            Err(RangeError::Unsupported)
        );
        assert_eq!(
            plan_ignored_range(0, 0, Some(10), CHUNK),
            Err(RangeError::Unsatisfiable)
        );
        assert_eq!(
            plan_ignored_range(0, 10, Some(0), CHUNK),
            Err(RangeError::Unsatisfiable)
        );
    }

    #[test]
    fn probe_total_uses_content_range_not_one_byte_body() {
        assert_eq!(
            total_from_probe(206, Some("bytes 0-0/1048576"), Some(1)),
            Some(1_048_576)
        );
        assert_eq!(
            total_from_probe(200, None, Some(1_048_576)),
            Some(1_048_576)
        );
        // 206 但总长未知 → None（不得拿 1 字节当总长）
        assert_eq!(total_from_probe(206, Some("bytes 0-0/*"), Some(1)), None);
        assert_eq!(total_from_probe(206, None, Some(1)), None);
        assert_eq!(total_from_probe(403, None, Some(1)), None);
    }
}
