// src-tauri/src/gemini_adapter.rs

use crate::models::{AppError, ChatMessage, StandardModel2Output, StreamChunk};
use crate::llm_manager::ApiConfig;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tauri::{Emitter, Window};
use std::time::Duration;

type Result<T> = std::result::Result<T, AppError>;

/// 处理流式聊天请求
pub async fn stream_chat(
    client: &Client,
    config: &ApiConfig,
    messages: &[ChatMessage],
    window: Window,
    stream_event: &str,
) -> Result<StandardModel2Output> {
    let url = build_gemini_url(config, true)?;
    let body = build_gemini_request_body(messages, config)?;

    let response = client
        .post(&url)
        .json(&body)
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| AppError::network(format!("Gemini API request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(AppError::llm(format!(
            "Gemini API error: {} - {}",
            status, error_text
        )));
    }

    process_gemini_stream(response, config, window, stream_event).await
}

/// 处理非流式聊天请求
pub async fn non_stream_chat(
    client: &Client,
    config: &ApiConfig,
    messages: &[ChatMessage],
) -> Result<StandardModel2Output> {
    let url = build_gemini_url(config, false)?;
    let body = build_gemini_request_body(messages, config)?;

    let response = client
        .post(&url)
        .json(&body)
        .timeout(Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| AppError::network(format!("Gemini API request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(AppError::llm(format!(
            "Gemini API error: {} - {}",
            status, error_text
        )));
    }

    let response_json: Value = response.json().await
        .map_err(|e| AppError::network(format!("Failed to parse Gemini response: {}", e)))?;

    // 提取响应内容
    let content = response_json
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    Ok(StandardModel2Output {
        assistant_message: content,
        raw_response: Some(serde_json::to_string(&response_json).unwrap_or_default()),
        chain_of_thought_details: None,
    })
}

/// 构建 Gemini API URL
fn build_gemini_url(config: &ApiConfig, stream: bool) -> Result<String> {
    let endpoint = if stream { "streamGenerateContent" } else { "generateContent" };
    Ok(format!(
        "{}/v1beta/models/{}:{}?key={}",
        config.base_url.trim_end_matches('/'),
        config.model,
        endpoint,
        config.api_key
    ))
}

/// 将通用 ChatMessage 转换为 Gemini 的 `contents` 格式
fn to_gemini_contents(messages: &[ChatMessage], is_multimodal: bool) -> Vec<Value> {
    let mut gemini_contents: Vec<Value> = Vec::new();
    let mut last_role: Option<String> = None;
    
    for msg in messages {
        let current_role = if msg.role == "assistant" { "model" } else { "user" };
        let mut parts = Vec::new();

        // 处理多模态图片数据
        if is_multimodal {
            if let Some(base64_vec) = &msg.image_base64 {
                for img_base64 in base64_vec {
                    parts.push(json!({
                        "inline_data": { 
                            "mime_type": "image/jpeg", 
                            "data": img_base64 
                        }
                    }));
                }
            }
        }
        
        // 处理文本内容
        if !msg.content.is_empty() {
            parts.push(json!({ "text": msg.content }));
        }

        // Gemini 要求 user/model 角色交替出现，此处合并相同角色的连续消息
        if last_role.as_deref() == Some(current_role) && !gemini_contents.is_empty() {
            if let Some(last_content) = gemini_contents.last_mut() {
                if let Some(last_parts) = last_content["parts"].as_array_mut() {
                    last_parts.extend(parts);
                }
            }
        } else {
            gemini_contents.push(json!({ 
                "role": current_role, 
                "parts": parts 
            }));
            last_role = Some(current_role.to_string());
        }
    }
    gemini_contents
}

/// 构建 Gemini 请求体
fn build_gemini_request_body(messages: &[ChatMessage], config: &ApiConfig) -> Result<Value> {
    Ok(json!({
        "contents": to_gemini_contents(messages, config.is_multimodal),
        "generationConfig": {
            "temperature": config.temperature,
            "maxOutputTokens": config.max_output_tokens,
        }
    }))
}

/// 处理 Gemini 的流式响应，并转换为 OpenAI SSE 格式发送到前端
async fn process_gemini_stream(
    response: reqwest::Response,
    config: &ApiConfig,
    window: Window,
    stream_event: &str,
) -> Result<StandardModel2Output> {
    let mut byte_stream = response.bytes_stream();
    let mut full_content = String::new();
    let mut chunk_id_counter = 0;

    while let Some(chunk_result) = byte_stream.next().await {
        let chunk = chunk_result.map_err(|e| AppError::network(format!("Stream read error: {}", e)))?;
        let chunk_str = String::from_utf8_lossy(&chunk);

        for line in chunk_str.lines() {
            if line.starts_with("data: ") {
                let data = &line[6..];
                if let Ok(json_data) = serde_json::from_str::<Value>(data) {
                    if let Some(text_delta) = json_data.pointer("/candidates/0/content/parts/0/text").and_then(Value::as_str) {
                        if !text_delta.is_empty() {
                            full_content.push_str(text_delta);

                            // 使用统一的 StreamChunk 结构体发送事件
                            let stream_chunk = StreamChunk {
                                content: text_delta.to_string(),
                                is_complete: false,
                                chunk_id: format!("gemini_chunk_{}", chunk_id_counter),
                            };

                            window.emit(stream_event, &stream_chunk)
                                .map_err(|e| AppError::unknown(format!("Failed to emit stream chunk: {}", e)))?;
                            chunk_id_counter += 1;
                        }
                    }
                }
            }
        }
    }

    // 发送完成信号
    let final_chunk = StreamChunk {
        content: full_content.clone(),
        is_complete: true,
        chunk_id: format!("gemini_final_{}", chunk_id_counter),
    };

    window.emit(stream_event, &final_chunk)
        .map_err(|e| AppError::unknown(format!("Failed to emit finish chunk: {}", e)))?;

    Ok(StandardModel2Output {
        assistant_message: full_content,
        raw_response: Some("stream_response".to_string()),
        chain_of_thought_details: None,
    })
}
