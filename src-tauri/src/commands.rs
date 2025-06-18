use crate::models::{
    AnalysisRequest, AnalysisResponse, ContinueChatRequest, ContinueChatResponse,
    InitialAnalysisData, SaveMistakeRequest, SaveMistakeResponse,
    MistakeItem, ChatMessage, ModelAssignments, AppError, ReviewAnalysisItem,
    ReviewSessionResponse, ReviewSession, ReviewChatMessage, StartStreamingAnswerRequest,
    SubjectConfig, CreateSubjectConfigRequest, UpdateSubjectConfigRequest,
    StartConsolidatedReviewAnalysisRequest, StartConsolidatedReviewAnalysisResponse,
    TriggerConsolidatedReviewStreamRequest, ContinueConsolidatedReviewStreamRequest,
    ConsolidatedReviewSession, AnkiDocumentGenerationRequest, AnkiDocumentGenerationResponse,
    AnkiGenerationOptions, SubLibrary, CreateSubLibraryRequest, UpdateSubLibraryRequest,
    RagAddDocumentsRequest, RagAddDocumentsFromContentRequest, GetDocumentsRequest,
    RagQueryOptionsWithLibraries, RagQueryOptions, RagQueryResponse,
    GenerateMistakeSummaryRequest, GenerateMistakeSummaryResponse,
    CustomAnkiTemplate, CreateTemplateRequest, UpdateTemplateRequest, TemplateImportRequest, TemplateExportResponse,
    ImageOcrRequest, ImageOcrResponse, CreateImageOcclusionRequest, ImageOcclusionResponse, ImageOcclusionCard,
};
use crate::llm_manager::ApiConfig;
use crate::analysis_service::AnalysisService;
use crate::database::Database;
use crate::file_manager::FileManager;
use std::sync::Arc;
use tauri::{State, Window};
use std::collections::HashMap;
use uuid::Uuid;

type Result<T> = std::result::Result<T, AppError>;
use chrono::Utc;
use serde_json;

// 应用状态
pub struct AppState {
    pub analysis_service: Arc<AnalysisService>,
    pub database: Arc<Database>,
    pub file_manager: Arc<FileManager>,
    pub temp_sessions: Arc<tokio::sync::Mutex<HashMap<String, TempSession>>>,
    pub llm_manager: Arc<crate::llm_manager::LLMManager>,
    pub review_sessions: Arc<tokio::sync::Mutex<HashMap<String, ConsolidatedReviewSession>>>,
    pub rag_manager: Arc<crate::rag_manager::RagManager>,
    pub image_occlusion_service: Arc<crate::image_occlusion_service::ImageOcclusionService>,
}

#[derive(Debug, Clone)]
pub struct TempSession {
    pub temp_id: String,
    pub subject: String,
    pub question_images: Vec<String>,
    pub analysis_images: Vec<String>,
    pub user_question: String,
    pub ocr_text: String,
    pub tags: Vec<String>,
    pub mistake_type: String,
    pub chat_history: Vec<ChatMessage>,
    pub created_at: chrono::DateTime<Utc>,
}

/// 获取模板配置
fn get_template_config(template_id: &str) -> std::result::Result<(String, Vec<String>, String, String, String), String> {
    // 内置模板配置（对应前端ankiTemplates.ts）
    match template_id {
        "minimal-card" => Ok((
            "极简卡片".to_string(),
            vec!["Front".to_string(), "Back".to_string(), "Notes".to_string()],
            r#"<div class="card minimal-card">
  <div class="question">{{Front}}</div>
  <div class="hint">点击显示答案</div>
</div>"#.to_string(),
            r#"<div class="card minimal-card">
  <div class="question">{{Front}}</div>
  <div class="hint">点击显示答案</div>
  
  <div class="answer">{{Back}}</div>
  
  {{#Notes}}
  <div class="notes">
    <div class="notes-label">注释：</div>
    <div>{{Notes}}</div>
  </div>
  {{/Notes}}
</div>"#.to_string(),
            r#".minimal-card {
  font-family: 'Segoe UI', system-ui, sans-serif;
  width: 100%;
  max-width: 100%;
  margin: 0;
  padding: 20px;
  border-radius: 16px;
  background: white;
  box-shadow: 0 5px 25px rgba(0,0,0,0.08);
  text-align: center;
  box-sizing: border-box;
  overflow: hidden;
}

.question {
  font-size: 20px;
  font-weight: 600;
  color: #2c3e50;
  line-height: 1.4;
  margin-bottom: 15px;
  word-wrap: break-word;
}

.answer {
  font-size: 16px;
  color: #27ae60;
  padding: 15px;
  background: #f9fbfa;
  border-radius: 12px;
  margin: 20px 0;
  border-left: 4px solid #2ecc71;
  display: block;
  word-wrap: break-word;
}

.hint {
  font-size: 12px;
  color: #95a5a6;
  font-style: italic;
  margin-bottom: 10px;
}

.notes {
  text-align: left;
  margin-top: 15px;
  padding: 12px;
  background: #f8f9fa;
  border-radius: 10px;
  font-size: 14px;
  color: #7f8c8d;
  word-wrap: break-word;
}

.notes-label {
  font-weight: 600;
  color: #3498db;
  margin-bottom: 5px;
}

.card:hover {
  box-shadow: 0 8px 30px rgba(0,0,0,0.12);
}"#.to_string(),
        )),
        "academic-card" => Ok((
            "学术卡片".to_string(),
            vec!["Front".to_string(), "Back".to_string(), "Example".to_string(), "Source".to_string(), "Tags".to_string()],
            r#"<div class="card academic-card">
  <div class="header">
    <div class="deck-name">{{Deck}}</div>
    <div class="card-type">知识卡片</div>
  </div>
  
  <div class="question">{{Front}}</div>
</div>"#.to_string(),
            r#"<div class="card academic-card">
  <div class="header">
    <div class="deck-name">{{Deck}}</div>
    <div class="card-type">知识卡片</div>
  </div>
  
  <div class="question">{{Front}}</div>
  
  <div class="divider"></div>
  
  <div class="answer">
    <div class="definition">{{Back}}</div>
    {{#Example}}
    <div class="example">
      <div class="example-label">示例：</div>
      <div>{{Example}}</div>
    </div>
    {{/Example}}
  </div>
  
  <div class="footer">
    <div class="source">{{Source}}</div>
    <div class="tags">{{#Tags}}<span class="tag">{{.}}</span>{{/Tags}}</div>
  </div>
</div>"#.to_string(),
            r#".academic-card {
  font-family: 'Georgia', serif;
  width: 100%;
  max-width: 100%;
  margin: 0;
  padding: 20px;
  background: #fcfaf7;
  border: 1px solid #e6e2dd;
  box-shadow: 0 3px 10px rgba(0,0,0,0.05);
  box-sizing: border-box;
  overflow: hidden;
}

.header {
  display: flex;
  justify-content: space-between;
  margin-bottom: 15px;
  font-size: 12px;
  color: #95a5a6;
}

.question {
  font-size: 20px;
  font-weight: bold;
  color: #2c3e50;
  text-align: center;
  margin: 15px 0 20px;
  word-wrap: break-word;
}

.divider {
  height: 1px;
  background: linear-gradient(90deg, transparent, #bdc3c7, transparent);
  margin: 10px 0 15px;
}

.definition {
  font-size: 16px;
  line-height: 1.6;
  color: #34495e;
  text-align: justify;
  word-wrap: break-word;
}

.example {
  margin-top: 15px;
  padding: 12px;
  background: #f8f9fa;
  border-left: 3px solid #3498db;
  word-wrap: break-word;
}

.example-label {
  font-weight: bold;
  color: #2980b9;
  margin-bottom: 5px;
}

.footer {
  display: flex;
  justify-content: space-between;
  margin-top: 20px;
  font-size: 11px;
  color: #7f8c8d;
  flex-wrap: wrap;
}

.tag {
  display: inline-block;
  background: #ecf0f1;
  padding: 2px 8px;
  border-radius: 12px;
  margin-left: 3px;
  margin-bottom: 2px;
  font-size: 10px;
}"#.to_string(),
        )),
        "code-card" => Ok((
            "编程卡片".to_string(),
            vec!["Front".to_string(), "Back".to_string(), "Code".to_string()],
            r#"<div class="card code-card">
  <div class="question">{{Front}}</div>
  
  <div class="hint">// 点击查看解决方案</div>
</div>"#.to_string(),
            r#"<div class="card code-card">
  <div class="question">{{Front}}</div>
  
  <div class="hint">// 点击查看解决方案</div>
  
  <div class="answer">
    {{#Code}}
    <pre><code>{{Code}}</code></pre>
    {{/Code}}
    <div class="explanation">{{Back}}</div>
  </div>
</div>"#.to_string(),
            r#".code-card {
  font-family: 'Fira Code', 'Consolas', monospace;
  width: 100%;
  max-width: 100%;
  margin: 0;
  padding: 20px;
  background: #2d3748;
  color: #cbd5e0;
  border-radius: 8px;
  box-shadow: 0 10px 25px rgba(0,0,0,0.3);
  box-sizing: border-box;
  overflow: hidden;
}

.question {
  font-size: 16px;
  line-height: 1.5;
  color: #81e6d9;
  margin-bottom: 15px;
  word-wrap: break-word;
}

.hint {
  text-align: center;
  color: #718096;
  font-style: italic;
  margin-bottom: 15px;
  font-size: 12px;
}

pre {
  background: #1a202c;
  padding: 15px;
  border-radius: 6px;
  overflow-x: auto;
  border-left: 3px solid #63b3ed;
  font-size: 12px;
  line-height: 1.4;
  word-wrap: break-word;
  white-space: pre-wrap;
}

code {
  color: #feb2b2;
  word-wrap: break-word;
}

.explanation {
  margin-top: 15px;
  padding: 12px;
  background: #4a5568;
  border-radius: 6px;
  font-size: 14px;
  line-height: 1.6;
  word-wrap: break-word;
}"#.to_string(),
        )),
        "cloze-card" => Ok((
            "填空题卡片".to_string(),
            vec!["Text".to_string(), "Hint".to_string(), "Source".to_string()],
            r#"<div class="card cloze-card">
  <div class="cloze-text">{{cloze:Text}}</div>
  
  {{#Hint}}
  <div class="hint-section">
    <div class="hint-label">💡 提示：</div>
    <div class="hint-content">{{Hint}}</div>
  </div>
  {{/Hint}}
</div>"#.to_string(),
            r#"<div class="card cloze-card">
  <div class="cloze-text">{{cloze:Text}}</div>
  
  {{#Hint}}
  <div class="hint-section">
    <div class="hint-label">💡 提示：</div>
    <div class="hint-content">{{Hint}}</div>
  </div>
  {{/Hint}}
  
  <div class="complete-text">
    <div class="complete-label">完整内容：</div>
    <div class="complete-content">{{Text}}</div>
  </div>
  
  {{#Source}}
  <div class="source-section">
    <span class="source-label">📚 来源：</span>
    <span class="source-content">{{Source}}</span>
  </div>
  {{/Source}}
</div>"#.to_string(),
            r#".cloze-card {
  font-family: 'PingFang SC', 'Microsoft YaHei', sans-serif;
  width: 100%;
  max-width: 100%;
  margin: 0;
  padding: 24px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border-radius: 12px;
  box-shadow: 0 8px 32px rgba(102, 126, 234, 0.3);
  box-sizing: border-box;
  overflow: hidden;
  position: relative;
}

.cloze-card::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(255, 255, 255, 0.1);
  backdrop-filter: blur(10px);
  border-radius: 12px;
  z-index: -1;
}

.cloze-text {
  font-size: 18px;
  line-height: 1.6;
  margin-bottom: 20px;
  text-align: justify;
  word-wrap: break-word;
}

.cloze {
  background: #FFD700;
  color: #2c3e50;
  padding: 2px 8px;
  border-radius: 6px;
  font-weight: 600;
  box-shadow: 0 2px 4px rgba(0,0,0,0.2);
}

.hint-section {
  background: rgba(255, 255, 255, 0.2);
  padding: 12px;
  border-radius: 8px;
  margin: 15px 0;
  border-left: 4px solid #FFD700;
}

.hint-label {
  font-weight: 600;
  font-size: 14px;
  margin-bottom: 5px;
  color: #FFD700;
}

.hint-content {
  font-size: 14px;
  line-height: 1.4;
  opacity: 0.9;
}

.complete-text {
  background: rgba(255, 255, 255, 0.15);
  padding: 15px;
  border-radius: 8px;
  margin-top: 20px;
  border: 1px solid rgba(255, 255, 255, 0.3);
}

.complete-label {
  font-weight: 600;
  font-size: 14px;
  margin-bottom: 8px;
  color: #E8F4FD;
}

.complete-content {
  font-size: 16px;
  line-height: 1.5;
  color: #F8F9FA;
}

.source-section {
  margin-top: 15px;
  padding-top: 10px;
  border-top: 1px solid rgba(255, 255, 255, 0.3);
  font-size: 12px;
  opacity: 0.8;
}

.source-label {
  font-weight: 600;
}"#.to_string(),
        )),
        "choice-card" => Ok((
            "选择题卡片".to_string(),
            vec!["Front".to_string(), "optiona".to_string(), "optionb".to_string(), "optionc".to_string(), "optiond".to_string(), "correct".to_string(), "explanation".to_string()],
            r#"<div class="card choice-card">
  <div class="question-section">
    <div class="question-label">📝 题目</div>
    <div class="question-text">{{Front}}</div>
  </div>
  
  <div class="options-section">
    <div class="option clickable" onclick="selectOption('A', '{{correct}}')">
      <span class="option-label">A</span>
      <span class="option-text">{{optiona}}</span>
      <span class="feedback" style="display: none;"></span>
    </div>
    <div class="option clickable" onclick="selectOption('B', '{{correct}}')">
      <span class="option-label">B</span>
      <span class="option-text">{{optionb}}</span>
      <span class="feedback" style="display: none;"></span>
    </div>
    <div class="option clickable" onclick="selectOption('C', '{{correct}}')">
      <span class="option-label">C</span>
      <span class="option-text">{{optionc}}</span>
      <span class="feedback" style="display: none;"></span>
    </div>
    <div class="option clickable" onclick="selectOption('D', '{{correct}}')">
      <span class="option-label">D</span>
      <span class="option-text">{{optiond}}</span>
      <span class="feedback" style="display: none;"></span>
    </div>
  </div>
  
  <div class="instruction">点击选项查看结果</div>
  
  {{#explanation}}
  <div class="explanation-section" style="display: none;">
    <div class="explanation-label">💡 解析</div>
    <div class="explanation-text">{{explanation}}</div>
  </div>
  {{/explanation}}

  <script>
  function selectOption(selected, correct) {
    const options = document.querySelectorAll('.choice-card .option');
    const explanationSection = document.querySelector('.choice-card .explanation-section');
    
    options.forEach(option => {
      option.classList.remove('clickable');
      option.onclick = null;
      
      const label = option.querySelector('.option-label').textContent;
      const feedback = option.querySelector('.feedback');
      
      if (label === selected) {
        if (selected === correct) {
          option.classList.add('correct-selected');
          feedback.innerHTML = '✓ 正确';
          feedback.style.color = '#22c55e';
        } else {
          option.classList.add('wrong-selected');
          feedback.innerHTML = '✗ 错误';
          feedback.style.color = '#ef4444';
        }
        feedback.style.display = 'inline';
      } else if (label === correct) {
        option.classList.add('correct-answer');
        const correctFeedback = option.querySelector('.feedback');
        correctFeedback.innerHTML = '✓ 正确答案';
        correctFeedback.style.color = '#22c55e';
        correctFeedback.style.display = 'inline';
      }
    });
    
    if (explanationSection) {
      explanationSection.style.display = 'block';
    }
    
    document.querySelector('.choice-card .instruction').textContent = 
      selected === correct ? '答对了！' : '答错了，正确答案是 ' + correct;
  }
  </script>
</div>"#.to_string(),
            r#"<div class="card choice-card">
  <div class="question-section">
    <div class="question-label">📝 题目</div>
    <div class="question-text">{{Front}}</div>
  </div>
  
  <div class="options-section answered">
    <div class="option">
      <span class="option-label">A</span>
      <span class="option-text">{{optiona}}</span>
    </div>
    <div class="option">
      <span class="option-label">B</span>
      <span class="option-text">{{optionb}}</span>
    </div>
    <div class="option">
      <span class="option-label">C</span>
      <span class="option-text">{{optionc}}</span>
    </div>
    <div class="option">
      <span class="option-label">D</span>
      <span class="option-text">{{optiond}}</span>
    </div>
  </div>
  
  <div class="answer-section">
    <div class="answer-label">✅ 正确答案：{{correct}}</div>
  </div>
  
  {{#explanation}}
  <div class="explanation-section">
    <div class="explanation-label">💡 解析</div>
    <div class="explanation-text">{{explanation}}</div>
  </div>
  {{/explanation}}
</div>"#.to_string(),
            r#".choice-card {
  font-family: 'PingFang SC', 'Microsoft YaHei', sans-serif;
  width: 100%;
  max-width: 100%;
  margin: 0;
  padding: 20px;
  background: #f8fafc;
  border: 2px solid #e2e8f0;
  border-radius: 12px;
  box-shadow: 0 4px 20px rgba(0,0,0,0.08);
  box-sizing: border-box;
  overflow: hidden;
}

.question-section {
  margin-bottom: 20px;
}

.question-label {
  font-size: 14px;
  font-weight: 600;
  color: #3b82f6;
  margin-bottom: 8px;
}

.question-text {
  font-size: 18px;
  font-weight: 500;
  color: #1e293b;
  line-height: 1.6;
  word-wrap: break-word;
}

.options-section {
  margin-bottom: 20px;
}

.option {
  display: flex;
  align-items: center;
  padding: 12px;
  margin: 8px 0;
  background: white;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  transition: all 0.2s ease;
  position: relative;
}

.option.clickable {
  cursor: pointer;
}

.option.clickable:hover {
  border-color: #3b82f6;
  box-shadow: 0 2px 8px rgba(59, 130, 246, 0.1);
  transform: translateY(-1px);
}

.option.correct-selected {
  background: #dcfce7;
  border-color: #22c55e;
  box-shadow: 0 2px 8px rgba(34, 197, 94, 0.2);
}

.option.wrong-selected {
  background: #fee2e2;
  border-color: #ef4444;
  box-shadow: 0 2px 8px rgba(239, 68, 68, 0.2);
}

.option.correct-answer {
  background: #f0f9ff;
  border-color: #22c55e;
  box-shadow: 0 2px 8px rgba(34, 197, 94, 0.15);
}

.feedback {
  font-weight: 600;
  font-size: 14px;
  margin-left: auto;
}

.option-label {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  background: #f1f5f9;
  color: #475569;
  border-radius: 50%;
  font-weight: 600;
  font-size: 14px;
  margin-right: 12px;
  flex-shrink: 0;
}

.option-text {
  flex: 1;
  font-size: 16px;
  color: #334155;
  line-height: 1.5;
  word-wrap: break-word;
}

.answer-section {
  background: #dcfce7;
  border: 1px solid #22c55e;
  border-radius: 8px;
  padding: 12px;
  margin: 15px 0;
}

.answer-label {
  font-weight: 600;
  color: #15803d;
  font-size: 16px;
}

.explanation-section {
  background: #fffbeb;
  border: 1px solid #f59e0b;
  border-radius: 8px;
  padding: 15px;
  margin-top: 15px;
}

.explanation-label {
  font-weight: 600;
  color: #d97706;
  margin-bottom: 8px;
  font-size: 14px;
}

.explanation-text {
  color: #92400e;
  line-height: 1.6;
  font-size: 14px;
  word-wrap: break-word;
}

.instruction {
  text-align: center;
  color: #64748b;
  font-style: italic;
  font-size: 14px;
  padding: 10px;
  background: #f1f5f9;
  border-radius: 6px;
}"#.to_string(),
        )),
        _ => Err(format!("未知的模板ID: {}", template_id))
    }
}

// 分析新错题 - 流式版本
#[tauri::command]
pub async fn analyze_new_mistake_stream(
    request: AnalysisRequest,
    window: Window,
    state: State<'_, AppState>,
) -> Result<AnalysisResponse> {
    // 生成临时ID
    let temp_id = Uuid::new_v4().to_string();
    
    println!("🚀 启动流式AI解答: {}, 思维链: {}", temp_id, true);
    // 安全地截取用户问题，使用字符边界而非字节边界
    let question_preview = if request.user_question.chars().count() > 50 {
        request.user_question.chars().take(50).collect::<String>() + "..."
    } else {
        request.user_question.clone()
    };
    
    println!("📝 请求信息: 科目={}, 题目图片={}, 解析图片={}, 用户问题={}", 
        request.subject, 
        request.question_image_files.len(),
        request.analysis_image_files.len(),
        question_preview
    );
    
    // 保存图片到本地
    let mut question_image_paths = Vec::new();
    for (index, base64_data) in request.question_image_files.iter().enumerate() {
        let file_name = format!("question_{}_{}.jpg", temp_id, index);
        let path = state.file_manager.save_image_from_base64(base64_data, &file_name)
            .await?;
        question_image_paths.push(path);
    }
    
    let mut analysis_image_paths = Vec::new();
    for (index, base64_data) in request.analysis_image_files.iter().enumerate() {
        let file_name = format!("analysis_{}_{}.jpg", temp_id, index);
        let path = state.file_manager.save_image_from_base64(base64_data, &file_name)
            .await?;
        analysis_image_paths.push(path);
    }
    
    // 调用分析服务（流式）
    let stream_event = format!("analysis_stream_{}", temp_id);
    let analysis_result = state.analysis_service.analyze_mistake_stream(
        &question_image_paths,
        &request.user_question,
        &request.subject,
        window,
        &stream_event,
    ).await?;
    
    // 创建临时会话
    let temp_session = TempSession {
        temp_id: temp_id.clone(),
        subject: request.subject,
        question_images: question_image_paths,
        analysis_images: analysis_image_paths,
        user_question: request.user_question,
        ocr_text: analysis_result.ocr_text.clone(),
        tags: analysis_result.tags.clone(),
        mistake_type: analysis_result.mistake_type.clone(),
        chat_history: vec![ChatMessage {
            role: "assistant".to_string(),
            content: analysis_result.first_answer.clone(),
            timestamp: Utc::now(),
            thinking_content: None, // 思维链内容由流式事件处理
            rag_sources: None,
            image_paths: None,
            image_base64: None,
        }],
        created_at: Utc::now(),
    };
    
    // 保存临时会话
    {
        let mut sessions = state.temp_sessions.lock().await;
        sessions.insert(temp_id.clone(), temp_session);
    }
    
    Ok(AnalysisResponse {
        temp_id,
        initial_data: InitialAnalysisData {
            ocr_text: analysis_result.ocr_text,
            tags: analysis_result.tags,
            mistake_type: analysis_result.mistake_type,
            first_answer: analysis_result.first_answer,
        },
    })
}

// 分析新错题 - 非流式版本（已废弃，为了兼容性保留）
#[tauri::command]
pub async fn analyze_new_mistake(
    _request: AnalysisRequest,
    _state: State<'_, AppState>,
) -> Result<AnalysisResponse> {
    println!("警告: analyze_new_mistake 非流式版本已废弃");
    Err(AppError::validation("非流式版本已废弃，请使用 analyze_new_mistake_stream"))
}

// 继续对话 - 流式版本
#[tauri::command]
pub async fn continue_chat_stream(
    request: ContinueChatRequest,
    window: Window,
    state: State<'_, AppState>,
) -> Result<ContinueChatResponse> {
    println!("继续对话(流式): {}", request.temp_id);
    
    // 获取临时会话
    let temp_session = {
        let sessions = state.temp_sessions.lock().await;
        sessions.get(&request.temp_id).cloned()
    };
    
    let mut session = temp_session.ok_or("临时会话不存在")?;
    
    // 构建上下文
    let mut context = std::collections::HashMap::new();
    context.insert("ocr_text".to_string(), serde_json::json!(session.ocr_text));
    context.insert("tags".to_string(), serde_json::json!(session.tags));
    context.insert("mistake_type".to_string(), serde_json::json!(session.mistake_type));
    context.insert("user_question".to_string(), serde_json::json!(session.user_question));
    
    // 启动流式对话
    let stream_event = format!("continue_chat_stream_{}", request.temp_id);
    
    // 获取模型配置以判断是否是推理模型
    let model_config = state.llm_manager.get_model2_config().await
        .map_err(|e| format!("获取模型配置失败: {}", e))?;
    
    // 使用前端传入的思维链设置，如果没有则根据模型类型自动决定
    let enable_chain_of_thought = request.enable_chain_of_thought.unwrap_or(model_config.is_reasoning);
    
    let model2_result = state.llm_manager.call_unified_model_2_stream(
        &context,
        &request.chat_history,
        &session.subject,
        enable_chain_of_thought,
        Some(session.question_images.clone()), // 🎯 修复：传入图片路径给第二模型
        Some("基于题目信息继续对话解答用户问题"),
        window,
        &stream_event,
    ).await.map_err(|e| format!("流式对话失败: {}", e))?;
    
    // 更新会话的聊天记录
    session.chat_history = request.chat_history;
    session.chat_history.push(ChatMessage {
        role: "assistant".to_string(),
        content: model2_result.assistant_message.clone(),
        timestamp: Utc::now(),
        thinking_content: model2_result.chain_of_thought_details.map(|details| details.to_string()),
        rag_sources: None,
        image_paths: None,
        image_base64: None,
    });
    
    // 保存更新后的会话
    {
        let mut sessions = state.temp_sessions.lock().await;
        sessions.insert(request.temp_id, session);
    }
    
    Ok(ContinueChatResponse {
        new_assistant_message: model2_result.assistant_message,
    })
}

// 继续对话 - 非流式版本（已废弃，为了兼容性保留）
#[tauri::command]
pub async fn continue_chat(
    _request: ContinueChatRequest,
    _state: State<'_, AppState>,
) -> Result<ContinueChatResponse> {
    println!("警告: continue_chat 非流式版本已废弃");
    Err(AppError::validation("非流式版本已废弃，请使用 continue_chat_stream"))
}

#[tauri::command]
pub async fn save_mistake_from_analysis(
    request: SaveMistakeRequest,
    state: State<'_, AppState>,
) -> Result<SaveMistakeResponse> {
    println!("保存错题分析结果: {}", request.temp_id);
    
    // 获取临时会话
    let temp_session = {
        let sessions = state.temp_sessions.lock().await;
        sessions.get(&request.temp_id).cloned()
    };
    
    let session = temp_session.ok_or("临时会话不存在")?;
    
    // 创建错题项
    let mistake_item = MistakeItem {
        id: Uuid::new_v4().to_string(),
        subject: session.subject,
        created_at: session.created_at,
        question_images: session.question_images,
        analysis_images: session.analysis_images,
        user_question: session.user_question,
        ocr_text: session.ocr_text,
        tags: session.tags,
        mistake_type: session.mistake_type,
        status: "summary_required".to_string(), // 需要生成总结才能完成
        updated_at: Utc::now(),
        chat_history: request.final_chat_history,
        mistake_summary: None,       // 新增字段：需要后续生成
        user_error_analysis: None,   // 新增字段：需要后续生成
    };
    
    // 保存到数据库
    match state.database.save_mistake(&mistake_item) {
        Ok(_) => {
            // 清理临时会话
            {
                let mut sessions = state.temp_sessions.lock().await;
                sessions.remove(&request.temp_id);
            }
            
            Ok(SaveMistakeResponse {
                success: true,
                final_mistake_item: Some(mistake_item),
            })
        }
        Err(e) => Err(AppError::database(format!("保存错题失败: {}", e))),
    }
}

// 错题库管理命令

#[tauri::command]
pub async fn get_mistakes(
    subject: Option<String>,
    mistake_type: Option<String>,
    tags: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<Vec<MistakeItem>> {
    println!("获取错题列表");
    
    let subject_filter = subject.as_deref();
    let type_filter = mistake_type.as_deref();
    let tags_filter = tags.as_ref().map(|v| v.as_slice());
    
    match state.database.get_mistakes(subject_filter, type_filter, tags_filter) {
        Ok(mistakes) => Ok(mistakes),
        Err(e) => Err(AppError::database(format!("获取错题列表失败: {}", e))),
    }
}

#[tauri::command]
pub async fn get_review_analyses(
    subject: Option<String>,
    status: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ReviewAnalysisItem>> {
    println!("获取回顾分析列表");
    
    let subject_filter = subject.as_deref();
    let status_filter = status.as_deref();
    
    match state.database.get_review_analyses(subject_filter, status_filter) {
        Ok(analyses) => Ok(analyses),
        Err(e) => Err(AppError::database(format!("获取回顾分析列表失败: {}", e))),
    }
}

#[tauri::command]
pub async fn delete_review_analysis(
    id: String,
    state: State<'_, AppState>,
) -> Result<bool> {
    println!("删除回顾分析: {}", id);
    
    match state.database.delete_review_analysis(&id) {
        Ok(deleted) => {
            if deleted {
                println!("✅ 回顾分析删除成功: {}", id);
            } else {
                println!("⚠️ 回顾分析不存在: {}", id);
            }
            Ok(deleted)
        },
        Err(e) => {
            println!("❌ 删除回顾分析失败: {}", e);
            Err(AppError::database(format!("删除回顾分析失败: {}", e)))
        },
    }
}

#[tauri::command]
pub async fn get_mistake_details(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<MistakeItem>> {
    println!("获取错题详情: {}", id);
    
    match state.database.get_mistake_by_id(&id) {
        Ok(mistake) => Ok(mistake),
        Err(e) => Err(AppError::database(format!("获取错题详情失败: {}", e))),
    }
}

#[tauri::command]
pub async fn update_mistake(
    mistake: MistakeItem,
    state: State<'_, AppState>,
) -> Result<MistakeItem> {
    println!("更新错题: {}", mistake.id);
    
    match state.database.save_mistake(&mistake) {
        Ok(_) => Ok(mistake),
        Err(e) => Err(AppError::database(format!("更新错题失败: {}", e))),
    }
}

#[tauri::command]
pub async fn delete_mistake(
    id: String,
    state: State<'_, AppState>,
) -> Result<bool> {
    println!("删除错题: {}", id);
    
    // 首先获取错题信息以获取图片路径
    if let Ok(Some(mistake)) = state.database.get_mistake_by_id(&id) {
        // 删除关联的图片文件
        for image_path in mistake.question_images.iter().chain(mistake.analysis_images.iter()) {
            if let Err(e) = state.file_manager.delete_image(image_path).await {
                println!("删除图片文件失败: {}, 错误: {}", image_path, e);
            }
        }
    }
    
    // 删除数据库记录
    match state.database.delete_mistake(&id) {
        Ok(deleted) => Ok(deleted),
        Err(e) => Err(AppError::database(format!("删除错题失败: {}", e))),
    }
}

// 🎯 修复BUG-03：新的错题对话接口，支持显式总结请求参数
#[tauri::command]
pub async fn continue_mistake_chat_stream_v2(
    mistakeId: String,
    chatHistory: Vec<ChatMessage>,
    userMessage: String,
    isSummaryRequest: Option<bool>,
    enableChainOfThought: Option<bool>,
    window: Window,
    state: State<'_, AppState>,
) -> Result<ContinueChatResponse> {
    println!("在错题详情页继续对话(流式v2): {}, 总结请求: {:?}", mistakeId, isSummaryRequest);

    // 获取错题详情
    let mistake = state.database.get_mistake_by_id(&mistakeId)
        .map_err(|e| AppError::database(format!("获取错题失败: {}", e)))?
        .ok_or_else(|| AppError::not_found("错题不存在"))?;

    // 构建上下文
    let mut context = std::collections::HashMap::new();
    context.insert("ocr_text".to_string(), serde_json::json!(mistake.ocr_text));
    context.insert("tags".to_string(), serde_json::json!(mistake.tags));
    context.insert("user_question".to_string(), serde_json::json!(mistake.user_question));
    context.insert("mistake_type".to_string(), serde_json::json!(mistake.mistake_type));

    let is_summary_request = isSummaryRequest.unwrap_or(false);

    // 根据是否为总结请求选择合适的模型配置
    let model_config = if is_summary_request {
        println!("🔍 显式总结请求，尝试使用总结模型");

        // 获取模型分配配置
        let model_assignments = state.llm_manager.get_model_assignments().await
            .map_err(|e| format!("获取模型分配失败: {}", e))?;

        // 优先使用总结模型，如果未配置则回退到第二模型
        let target_model_id = model_assignments.summary_model_config_id
            .or(model_assignments.model2_config_id)
            .ok_or_else(|| "没有配置可用的总结模型或第二模型".to_string())?;

        // 获取目标模型配置
        let api_configs = state.llm_manager.get_api_configs().await
            .map_err(|e| format!("获取API配置失败: {}", e))?;

        let target_config = api_configs.iter()
            .find(|config| config.id == target_model_id && config.enabled)
            .ok_or_else(|| format!("找不到可用的目标模型配置: {}", target_model_id))?;

        println!("📋 总结请求使用模型: {} ({})", target_config.name, target_config.model);
        target_config.clone()
    } else {
        // 常规对话使用第二模型
        state.llm_manager.get_model2_config().await
            .map_err(|e| format!("获取模型配置失败: {}", e))?
    };

    // 决定是否启用思维链
    let enable_cot = if is_summary_request {
        enableChainOfThought.unwrap_or(false)
    } else {
        enableChainOfThought.unwrap_or(model_config.is_reasoning)
    };

    // 为多模态模型传递图片信息
    let image_paths = if model_config.is_multimodal && !mistake.question_images.is_empty() {
        Some(mistake.question_images.clone())
    } else {
        None
    };

    // 构建完整的聊天历史（包含新的用户消息）
    let mut full_chat_history = chatHistory;
    if !userMessage.trim().is_empty() {
        full_chat_history.push(ChatMessage {
            role: "user".to_string(),
            content: userMessage,
            timestamp: Utc::now(),
            thinking_content: None,
            rag_sources: None,
            image_paths: None,
            image_base64: None,
        });
    }

    // 调用统一AI接口获取回复（流式）
    let stream_event = format!("mistake_chat_stream_{}", mistakeId);

    let model2_result = if is_summary_request {
        // 总结请求使用动态选择的模型配置
        state.llm_manager.call_unified_model_stream_with_config(
            &model_config,
            &context,
            &full_chat_history,
            &mistake.subject,
            enable_cot,
            image_paths.clone(),
            Some("基于题目信息和聊天记录生成学习总结"),
            window,
            &stream_event,
        ).await.map_err(|e| format!("获取AI回复失败: {}", e))?
    } else {
        // 常规对话使用第二模型
        state.llm_manager.call_unified_model_2_stream(
            &context,
            &full_chat_history,
            &mistake.subject,
            enable_cot,
            image_paths,
            Some("基于题目信息和聊天记录进行深入追问解答"),
            window,
            &stream_event,
        ).await.map_err(|e| format!("获取AI回复失败: {}", e))?
    };

    let response = model2_result.assistant_message.clone();

    // 🎯 修复总结BUG：过滤掉魔法字符串，避免污染数据库
    let filtered_chat_history: Vec<ChatMessage> = full_chat_history.into_iter()
        .filter(|msg| !msg.content.contains("[SUMMARY_REQUEST]"))
        .collect();

    // 更新错题的聊天记录
    let mut updated_mistake = mistake;
    updated_mistake.chat_history = filtered_chat_history;
    
    // 🎯 关键修复：只有非总结请求才将AI响应添加到聊天记录
    if !is_summary_request {
        updated_mistake.chat_history.push(ChatMessage {
            role: "assistant".to_string(),
            content: response.clone(),
            timestamp: Utc::now(),
            thinking_content: extract_thinking_content_from_model_output(&model2_result),
            rag_sources: None,
            image_paths: None,
            image_base64: None,
        });
        println!("✅ [聊天记录v2] 普通对话响应已添加到聊天记录");
    } else {
        println!("🚫 [总结请求v2] 总结响应不添加到聊天记录，避免显示为第二个AI回复");
    }
    
    updated_mistake.updated_at = Utc::now();

    // 保存更新后的错题
    if let Err(e) = state.database.save_mistake(&updated_mistake) {
        println!("保存聊天记录失败: {}", e);
    }

    Ok(ContinueChatResponse {
        new_assistant_message: response,
    })
}

// 在错题详情页继续对话 - 流式版本（保持向后兼容）
#[tauri::command]
pub async fn continue_mistake_chat_stream(
    mistakeId: String,
    chatHistory: Vec<ChatMessage>,
    enableChainOfThought: Option<bool>,
    window: Window,
    state: State<'_, AppState>,
) -> Result<ContinueChatResponse> {
    println!("在错题详情页继续对话(流式): {}", mistakeId);
    
    // 获取错题详情
    let mistake = state.database.get_mistake_by_id(&mistakeId)
        .map_err(|e| AppError::database(format!("获取错题失败: {}", e)))?
        .ok_or_else(|| AppError::not_found("错题不存在"))?;
    
    // 构建上下文
    let mut context = std::collections::HashMap::new();
    context.insert("ocr_text".to_string(), serde_json::json!(mistake.ocr_text));
    context.insert("tags".to_string(), serde_json::json!(mistake.tags));
    context.insert("user_question".to_string(), serde_json::json!(mistake.user_question));
    context.insert("mistake_type".to_string(), serde_json::json!(mistake.mistake_type));

    // 🎯 新增：检测是否为总结请求
    let is_summary_request = chatHistory.last()
        .map(|msg| msg.content.contains("[SUMMARY_REQUEST]"))
        .unwrap_or(false);
    
    // 🎯 新增：根据是否为总结请求选择合适的模型配置
    let model_config = if is_summary_request {
        println!("🔍 检测到总结请求，尝试使用总结模型");
        
        // 获取模型分配配置
        let model_assignments = state.llm_manager.get_model_assignments().await
            .map_err(|e| format!("获取模型分配失败: {}", e))?;
        
        // 优先使用总结模型，如果未配置则回退到第二模型
        let target_model_id = model_assignments.summary_model_config_id
            .or(model_assignments.model2_config_id)
            .ok_or_else(|| "没有配置可用的总结模型或第二模型".to_string())?;
        
        // 获取目标模型配置
        let api_configs = state.llm_manager.get_api_configs().await
            .map_err(|e| format!("获取API配置失败: {}", e))?;
        
        let target_config = api_configs.iter()
            .find(|config| config.id == target_model_id && config.enabled)
            .ok_or_else(|| format!("找不到可用的目标模型配置: {}", target_model_id))?;
        
        println!("📋 总结请求使用模型: {} ({})", target_config.name, target_config.model);
        target_config.clone()
    } else {
        // 常规对话使用第二模型
        state.llm_manager.get_model2_config().await
            .map_err(|e| format!("获取模型配置失败: {}", e))?
    };
    
    // 决定是否启用思维链：用户设置优先，否则根据模型类型自动决定
    // 总结请求默认不启用思维链（除非用户明确要求）
    let enable_cot = if is_summary_request {
        enableChainOfThought.unwrap_or(false)
    } else {
        enableChainOfThought.unwrap_or(model_config.is_reasoning)
    };

    // 调用统一AI接口获取回复（流式）
    let stream_event = format!("mistake_chat_stream_{}", mistakeId);
    
    // 🎯 修复BUG-01：为多模态模型传递图片信息
    let image_paths = if model_config.is_multimodal && !mistake.question_images.is_empty() {
        Some(mistake.question_images.clone())
    } else {
        None
    };

    // 🎯 新增：根据模型配置调用相应的AI接口
    let model2_result = if is_summary_request {
        // 总结请求使用动态选择的模型配置
        state.llm_manager.call_unified_model_stream_with_config(
            &model_config,
            &context,
            &chatHistory,
            &mistake.subject,
            enable_cot,
            image_paths.clone(), // 🎯 修复：传递图片信息
            Some("基于题目信息和聊天记录生成学习总结"),
            window,
            &stream_event,
        ).await.map_err(|e| format!("获取AI回复失败: {}", e))?
    } else {
        // 常规对话使用第二模型
        state.llm_manager.call_unified_model_2_stream(
            &context,
            &chatHistory,
            &mistake.subject,
            enable_cot,
            image_paths, // 🎯 修复：传递图片信息
            Some("基于题目信息和聊天记录进行深入追问解答"),
            window,
            &stream_event,
        ).await.map_err(|e| format!("获取AI回复失败: {}", e))?
    };
    
    let response = model2_result.assistant_message.clone(); // Clone to avoid partial move
    
    // 🎯 修复总结BUG：检测是否为总结请求，如果是则不将响应添加到聊天记录
    let is_summary_request = chatHistory.last()
        .map(|msg| msg.content.contains("[SUMMARY_REQUEST]"))
        .unwrap_or(false);

    // 🎯 修复BUG-03：过滤掉魔法字符串，避免污染数据库
    let filtered_chat_history: Vec<ChatMessage> = chatHistory.into_iter()
        .filter(|msg| !msg.content.contains("[SUMMARY_REQUEST]"))
        .collect();

    // 更新错题的聊天记录
    let mut updated_mistake = mistake;
    updated_mistake.chat_history = filtered_chat_history;
    
    // 🎯 关键修复：只有非总结请求才将AI响应添加到聊天记录
    if !is_summary_request {
        updated_mistake.chat_history.push(ChatMessage {
            role: "assistant".to_string(),
            content: response.clone(),
            timestamp: Utc::now(),
            thinking_content: extract_thinking_content_from_model_output(&model2_result),
            rag_sources: None,
            image_paths: None,
            image_base64: None,
        });
        println!("✅ [聊天记录] 普通对话响应已添加到聊天记录");
    } else {
        println!("🚫 [总结请求] 总结响应不添加到聊天记录，避免显示为第二个AI回复");
    }
    
    updated_mistake.updated_at = Utc::now();
    
    // 保存更新后的错题
    if let Err(e) = state.database.save_mistake(&updated_mistake) {
        println!("保存聊天记录失败: {}", e);
    }
    
    Ok(ContinueChatResponse {
        new_assistant_message: response,
    })
}

// 在错题详情页继续对话 - 非流式版本（已废弃，为了兼容性保留）
#[tauri::command]
pub async fn continue_mistake_chat(
    _mistake_id: String,
    _chat_history: Vec<ChatMessage>,
    _state: State<'_, AppState>,
) -> Result<ContinueChatResponse> {
    println!("警告: continue_mistake_chat 非流式版本已废弃");
    Err(AppError::validation("非流式版本已废弃，请使用 continue_mistake_chat_stream"))
}

// 回顾分析命令 - 流式版本
#[tauri::command]
pub async fn analyze_review_session_stream(
    subject: String,
    mistake_ids: Vec<String>,
    window: Window,
    state: State<'_, AppState>,
) -> Result<ReviewSessionResponse> {
    println!("回顾分析(流式): {} 个错题", mistake_ids.len());
    
    // 获取所有选中的错题
    let mut mistakes = Vec::new();
    for id in &mistake_ids {
        match state.database.get_mistake_by_id(id) {
            Ok(Some(mistake)) => mistakes.push(mistake),
            Ok(None) => return Err(AppError::not_found(format!("错题不存在: {}", id))),
            Err(e) => return Err(AppError::database(format!("获取错题失败: {}", e))),
        }
    }
    
    // 调用分析服务进行回顾分析（流式）
    let stream_event = "review_analysis_stream";
    let analysis_result = state.analysis_service.analyze_review_session_stream(
        &mistakes,
        &subject,
        window,
        stream_event,
    ).await.map_err(|e| format!("回顾分析失败: {}", e))?;
    
    let review_id = Uuid::new_v4().to_string();
    let now = Utc::now();
    
    // 创建回顾分析会话
    let review_session = ReviewSession {
        id: review_id.clone(),
        subject: subject.clone(),
        mistake_ids: mistake_ids.clone(),
        analysis_summary: analysis_result.clone(),
        created_at: now,
        updated_at: now,
        chat_history: vec![ReviewChatMessage {
            id: Uuid::new_v4().to_string(),
            session_id: review_id.clone(),
            role: "assistant".to_string(),
            content: analysis_result.clone(),
            timestamp: now,
        }],
    };
    
    // 保存回顾分析会话到数据库
    match state.database.save_review_session(&review_session) {
        Ok(_) => {
            println!("回顾分析会话已保存: {}", review_id);
        },
        Err(e) => {
            println!("保存回顾分析会话失败: {}", e);
            return Err(AppError::database(format!("保存回顾分析会话失败: {}", e)));
        }
    }
    
    Ok(ReviewSessionResponse {
        review_id,
        analysis_summary: analysis_result.clone(),
        chat_history: Some(vec![ChatMessage {
            role: "assistant".to_string(),
            content: analysis_result,
            timestamp: now,
            thinking_content: None,
            rag_sources: None,
            image_paths: None,
            image_base64: None,
        }]),
    })
}


// 统计和设置命令

#[tauri::command]
pub async fn get_statistics(
    state: State<'_, AppState>,
) -> Result<serde_json::Value> {
    println!("获取统计信息");
    
    match state.database.get_statistics() {
        Ok(stats) => {
            let stats_json = serde_json::to_value(stats)
                .map_err(|e| format!("序列化统计数据失败: {}", e))?;
            Ok(stats_json)
        }
        Err(e) => Err(AppError::database(format!("获取统计信息失败: {}", e))),
    }
}

#[tauri::command]
pub async fn save_setting(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("保存设置: {} = {}", key, value);
    
    match state.database.save_setting(&key, &value) {
        Ok(_) => Ok(()),
        Err(e) => Err(AppError::database(format!("保存设置失败: {}", e))),
    }
}

#[tauri::command]
pub async fn get_setting(
    key: String,
    state: State<'_, AppState>,
) -> Result<Option<String>> {
    println!("获取设置: {}", key);
    
    match state.database.get_setting(&key) {
        Ok(value) => Ok(value),
        Err(e) => Err(AppError::database(format!("获取设置失败: {}", e))),
    }
}

#[tauri::command]
pub async fn test_api_connection(
    api_key: String,
    api_base: String,
    model: Option<String>,
    state: State<'_, AppState>,
) -> Result<bool> {
    println!("测试API连接: {} (模型: {:?})", api_base, model);
    
    // 创建临时的LLM管理器进行测试
    let temp_llm = crate::llm_manager::LLMManager::new(state.database.clone(), state.file_manager.clone());
    
    let result = if let Some(model_name) = model.as_deref() {
        temp_llm.test_connection_with_model(&api_key, &api_base, Some(model_name)).await
    } else {
        temp_llm.test_connection(&api_key, &api_base).await
    };
    
    match result {
        Ok(success) => {
            println!("API连接测试结果: {}", success);
            if !success {
                println!("API连接测试失败：虽然没有异常，但测试返回了false");
            }
            Ok(success)
        },
        Err(e) => {
            println!("API连接测试错误: {}", e);
            // 为了获得更详细的错误信息，我们返回错误而不是false
            Err(AppError::validation(format!("API连接测试失败: {}", e)))
        }
    }
}

#[tauri::command]
pub async fn get_supported_subjects(
    state: State<'_, AppState>,
) -> Result<Vec<String>> {
    println!("获取支持的科目（从数据库动态获取）");
    
    // 从数据库获取所有已启用的科目配置
    match state.database.get_all_subject_configs(true) {
        Ok(configs) => {
            let subjects: Vec<String> = configs
                .into_iter()
                .map(|config| config.subject_name)
                .collect();
            
            println!("从数据库获取到 {} 个启用的科目: {:?}", subjects.len(), subjects);
            
            // 如果数据库中没有科目配置，返回默认科目列表并记录警告
            if subjects.is_empty() {
                println!("⚠️ 数据库中没有启用的科目配置，返回默认科目列表");
                Ok(vec![
                    "数学".to_string(),
                    "物理".to_string(),
                    "化学".to_string(),
                    "英语".to_string(),
                    "语文".to_string(),
                    "生物".to_string(),
                    "历史".to_string(),
                    "地理".to_string(),
                    "政治".to_string(),
                ])
            } else {
                Ok(subjects)
            }
        }
        Err(e) => {
            println!("⚠️ 从数据库获取科目配置失败: {}，返回默认科目列表", e);
            // 数据库查询失败时返回默认科目列表，确保应用可用性
            Ok(vec![
                "数学".to_string(),
                "物理".to_string(),
                "化学".to_string(),
                "英语".to_string(),
                "语文".to_string(),
                "生物".to_string(),
                "历史".to_string(),
                "地理".to_string(),
                "政治".to_string(),
            ])
        }
    }
}

// 文件管理命令

#[tauri::command]
pub async fn get_image_as_base64(
    relative_path: String,
    state: State<'_, AppState>,
) -> Result<String> {
    println!("获取图片base64: {}", relative_path);
    
    match state.file_manager.get_image_as_base64(&relative_path).await {
        Ok(base64_data) => Ok(base64_data),
        Err(e) => Err(AppError::database(format!("获取图片失败: {}", e))),
    }
}

#[tauri::command]
pub async fn save_image_from_base64_path(
    base64_data: String,
    file_name: String,
    state: State<'_, AppState>,
) -> Result<String> {
    println!("从Base64保存图片: {}", file_name);
    
    match state.file_manager.save_image_from_base64(&base64_data, &file_name).await {
        Ok(saved_path) => Ok(saved_path),
        Err(e) => Err(AppError::database(format!("保存图片失败: {}", e))),
    }
}

#[tauri::command]
pub async fn cleanup_orphaned_images(
    state: State<'_, AppState>,
) -> Result<Vec<String>> {
    println!("清理孤立图片");
    
    match state.file_manager.cleanup_orphaned_images(&state.database).await {
        Ok(cleaned_files) => Ok(cleaned_files),
        Err(e) => Err(AppError::database(format!("清理孤立图片失败: {}", e))),
    }
}

/// 获取图片统计信息
#[tauri::command]
pub async fn get_image_statistics(
    state: State<'_, AppState>,
) -> Result<crate::file_manager::ImageStatistics> {
    println!("获取图片统计信息");
    
    state.file_manager.get_image_statistics().await
        .map_err(|e| AppError::database(format!("获取图片统计失败: {}", e)))
}


// 专用配置管理命令

#[tauri::command]
pub async fn get_api_configurations(
    state: State<'_, AppState>,
) -> Result<Vec<ApiConfig>> {
    println!("获取API配置列表");
    
    state.llm_manager.get_api_configs().await
}

#[tauri::command]
pub async fn save_api_configurations(
    configs: Vec<ApiConfig>,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("保存API配置列表: {} 个配置", configs.len());
    
    state.llm_manager.save_api_configurations(&configs).await
}

#[tauri::command]
pub async fn get_model_assignments(
    state: State<'_, AppState>,
) -> Result<ModelAssignments> {
    println!("获取模型分配配置");
    
    state.llm_manager.get_model_assignments().await
}

#[tauri::command]
pub async fn save_model_assignments(
    assignments: ModelAssignments,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("保存模型分配配置");
    
    state.llm_manager.save_model_assignments(&assignments).await
}

// 回顾分析追问 - 流式版本
#[tauri::command]
pub async fn continue_review_chat_stream(
    review_id: String,
    chat_history: Vec<ChatMessage>,
    window: Window,
    state: State<'_, AppState>,
) -> Result<ContinueChatResponse> {
    println!("回顾分析追问(流式): {}", review_id);
    
    // 获取回顾分析会话
    let review_session = state.database.get_review_session_by_id(&review_id)
        .map_err(|e| format!("获取回顾分析会话失败: {}", e))?
        .ok_or("回顾分析会话不存在")?;
    
    // 构建上下文信息
    let mut context = std::collections::HashMap::new();
    context.insert("review_type".to_string(), serde_json::json!("回顾分析"));
    context.insert("subject".to_string(), serde_json::json!(review_session.subject));
    context.insert("mistake_count".to_string(), serde_json::json!(review_session.mistake_ids.len()));
    context.insert("analysis_summary".to_string(), serde_json::json!(review_session.analysis_summary));
    
    // 获取模型配置以判断是否是推理模型
    let model_config = state.llm_manager.get_model2_config().await
        .map_err(|e| format!("获取模型配置失败: {}", e))?;
    
    // 推理模型自动启用思维链，回顾分析追问也需要深度思考
    let enable_chain_of_thought = model_config.is_reasoning || true; // 回顾分析追问总是启用思维链
    
    // 🎯 修复：获取相关错题的图片信息
    let mut all_image_paths = Vec::new();
    for mistake_id in &review_session.mistake_ids {
        if let Ok(Some(mistake)) = state.database.get_mistake_by_id(mistake_id) {
            all_image_paths.extend(mistake.question_images);
        }
    }
    
    // 🎯 修复：为多模态模型传递图片信息
    let image_paths = if model_config.is_multimodal && !all_image_paths.is_empty() {
        Some(all_image_paths)
    } else {
        None
    };

    // 调用统一AI接口获取回复（流式）
    let stream_event = format!("review_chat_stream_{}", review_id);
    let response = state.llm_manager.call_unified_model_2_stream(
        &context,
        &chat_history,
        &review_session.subject,
        enable_chain_of_thought,
        image_paths, // 🎯 修复：传递相关错题的图片信息
        Some("基于回顾分析结果和相关题目图片进行追问解答"),
        window,
        &stream_event,
    ).await.map_err(|e| format!("获取AI回复失败: {}", e))?;
    
    // 创建新的聊天消息
    let new_message = ReviewChatMessage {
        id: Uuid::new_v4().to_string(),
        session_id: review_id.clone(),
        role: "assistant".to_string(),
        content: response.assistant_message.clone(),
        timestamp: Utc::now(),
    };
    
    // 保存聊天消息到数据库
    if let Err(e) = state.database.add_review_chat_message(&new_message) {
        println!("保存回顾分析聊天记录失败: {}", e);
    }
    
    Ok(ContinueChatResponse {
        new_assistant_message: response.assistant_message,
    })
}

// 回顾分析追问 - 非流式版本（已废弃，为了兼容性保留）
#[tauri::command]
pub async fn continue_review_chat(
    _review_id: String,
    _chat_history: Vec<ChatMessage>,
    _state: State<'_, AppState>,
) -> Result<ContinueChatResponse> {
    println!("警告: continue_review_chat 非流式版本已废弃");
    Err(AppError::validation("非流式版本已废弃，请使用 continue_review_chat_stream"))
}

// 分步骤分析：先OCR，再流式AI解答
#[tauri::command]
pub async fn analyze_step_by_step(
    request: AnalysisRequest,
    state: State<'_, AppState>,
) -> Result<serde_json::Value> {
    println!("🚀 开始分步骤分析: 科目={}, 问题图片={}, 解析图片={}, 用户问题长度={}", 
        request.subject, 
        request.question_image_files.len(),
        request.analysis_image_files.len(),
        request.user_question.len()
    );
    
    let temp_id = Uuid::new_v4().to_string();
    println!("🆔 生成临时ID: {}", temp_id);
    
    // 保存上传的图片文件
    let mut question_image_paths = Vec::new();
    let mut analysis_image_paths = Vec::new();
    
    // 处理问题图片
    println!("📁 开始保存问题图片，共{}张", request.question_image_files.len());
    for (index, base64_data) in request.question_image_files.iter().enumerate() {
        let filename = format!("question_{}_{}.jpg", temp_id, index);
        println!("💾 保存问题图片 {}: {}", index + 1, filename);
        match state.file_manager.save_image_from_base64(base64_data, &filename).await {
            Ok(path) => {
                println!("✅ 问题图片保存成功: {}", path);
                question_image_paths.push(path);
            },
            Err(e) => {
                let error_msg = format!("保存问题图片失败: {}", e);
                println!("❌ {}", error_msg);
                return Err(AppError::database(error_msg));
            }
        }
    }
    
    // 处理解析图片
    for (index, base64_data) in request.analysis_image_files.iter().enumerate() {
        let filename = format!("analysis_{}_{}.jpg", temp_id, index);
        match state.file_manager.save_image_from_base64(base64_data, &filename).await {
            Ok(path) => analysis_image_paths.push(path),
            Err(e) => return Err(AppError::database(format!("保存解析图片失败: {}", e))),
        }
    }
    
    // 第一步：只进行OCR和分类分析
    println!("🔍 开始调用模型一进行OCR分析，图片数量: {}", question_image_paths.len());
    let model1_result = state.llm_manager.call_unified_model_1(
        question_image_paths.clone(),
        &request.user_question,
        &request.subject,
        None,
    ).await.map_err(|e| {
        let error_msg = format!("OCR分析失败: {}", e);
        println!("❌ {}", error_msg);
        AppError::llm(error_msg)
    })?;
    println!("✅ OCR分析完成");
    
    // 创建临时会话（暂时不包含AI解答）
    let temp_session = TempSession {
        temp_id: temp_id.clone(),
        subject: request.subject.clone(),
        question_images: question_image_paths,
        analysis_images: analysis_image_paths,
        user_question: request.user_question.clone(),
        ocr_text: model1_result.ocr_text.clone(),
        tags: model1_result.tags.clone(),
        mistake_type: model1_result.mistake_type.clone(),
        chat_history: vec![], // 暂时为空，等待流式填充
        created_at: Utc::now(),
    };
    
    // 保存临时会话
    {
        let mut sessions = state.temp_sessions.lock().await;
        sessions.insert(temp_id.clone(), temp_session);
    }
    
    // 返回OCR结果
    let response = serde_json::json!({
        "temp_id": temp_id,
        "ocr_result": {
            "ocr_text": model1_result.ocr_text,
            "tags": model1_result.tags,
            "mistake_type": model1_result.mistake_type
        }
    });
    
    Ok(response)
}

// 启动流式AI解答
#[tauri::command]
pub async fn start_streaming_answer(
    request: StartStreamingAnswerRequest,
    window: Window,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("启动流式AI解答: {}, 思维链: {}", request.temp_id, request.enable_chain_of_thought);
    
    // 获取临时会话
    let temp_session = {
        let sessions = state.temp_sessions.lock().await;
        sessions.get(&request.temp_id).cloned()
    };
    
    let session = temp_session.ok_or("临时会话不存在")?;
    
    // 构建上下文
    let mut context = std::collections::HashMap::new();
    context.insert("ocr_text".to_string(), serde_json::json!(session.ocr_text));
    context.insert("tags".to_string(), serde_json::json!(session.tags));
    context.insert("mistake_type".to_string(), serde_json::json!(session.mistake_type));
    context.insert("user_question".to_string(), serde_json::json!(session.user_question));
    
    // 启动流式AI解答
    let stream_event = format!("analysis_stream_{}", request.temp_id);
    let model2_result = state.llm_manager.call_unified_model_2_stream(
        &context,
        &[], // 空的聊天历史
        &session.subject,
        request.enable_chain_of_thought, // 使用传入的思维链参数
        Some(session.question_images.clone()), // 🎯 修复：传入图片路径给第二模型
        None, // 暂时不使用任务上下文
        window,
        &stream_event,
    ).await.map_err(|e| format!("流式AI解答失败: {}", e))?;
    
    // 更新临时会话的聊天历史
    {
        let mut sessions = state.temp_sessions.lock().await;
        if let Some(session) = sessions.get_mut(&request.temp_id) {
            session.chat_history.push(ChatMessage {
                role: "assistant".to_string(),
                content: model2_result.assistant_message,
                timestamp: Utc::now(),
                thinking_content: None,
                rag_sources: None,
                image_paths: None,
                image_base64: None,
            });
        }
    }
    
    Ok(())
}

// 获取支持的模型适配器选项
#[tauri::command]
pub async fn get_model_adapter_options(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>> {
    println!("获取模型适配器选项");
    
    // 尝试从数据库加载自定义适配器选项
    match state.database.get_setting("model_adapter_options") {
        Ok(Some(options_json)) => {
            // 尝试解析自定义配置
            match serde_json::from_str::<Vec<serde_json::Value>>(&options_json) {
                Ok(mut custom_options) => {
                    // 如果列表中缺少 Google(Gemini) 选项，则自动补充
                    let has_google = custom_options.iter().any(|item| {
                        item.get("value").and_then(|v| v.as_str()) == Some("google")
                    });

                    if !has_google {
                        if let Some(google_item) = get_default_model_adapter_options()
                            .into_iter()
                            .find(|item| item.get("value").and_then(|v| v.as_str()) == Some("google"))
                        {
                            println!("🚀 未检测到 Google (Gemini) 适配器，已自动添加。");
                            custom_options.push(google_item);

                            // 将更新后的列表写回数据库
                            if let Err(e) = state.database.save_setting(
                                "model_adapter_options",
                                &serde_json::to_string(&custom_options).unwrap_or_default(),
                            ) {
                                println!("⚠️  写回模型适配器选项失败: {}", e);
                            }
                        }
                    }

                    if !custom_options.is_empty() {
                        println!("使用自定义模型适配器选项: {} 个", custom_options.len());
                        return Ok(custom_options);
                    }
                }
                Err(e) => {
                    println!("解析自定义模型适配器选项失败: {}, 使用默认配置", e);
                }
            }
        }
        Ok(None) => {
            // 没有自定义配置，使用默认配置
            println!("没有找到自定义模型适配器选项，使用默认配置");
        }
        Err(e) => {
            println!("获取模型适配器选项配置失败: {}, 使用默认配置", e);
        }
    }
    
    // 返回默认的模型适配器选项
    let default_options = get_default_model_adapter_options();
    
    // 如果数据库中没有配置，保存默认配置供将来使用
    if let Err(e) = state.database.save_setting("model_adapter_options", &serde_json::to_string(&default_options).unwrap_or_default()) {
        println!("保存默认模型适配器选项失败: {}", e);
    }
    
    Ok(default_options)
}

/// 保存自定义模型适配器选项
#[tauri::command]
pub async fn save_model_adapter_options(
    state: State<'_, AppState>,
    options: Vec<serde_json::Value>,
) -> Result<()> {
    println!("保存自定义模型适配器选项: {} 个", options.len());
    
    // 验证选项格式
    for (i, option) in options.iter().enumerate() {
        if !option.is_object() ||
           option.get("value").is_none() ||
           option.get("label").is_none() {
            return Err(AppError::validation(format!(
                "模型适配器选项 {} 格式无效，必须包含 'value' 和 'label' 字段", i
            )));
        }
    }
    
    let options_json = serde_json::to_string(&options)
        .map_err(|e| AppError::validation(format!("序列化模型适配器选项失败: {}", e)))?;
    
    state.database.save_setting("model_adapter_options", &options_json)
        .map_err(|e| AppError::database(format!("保存模型适配器选项失败: {}", e)))?;
    
    println!("模型适配器选项保存成功");
    Ok(())
}

/// 重置模型适配器选项为默认值
#[tauri::command]
pub async fn reset_model_adapter_options(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>> {
    println!("重置模型适配器选项为默认值");
    
    let default_options = get_default_model_adapter_options();
    let options_json = serde_json::to_string(&default_options)
        .map_err(|e| AppError::validation(format!("序列化默认模型适配器选项失败: {}", e)))?;
    
    state.database.save_setting("model_adapter_options", &options_json)
        .map_err(|e| AppError::database(format!("重置模型适配器选项失败: {}", e)))?;
    
    println!("模型适配器选项重置成功");
    Ok(default_options)
}

// 科目配置管理命令

#[tauri::command]
pub async fn get_all_subject_configs(
    enabled_only: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<SubjectConfig>> {
    println!("获取科目配置列表: enabled_only={:?}", enabled_only);
    
    state.database.get_all_subject_configs(enabled_only.unwrap_or(false))
        .map_err(|e| AppError::database(format!("获取科目配置失败: {}", e)))
}

#[tauri::command]
pub async fn get_subject_config_by_id(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<SubjectConfig>> {
    println!("获取科目配置详情: {}", id);
    
    state.database.get_subject_config_by_id(&id)
        .map_err(|e| AppError::database(format!("获取科目配置失败: {}", e)))
}

#[tauri::command]
pub async fn get_subject_config_by_name(
    subject_name: String,
    state: State<'_, AppState>,
) -> Result<Option<SubjectConfig>> {
    println!("根据科目名称获取配置: {}", subject_name);
    
    state.database.get_subject_config_by_name(&subject_name)
        .map_err(|e| AppError::database(format!("获取科目配置失败: {}", e)))
}

#[tauri::command]
pub async fn create_subject_config(
    request: CreateSubjectConfigRequest,
    state: State<'_, AppState>,
) -> Result<SubjectConfig> {
    println!("创建科目配置: {}", request.subject_name);
    
    // 检查科目名称是否已存在
    if let Ok(Some(_)) = state.database.get_subject_config_by_name(&request.subject_name) {
        return Err(AppError::validation(format!("科目 '{}' 已存在", request.subject_name)));
    }
    
    let now = Utc::now();
    let config = SubjectConfig {
        id: Uuid::new_v4().to_string(),
        subject_name: request.subject_name,
        display_name: request.display_name,
        description: request.description.unwrap_or_default(),
        is_enabled: true,
        prompts: request.prompts.unwrap_or_default(),
        mistake_types: request.mistake_types.unwrap_or_else(|| vec![
            "计算错误".to_string(),
            "概念理解".to_string(),
            "方法应用".to_string(),
            "知识遗忘".to_string(),
            "审题不清".to_string(),
        ]),
        default_tags: request.default_tags.unwrap_or_else(|| vec![
            "基础知识".to_string(),
            "重点难点".to_string(),
            "易错点".to_string(),
        ]),
        created_at: now,
        updated_at: now,
    };
    
    state.database.save_subject_config(&config)
        .map_err(|e| AppError::database(format!("保存科目配置失败: {}", e)))?;
    
    Ok(config)
}

#[tauri::command]
pub async fn update_subject_config(
    request: UpdateSubjectConfigRequest,
    state: State<'_, AppState>,
) -> Result<SubjectConfig> {
    println!("更新科目配置: {}", request.id);
    
    // 获取现有配置
    let mut config = state.database.get_subject_config_by_id(&request.id)
        .map_err(|e| AppError::database(format!("获取科目配置失败: {}", e)))?
        .ok_or_else(|| AppError::not_found("科目配置不存在"))?;
    
    // 更新字段
    if let Some(display_name) = request.display_name {
        config.display_name = display_name;
    }
    if let Some(description) = request.description {
        config.description = description;
    }
    if let Some(is_enabled) = request.is_enabled {
        config.is_enabled = is_enabled;
    }
    if let Some(prompts) = request.prompts {
        config.prompts = prompts;
    }
    if let Some(mistake_types) = request.mistake_types {
        config.mistake_types = mistake_types;
    }
    if let Some(default_tags) = request.default_tags {
        config.default_tags = default_tags;
    }
    
    config.updated_at = Utc::now();
    
    state.database.save_subject_config(&config)
        .map_err(|e| AppError::database(format!("更新科目配置失败: {}", e)))?;
    
    Ok(config)
}

#[tauri::command]
pub async fn delete_subject_config(
    id: String,
    state: State<'_, AppState>,
) -> Result<bool> {
    println!("删除科目配置: {}", id);
    
    state.database.delete_subject_config(&id)
        .map_err(|e| AppError::database(format!("删除科目配置失败: {}", e)))
}

#[tauri::command]
pub async fn initialize_default_subject_configs(
    state: State<'_, AppState>,
) -> Result<()> {
    println!("初始化默认科目配置");
    
    state.database.initialize_default_subject_configs()
        .map_err(|e| AppError::database(format!("初始化默认科目配置失败: {}", e)))
}

// ============================================================================
// Batch Operations Commands
// ============================================================================

use crate::batch_operations::{BatchOperationExt, batch_utils};
use serde::{Serialize, Deserialize};

#[derive(Debug, Deserialize)]
pub struct BatchDeleteRequest {
    pub mistake_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BatchOperationResult {
    pub success: bool,
    pub processed_count: usize,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct BatchUpdateStatusRequest {
    pub updates: std::collections::HashMap<String, String>, // mistake_id -> new_status
}

#[derive(Debug, Deserialize)]
pub struct BatchUpdateTagsRequest {
    pub updates: std::collections::HashMap<String, Vec<String>>, // mistake_id -> new_tags
}

#[derive(Debug, Deserialize)]
pub struct BatchCleanupRequest {
    pub archive_days: Option<i64>, // If provided, archive mistakes older than this many days
}

#[derive(Debug, Serialize)]
pub struct BatchCleanupResult {
    pub orphaned_messages_cleaned: usize,
    pub mistakes_archived: usize,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct BatchExportRequest {
    pub mistake_ids: Vec<String>,
}

/// Batch delete multiple mistakes
#[tauri::command]
pub async fn batch_delete_mistakes(
    request: BatchDeleteRequest,
    state: State<'_, AppState>,
) -> Result<BatchOperationResult> {
    println!("批量删除错题: {} 个", request.mistake_ids.len());
    
    if request.mistake_ids.is_empty() {
        return Ok(BatchOperationResult {
            success: true,
            processed_count: 0,
            message: "没有需要删除的错题".to_string(),
        });
    }

    let deleted_count = state.database.with_batch_operations(|batch_ops| {
        batch_ops.batch_delete_mistakes(&request.mistake_ids)
    }).map_err(|e| AppError::database(format!("批量删除错题失败: {}", e)))?;

    Ok(BatchOperationResult {
        success: true,
        processed_count: deleted_count,
        message: format!("成功删除 {} 个错题", deleted_count),
    })
}

/// Batch update mistake statuses
#[tauri::command]
pub async fn batch_update_mistake_statuses(
    request: BatchUpdateStatusRequest,
    state: State<'_, AppState>,
) -> Result<BatchOperationResult> {
    println!("批量更新错题状态: {} 个", request.updates.len());
    
    if request.updates.is_empty() {
        return Ok(BatchOperationResult {
            success: true,
            processed_count: 0,
            message: "没有需要更新的错题状态".to_string(),
        });
    }

    let updated_count = state.database.with_batch_operations(|batch_ops| {
        batch_ops.batch_update_mistake_statuses(&request.updates)
    }).map_err(|e| AppError::database(format!("批量更新错题状态失败: {}", e)))?;

    Ok(BatchOperationResult {
        success: true,
        processed_count: updated_count,
        message: format!("成功更新 {} 个错题的状态", updated_count),
    })
}

/// Batch update mistake tags
#[tauri::command]
pub async fn batch_update_mistake_tags(
    request: BatchUpdateTagsRequest,
    state: State<'_, AppState>,
) -> Result<BatchOperationResult> {
    println!("批量更新错题标签: {} 个", request.updates.len());
    
    if request.updates.is_empty() {
        return Ok(BatchOperationResult {
            success: true,
            processed_count: 0,
            message: "没有需要更新的错题标签".to_string(),
        });
    }

    let updated_count = state.database.with_batch_operations(|batch_ops| {
        batch_ops.batch_update_mistake_tags(&request.updates)
    }).map_err(|e| AppError::database(format!("批量更新错题标签失败: {}", e)))?;

    Ok(BatchOperationResult {
        success: true,
        processed_count: updated_count,
        message: format!("成功更新 {} 个错题的标签", updated_count),
    })
}

/// Batch cleanup operations (orphaned messages, old mistakes)
#[tauri::command]
pub async fn batch_cleanup_database(
    request: BatchCleanupRequest,
    state: State<'_, AppState>,
) -> Result<BatchCleanupResult> {
    println!("批量清理数据库");

    let (orphaned_count, archived_count) = batch_utils::bulk_cleanup(
        &state.database,
        request.archive_days
    ).map_err(|e| AppError::database(format!("数据库清理失败: {}", e)))?;

    let message = if archived_count > 0 {
        format!(
            "清理完成：删除 {} 条孤立消息，归档 {} 个旧错题",
            orphaned_count, archived_count
        )
    } else {
        format!("清理完成：删除 {} 条孤立消息", orphaned_count)
    };

    Ok(BatchCleanupResult {
        orphaned_messages_cleaned: orphaned_count,
        mistakes_archived: archived_count,
        message,
    })
}

/// Batch export mistakes with full data
#[tauri::command]
pub async fn batch_export_mistakes(
    request: BatchExportRequest,
    state: State<'_, AppState>,
) -> Result<Vec<MistakeItem>> {
    println!("批量导出错题: {} 个", request.mistake_ids.len());
    
    if request.mistake_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mistakes = batch_utils::bulk_export_mistakes(
        &state.database,
        &request.mistake_ids
    ).map_err(|e| AppError::database(format!("批量导出错题失败: {}", e)))?;

    println!("批量导出完成: {} 个错题", mistakes.len());
    Ok(mistakes)
}

/// Batch save mistakes (useful for import operations)
#[tauri::command]
pub async fn batch_save_mistakes(
    mistakes: Vec<MistakeItem>,
    state: State<'_, AppState>,
) -> Result<BatchOperationResult> {
    println!("批量保存错题: {} 个", mistakes.len());
    
    if mistakes.is_empty() {
        return Ok(BatchOperationResult {
            success: true,
            processed_count: 0,
            message: "没有需要保存的错题".to_string(),
        });
    }

    let saved_count = batch_utils::bulk_import_mistakes(
        &state.database,
        &mistakes
    ).map_err(|e| AppError::database(format!("批量保存错题失败: {}", e)))?;

    Ok(BatchOperationResult {
        success: true,
        processed_count: saved_count,
        message: format!("成功保存 {} 个错题", saved_count),
    })
}

// ============================================================================
// Database Optimization Commands
// ============================================================================

use crate::database_optimizations::DatabaseOptimizationExt;

#[derive(Debug, Deserialize)]
pub struct OptimizedGetMistakesRequest {
    pub subject_filter: Option<String>,
    pub type_filter: Option<String>,
    pub tags_filter: Option<Vec<String>>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct FullTextSearchRequest {
    pub search_term: String,
    pub subject_filter: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct DateRangeRequest {
    pub start_date: String, // RFC3339 format
    pub end_date: String,   // RFC3339 format
    pub subject_filter: Option<String>,
}

/// Get mistakes using optimized queries with tag filtering
#[tauri::command]
pub async fn get_mistakes_optimized(
    request: OptimizedGetMistakesRequest,
    state: State<'_, AppState>,
) -> Result<Vec<MistakeItem>> {
    println!(
        "获取错题（优化版）: subject={:?}, type={:?}, tags={:?}, limit={:?}, offset={:?}",
        request.subject_filter, request.type_filter, request.tags_filter, request.limit, request.offset
    );

    let mistakes = state.database.get_mistakes_optimized(
        request.subject_filter.as_deref(),
        request.type_filter.as_deref(),
        request.tags_filter.as_deref().map(|v| v.as_ref()),
        request.limit,
        request.offset,
    ).map_err(|e| AppError::database(format!("获取错题失败: {}", e)))?;

    println!("获取错题（优化版）完成: {} 个", mistakes.len());
    Ok(mistakes)
}

/// Get tag statistics using optimized JSON queries
#[tauri::command]
pub async fn get_tag_statistics_optimized(
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<String, i32>> {
    println!("获取标签统计（优化版）");

    let stats = state.database.get_tag_statistics_optimized()
        .map_err(|e| AppError::database(format!("获取标签统计失败: {}", e)))?;

    println!("获取标签统计（优化版）完成: {} 个标签", stats.len());
    Ok(stats)
}

/// Full-text search across mistake content
#[tauri::command]
pub async fn search_mistakes_fulltext(
    request: FullTextSearchRequest,
    state: State<'_, AppState>,
) -> Result<Vec<MistakeItem>> {
    println!("全文搜索错题: '{}'", request.search_term);

    if request.search_term.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mistakes = state.database.search_mistakes_fulltext(
        &request.search_term,
        request.subject_filter.as_deref(),
        request.limit,
    ).map_err(|e| AppError::database(format!("全文搜索失败: {}", e)))?;

    println!("全文搜索完成: {} 个结果", mistakes.len());
    Ok(mistakes)
}

/// Get mistakes by date range
#[tauri::command]
pub async fn get_mistakes_by_date_range(
    request: DateRangeRequest,
    state: State<'_, AppState>,
) -> Result<Vec<MistakeItem>> {
    println!("按日期范围获取错题: {} 到 {}", request.start_date, request.end_date);

    let mistakes = state.database.get_mistakes_by_date_range(
        &request.start_date,
        &request.end_date,
        request.subject_filter.as_deref(),
    ).map_err(|e| AppError::database(format!("按日期范围获取错题失败: {}", e)))?;

    println!("按日期范围获取错题完成: {} 个", mistakes.len());
    Ok(mistakes)
}

/// Create performance indexes for better query speed
#[tauri::command]
pub async fn create_performance_indexes(
    state: State<'_, AppState>,
) -> Result<String> {
    println!("创建性能索引");

    state.database.create_performance_indexes()
        .map_err(|e| AppError::database(format!("创建性能索引失败: {}", e)))?;

    Ok("性能索引创建成功".to_string())
}

/// Analyze query performance
#[tauri::command]
pub async fn analyze_query_performance(
    query: String,
    state: State<'_, AppState>,
) -> Result<String> {
    println!("分析查询性能: {}", query);

    let analysis = state.database.analyze_query_performance(&query)
        .map_err(|e| AppError::database(format!("查询性能分析失败: {}", e)))?;

    Ok(analysis)
}

/// 从模型输出中提取思维链内容
fn extract_thinking_content_from_model_output(model_output: &crate::models::StandardModel2Output) -> Option<String> {
    match &model_output.chain_of_thought_details {
        Some(cot_details) => {
            // 尝试提取结构化的思维链内容
            if let Some(reasoning_content) = cot_details.get("reasoning_content") {
                if let Some(reasoning_str) = reasoning_content.as_str() {
                    if !reasoning_str.trim().is_empty() {
                        return Some(reasoning_str.to_string());
                    }
                }
            }
            
            // 如果有解析的段落，格式化为可读内容
            if let Some(parsed_sections) = cot_details.get("parsed_sections") {
                if let Some(sections_array) = parsed_sections.as_array() {
                    if !sections_array.is_empty() {
                        let formatted_sections = sections_array
                            .iter()
                            .filter_map(|section| {
                                let title = section.get("title")?.as_str()?;
                                let content = section.get("content")?.as_str()?;
                                if !content.trim().is_empty() {
                                    Some(format!("## {}\n{}", title, content))
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n\n");
                        
                        if !formatted_sections.trim().is_empty() {
                            return Some(formatted_sections);
                        }
                    }
                }
            }
            
            // 回退到完整响应
            if let Some(full_response) = cot_details.get("full_response") {
                if let Some(response_str) = full_response.as_str() {
                    // 如果启用了思维链但没有特殊内容，至少返回标记
                    if cot_details.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false) {
                        return Some(format!("## 完整推理过程\n{}", response_str));
                    }
                }
            }
            
            None
        }
        None => None,
    }
}

/// 获取默认的模型适配器选项
fn get_default_model_adapter_options() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "value": "general",
            "label": "通用模型",
            "description": "适用于大多数标准AI模型（如GPT、Claude、通义千问等）",
            "is_default": true,
            "capabilities": ["chat", "text_generation"],
            "supported_features": ["streaming", "multimodal"]
        }),
        serde_json::json!({
            "value": "deepseek-r1",
            "label": "DeepSeek-R1",
            "description": "专为DeepSeek-R1推理模型优化，支持思维链流式输出",
            "is_default": true,
            "capabilities": ["reasoning", "chain_of_thought", "problem_solving"],
            "supported_features": ["streaming", "thinking_chain", "reasoning_output"]
        }),
        serde_json::json!({
            "value": "google",
            "label": "Google Gemini",
            "description": "Google Gemini系列模型，支持多模态和高质量文本生成",
            "is_default": true,
            "capabilities": ["chat", "text_generation", "multimodal", "vision"],
            "supported_features": ["streaming", "multimodal", "image_analysis"]
        }),
        serde_json::json!({
            "value": "o1-series",
            "label": "OpenAI o1系列",
            "description": "OpenAI o1-preview和o1-mini等推理模型",
            "is_default": true,
            "capabilities": ["reasoning", "problem_solving", "scientific_analysis"],
            "supported_features": ["reasoning_tokens", "thinking_process"]
        }),
        serde_json::json!({
            "value": "claude-3-5-sonnet",
            "label": "Claude 3.5 Sonnet",
            "description": "Anthropic Claude 3.5 Sonnet高性能模型",
            "is_default": true,
            "capabilities": ["chat", "analysis", "coding", "multimodal"],
            "supported_features": ["streaming", "vision", "long_context"]
        })
    ]
}

// ============================================================================
// 回顾分析功能相关命令
// ============================================================================

/// 开始统一回顾分析 - 第一步：创建会话并缓存数据
#[tauri::command]
pub async fn start_consolidated_review_analysis(
    request: StartConsolidatedReviewAnalysisRequest,
    state: State<'_, AppState>,
) -> Result<StartConsolidatedReviewAnalysisResponse> {
    // 安全地截取中文字符串，使用字符边界而非字节边界
    let prompt_preview = if request.overall_prompt.chars().count() > 50 {
        request.overall_prompt.chars().take(50).collect::<String>() + "..."
    } else {
        request.overall_prompt.clone()
    };
    
    println!("🔄 开始统一回顾分析: 科目={}, 输入长度={}, 问题={}, 错题数量={}", 
        request.subject, 
        request.consolidated_input.len(),
        prompt_preview,
        request.mistake_ids.len()
    );
    println!("🔍 调试: 错题ID列表={:?}", request.mistake_ids);

    // 生成唯一的回顾会话ID
    let review_session_id = format!("review_{}", Uuid::new_v4());

    // 创建回顾会话数据
    let review_session = ConsolidatedReviewSession {
        review_session_id: review_session_id.clone(),
        subject: request.subject.clone(),
        consolidated_input: request.consolidated_input.clone(),
        overall_prompt: request.overall_prompt.clone(),
        enable_chain_of_thought: request.enable_chain_of_thought,
        created_at: Utc::now(),
        chat_history: Vec::new(),
        mistake_ids: request.mistake_ids.clone(), // 🎯 修复：添加错题ID信息
    };

    // 存储到临时会话中
    {
        let mut sessions = state.review_sessions.lock().await;
        sessions.insert(review_session_id.clone(), review_session.clone());
    }

    // 立即保存到数据库（创建初始记录）
    let initial_review_analysis = ReviewAnalysisItem {
        id: review_session_id.clone(),
        name: "回顾分析会话".to_string(), // 默认名称
        subject: request.subject.clone(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        mistake_ids: request.mistake_ids.clone(), // 🔧 修复：使用传入的错题ID列表
        consolidated_input: request.consolidated_input.clone(),
        user_question: request.overall_prompt.clone(),
        status: "pending".to_string(), // 待启动状态，等待流式分析
        tags: Vec::new(),
        analysis_type: "consolidated_review".to_string(),
        chat_history: Vec::new(),
    };

    match state.database.save_review_analysis(&initial_review_analysis) {
        Ok(_) => {
            println!("✅ 初始回顾分析记录已保存到数据库");
        }
        Err(e) => {
            println!("❌ 保存初始回顾分析记录失败: {}", e);
            // 返回错误，因为数据库保存失败意味着数据不一致
            return Err(AppError::database(format!("保存回顾分析失败: {}", e)));
        }
    }

    println!("✅ 回顾分析会话已创建: {}", review_session_id);

    Ok(StartConsolidatedReviewAnalysisResponse {
        review_session_id,
    })
}

/// 触发统一回顾分析流式处理 - 第二步：开始AI分析
#[tauri::command]
pub async fn trigger_consolidated_review_stream(
    request: TriggerConsolidatedReviewStreamRequest,
    window: Window,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("🚀 触发统一回顾分析流式处理: {}", request.review_session_id);

    // 从缓存中获取会话数据
    let session_data = {
        let sessions = state.review_sessions.lock().await;
        sessions.get(&request.review_session_id)
            .ok_or_else(|| AppError::not_found("回顾分析会话不存在"))?
            .clone()
    };

    // 获取回顾分析模型配置
    let model_assignments = state.llm_manager.get_model_assignments().await?;
    let review_model_config_id = model_assignments.review_analysis_model_config_id
        .ok_or_else(|| AppError::configuration("未配置回顾分析模型"))?;

    let api_configs = state.llm_manager.get_api_configs().await?;
    let review_model_config = api_configs.iter()
        .find(|config| config.id == review_model_config_id)
        .ok_or_else(|| AppError::configuration("找不到回顾分析模型配置"))?;

    // 获取回顾分析提示词
    let consolidated_review_prompt = state.llm_manager
        .get_subject_prompt(&session_data.subject, "consolidated_review");

    // 构造LLM请求消息
    let system_message = consolidated_review_prompt.replace("{subject}", &session_data.subject);
    let user_message = format!(
        "{}\n\n---\n请基于以上所有错题信息，针对以下问题进行分析：\n{}",
        session_data.consolidated_input,
        session_data.overall_prompt
    );

    // 构造聊天历史（开始新对话）
    let mut chat_history = session_data.chat_history.clone();
    chat_history.push(ChatMessage {
        role: "user".to_string(),
        content: user_message,
        timestamp: Utc::now(),
        thinking_content: None,
        rag_sources: None,
        image_paths: None,
        image_base64: None,
    });

    // 准备流式事件名称
    let stream_event = format!("review_analysis_stream_{}", request.review_session_id);
    let _thinking_stream_event = format!("review_analysis_stream_{}_reasoning", request.review_session_id);

    println!("📡 开始调用回顾分析模型: {}", review_model_config.name);

    // 调用LLM进行流式分析
    let context = std::collections::HashMap::from([
        ("system_prompt".to_string(), serde_json::Value::String(system_message)),
        ("task_type".to_string(), serde_json::Value::String("consolidated_review".to_string())),
    ]);

    let _result = state.llm_manager.call_unified_model_2_stream(
        &context,
        &chat_history,
        &session_data.subject,
        request.enable_chain_of_thought,
        None, // 不需要图片
        Some("consolidated_review"),
        window,
        &stream_event,
    ).await?;

    println!("✅ 回顾分析流式处理已启动");

    Ok(())
}

/// 继续统一回顾分析对话
#[tauri::command] 
pub async fn continue_consolidated_review_stream(
    request: ContinueConsolidatedReviewStreamRequest,
    window: Window,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("💬 继续统一回顾分析对话: {}", request.review_session_id);

    // 获取会话数据（支持从数据库恢复）
    let session_data = {
        let mut sessions = state.review_sessions.lock().await;
        
        // 首先尝试从内存获取
        if let Some(session) = sessions.get(&request.review_session_id) {
            session.clone()
        } else {
            // 如果内存中没有，尝试从数据库恢复
            println!("🔄 内存中没有找到会话，尝试从数据库恢复: {}", request.review_session_id);
            
            match state.database.get_review_analysis_by_id(&request.review_session_id) {
                Ok(Some(review_analysis)) => {
                    // 从数据库记录重建会话数据
                    let restored_session = ConsolidatedReviewSession {
                        review_session_id: review_analysis.id.clone(),
                        subject: review_analysis.subject.clone(),
                        consolidated_input: review_analysis.consolidated_input.clone(),
                        overall_prompt: review_analysis.user_question.clone(),
                        enable_chain_of_thought: true, // 默认启用思维链
                        created_at: review_analysis.created_at,
                        chat_history: review_analysis.chat_history.clone(),
                        mistake_ids: review_analysis.mistake_ids.clone(), // 🎯 修复：保留错题ID信息
                    };
                    
                    // 将恢复的会话存回内存缓存
                    sessions.insert(request.review_session_id.clone(), restored_session.clone());
                    println!("✅ 从数据库成功恢复会话: {}", request.review_session_id);
                    
                    restored_session
                },
                Ok(None) => {
                    return Err(AppError::not_found("回顾分析会话不存在"));
                },
                Err(e) => {
                    println!("❌ 从数据库加载会话失败: {}", e);
                    return Err(AppError::database(format!("加载会话失败: {}", e)));
                }
            }
        }
    };

    // 更新会话聊天历史
    {
        let mut sessions = state.review_sessions.lock().await;
        if let Some(session) = sessions.get_mut(&request.review_session_id) {
            session.chat_history = request.chat_history.clone();
        }
    }

    // 获取回顾分析模型配置
    let model_assignments = state.llm_manager.get_model_assignments().await?;
    let _review_model_config_id = model_assignments.review_analysis_model_config_id
        .ok_or_else(|| AppError::configuration("未配置回顾分析模型"))?;

    // 获取回顾分析提示词
    let consolidated_review_prompt = state.llm_manager
        .get_subject_prompt(&session_data.subject, "consolidated_review");

    let system_message = consolidated_review_prompt.replace("{subject}", &session_data.subject);

    // 准备流式事件名称 - 🎯 修复：追问使用不同的事件名称，参考错题分析实现
    let stream_event = format!("review_chat_stream_{}", request.review_session_id);

    println!("📡 继续回顾分析对话，消息数量: {}", request.chat_history.len());
    println!("🔍 [追问调试] 详细信息:");
    println!("  - review_id: {}", request.review_session_id);
    println!("  - stream_event: {}", stream_event);
    println!("  - subject: {}", session_data.subject);
    println!("  - enable_chain_of_thought: {}", request.enable_chain_of_thought);
    println!("  - chat_history_length: {}", request.chat_history.len());
    println!("  - system_message: {}", system_message);
    
    // 输出聊天历史的详细信息
    for (i, msg) in request.chat_history.iter().enumerate() {
        println!("  - chat[{}]: {} - {}", i, msg.role, 
                if msg.content.chars().count() > 100 { 
                    format!("{}...", msg.content.chars().take(100).collect::<String>()) 
                } else { 
                    msg.content.clone() 
                });
    }

    // 调用LLM进行流式对话
    let context = std::collections::HashMap::from([
        ("system_prompt".to_string(), serde_json::Value::String(system_message)),
        ("task_type".to_string(), serde_json::Value::String("consolidated_review_chat".to_string())),
    ]);

    // 🎯 修复：获取相关错题的图片信息
    let mut all_image_paths = Vec::new();
    for mistake_id in &session_data.mistake_ids {
        if let Ok(Some(mistake)) = state.database.get_mistake_by_id(mistake_id) {
            all_image_paths.extend(mistake.question_images);
        }
    }
    
    // 🎯 修复：获取模型配置，为多模态模型传递图片信息
    let model_config = state.llm_manager.get_model2_config().await?;
    let image_paths = if model_config.is_multimodal && !all_image_paths.is_empty() {
        Some(all_image_paths)
    } else {
        None
    };

    println!("🚀 [追问调试] 即将调用 call_unified_model_2_stream");
    println!("📸 [图片调试] 错题数量: {}, 图片路径数量: {}", session_data.mistake_ids.len(), image_paths.as_ref().map(|p| p.len()).unwrap_or(0));
    
    let result = state.llm_manager.call_unified_model_2_stream(
        &context,
        &request.chat_history,
        &session_data.subject,
        request.enable_chain_of_thought,
        image_paths, // 🎯 修复：传递相关错题的图片信息
        Some("consolidated_review_chat"),
        window.clone(),
        &stream_event,
    ).await?;
    println!("✅ [追问调试] call_unified_model_2_stream 调用完成，返回内容长度: {}", result.assistant_message.len());

    // 流式处理完成后，像错题分析一样自动保存到数据库
    {
        // 从内存缓存获取session信息
        let session_data = {
            let sessions = state.review_sessions.lock().await;
            sessions.get(&request.review_session_id).cloned()
        };

        if let Some(session) = session_data {
            // 构建更新后的聊天历史
            let mut updated_chat_history = request.chat_history.clone();
            updated_chat_history.push(ChatMessage {
                role: "assistant".to_string(),
                content: result.assistant_message.clone(),
                timestamp: Utc::now(),
                thinking_content: extract_thinking_content_from_model_output(&result),
                rag_sources: None,
                image_paths: None,
                image_base64: None,
            });

            // 创建ReviewAnalysisItem并保存到数据库（复用错题分析模式）
            let review_analysis = ReviewAnalysisItem {
                id: request.review_session_id.clone(),
                name: format!("回顾分析-{}", Utc::now().format("%Y%m%d")), // ConsolidatedReviewSession没有name字段
                subject: session.subject.clone(),
                created_at: session.created_at,
                updated_at: Utc::now(),
                mistake_ids: session.mistake_ids.clone(), // 🎯 修复：使用实际的错题ID列表
                consolidated_input: session.consolidated_input.clone(),
                user_question: "统一回顾分析".to_string(),
                status: "completed".to_string(),
                tags: vec![],
                analysis_type: "consolidated_review".to_string(),
                chat_history: updated_chat_history.clone(),
            };

            // 保存到数据库（自动保存，像错题分析一样）
            if let Err(e) = state.database.save_review_analysis(&review_analysis) {
                println!("❌ 自动保存回顾分析到数据库失败: {}", e);
            } else {
                println!("✅ 自动保存回顾分析到数据库成功");
            }

            // 同时更新内存缓存
            {
                let mut sessions = state.review_sessions.lock().await;
                if let Some(cached_session) = sessions.get_mut(&request.review_session_id) {
                    cached_session.chat_history = updated_chat_history;
                }
            }
        }
    }

    println!("✅ 回顾分析对话继续处理完成");

    Ok(())
}

/// 获取回顾分析数据（从数据库加载）- 复用错题分析的加载模式
#[tauri::command] 
pub async fn get_review_analysis_by_id(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::models::ReviewAnalysisItem>> {
    println!("🔍 从数据库获取回顾分析: {}", id);
    
    match state.database.get_review_analysis_by_id(&id) {
        Ok(review) => {
            if let Some(ref r) = review {
                println!("✅ 成功获取回顾分析，聊天记录数量: {}", r.chat_history.len());
            } else {
                println!("⚠️ 未找到回顾分析: {}", id);
            }
            Ok(review)
        }
        Err(e) => {
            println!("❌ 获取回顾分析失败: {}", e);
            Err(AppError::database(format!("获取回顾分析失败: {}", e)))
        }
    }
}

/// 获取统一回顾分析会话数据（兼容旧接口）
#[tauri::command]
pub async fn get_consolidated_review_session(
    sessionId: String,
    state: State<'_, AppState>,
) -> Result<Option<ConsolidatedReviewSession>> {
    println!("🔍 获取统一回顾分析会话: {}", sessionId);
    
    // 从缓存中获取会话数据
    let sessions = state.review_sessions.lock().await;
    let session = sessions.get(&sessionId).cloned();
    
    Ok(session)
}

/// 生成ANKI卡片
#[tauri::command]
pub async fn generate_anki_cards_from_document(
    request: AnkiDocumentGenerationRequest,
    state: State<'_, AppState>,
) -> Result<AnkiDocumentGenerationResponse> {
    println!("🎯 开始生成ANKI卡片: 科目={}, 文档长度={}", request.subject_name, request.document_content.len());
    
    // 调用LLM Manager的ANKI制卡功能
    match state.llm_manager.generate_anki_cards_from_document(
        &request.document_content,
        &request.subject_name,
        request.options.as_ref(),
    ).await {
        Ok(cards) => {
            println!("✅ ANKI卡片生成成功: {} 张卡片", cards.len());
            Ok(AnkiDocumentGenerationResponse {
                success: true,
                cards,
                error_message: None,
            })
        }
        Err(e) => {
            println!("❌ ANKI卡片生成失败: {}", e);
            Ok(AnkiDocumentGenerationResponse {
                success: false,
                cards: vec![],
                error_message: Some(e.to_string()),
            })
        }
    }
}

/// 从DOCX/PDF文档文件生成ANKI卡片
#[tauri::command]
pub async fn generate_anki_cards_from_document_file(
    file_path: String,
    subject_name: String,
    options: Option<AnkiGenerationOptions>,
    state: State<'_, AppState>,
) -> Result<AnkiDocumentGenerationResponse> {
    println!("🎯 开始从文档文件生成ANKI卡片: 文件={}, 科目={}", file_path, subject_name);
    
    // 1. 首先解析文档内容
    let document_content = match parse_document_from_path(file_path.clone()).await {
        Ok(content) => content,
        Err(e) => {
            println!("❌ 文档解析失败: {}", e);
            return Ok(AnkiDocumentGenerationResponse {
                success: false,
                cards: vec![],
                error_message: Some(format!("文档解析失败: {}", e)),
            });
        }
    };
    
    println!("✅ 文档解析成功，提取文本长度: {}", document_content.len());
    
    // 2. 调用ANKI卡片生成
    match state.llm_manager.generate_anki_cards_from_document(
        &document_content,
        &subject_name,
        options.as_ref(),
    ).await {
        Ok(cards) => {
            println!("✅ ANKI卡片生成成功: {} 张卡片", cards.len());
            Ok(AnkiDocumentGenerationResponse {
                success: true,
                cards,
                error_message: None,
            })
        }
        Err(e) => {
            println!("❌ ANKI卡片生成失败: {}", e);
            Ok(AnkiDocumentGenerationResponse {
                success: false,
                cards: vec![],
                error_message: Some(e.to_string()),
            })
        }
    }
}

/// 从Base64编码的DOCX/PDF文档生成ANKI卡片
#[tauri::command]
pub async fn generate_anki_cards_from_document_base64(
    file_name: String,
    base64_content: String,
    subject_name: String,
    options: Option<AnkiGenerationOptions>,
    state: State<'_, AppState>,
) -> Result<AnkiDocumentGenerationResponse> {
    println!("🎯 开始从Base64文档生成ANKI卡片: 文件={}, 科目={}", file_name, subject_name);
    
    // 1. 首先解析文档内容
    let document_content = match parse_document_from_base64(file_name.clone(), base64_content).await {
        Ok(content) => content,
        Err(e) => {
            println!("❌ 文档解析失败: {}", e);
            return Ok(AnkiDocumentGenerationResponse {
                success: false,
                cards: vec![],
                error_message: Some(format!("文档解析失败: {}", e)),
            });
        }
    };
    
    println!("✅ 文档解析成功，提取文本长度: {}", document_content.len());
    
    // 2. 调用ANKI卡片生成
    match state.llm_manager.generate_anki_cards_from_document(
        &document_content,
        &subject_name,
        options.as_ref(),
    ).await {
        Ok(cards) => {
            println!("✅ ANKI卡片生成成功: {} 张卡片", cards.len());
            Ok(AnkiDocumentGenerationResponse {
                success: true,
                cards,
                error_message: None,
            })
        }
        Err(e) => {
            println!("❌ ANKI卡片生成失败: {}", e);
            Ok(AnkiDocumentGenerationResponse {
                success: false,
                cards: vec![],
                error_message: Some(e.to_string()),
            })
        }
    }
}

// ==================== AnkiConnect集成功能 ====================

/// 检查AnkiConnect连接状态
#[tauri::command]
pub async fn check_anki_connect_status() -> Result<bool> {
    match crate::anki_connect_service::check_anki_connect_availability().await {
        Ok(available) => Ok(available),
        Err(e) => Err(AppError::validation(e)),
    }
}

/// 获取所有牌组名称
#[tauri::command]
pub async fn get_anki_deck_names() -> Result<Vec<String>> {
    match crate::anki_connect_service::get_deck_names().await {
        Ok(deck_names) => Ok(deck_names),
        Err(e) => Err(AppError::validation(e)),
    }
}

/// 获取所有笔记类型名称
#[tauri::command]
pub async fn get_anki_model_names() -> Result<Vec<String>> {
    match crate::anki_connect_service::get_model_names().await {
        Ok(model_names) => Ok(model_names),
        Err(e) => Err(AppError::validation(e)),
    }
}

/// 创建牌组（如果不存在）
#[tauri::command]
pub async fn create_anki_deck(deck_name: String) -> Result<()> {
    match crate::anki_connect_service::create_deck_if_not_exists(&deck_name).await {
        Ok(_) => Ok(()),
        Err(e) => Err(AppError::validation(e)),
    }
}

/// 将选定的卡片添加到AnkiConnect
#[tauri::command]
pub async fn add_cards_to_anki_connect(
    selected_cards: Vec<crate::models::AnkiCard>,
    deck_name: String,
    mut note_type: String,
) -> Result<Vec<Option<u64>>> {
    if selected_cards.is_empty() {
        return Err(AppError::validation("没有选择任何卡片".to_string()));
    }

    if deck_name.trim().is_empty() {
        return Err(AppError::validation("牌组名称不能为空".to_string()));
    }

    if note_type.trim().is_empty() {
        return Err(AppError::validation("笔记类型不能为空".to_string()));
    }

    // 检查是否为填空题
    let is_cloze = selected_cards.iter().any(|card| {
        let text_content = card.text.as_deref().unwrap_or("");
        text_content.contains("{{c") && text_content.contains("}}")
    });

    if is_cloze {
        println!("🔍 检测到填空题，开始验证笔记类型...");
        
        // 检查Anki中是否存在名为"Cloze"的笔记类型
        let model_names = crate::anki_connect_service::get_model_names().await
            .map_err(|e| AppError::validation(format!("获取Anki笔记类型失败: {}", e)))?;
        
        if !model_names.iter().any(|name| name == "Cloze") {
            return Err(AppError::validation(
                "Anki中缺少标准的'Cloze'笔记类型，请在Anki中手动添加一个。".to_string()
            ));
        }

        // 如果用户选择的不是"Cloze"，但又是填空题，则强制使用"Cloze"
        if note_type != "Cloze" {
            println!("⚠️ 用户选择了非标准的填空题笔记类型 '{}'，将强制使用 'Cloze'。", note_type);
            note_type = "Cloze".to_string();
        }
    }

    println!("📤 开始添加 {} 张卡片到Anki牌组: {} (笔记类型: {})", selected_cards.len(), deck_name, note_type);

    // 首先尝试创建牌组（如果不存在）
    if let Err(e) = crate::anki_connect_service::create_deck_if_not_exists(&deck_name).await {
        println!("⚠️ 创建牌组失败（可能已存在）: {}", e);
    }

    match crate::anki_connect_service::add_notes_to_anki(selected_cards, deck_name, note_type).await {
        Ok(note_ids) => {
            let successful_count = note_ids.iter().filter(|id| id.is_some()).count();
            let failed_count = note_ids.len() - successful_count;
            
            println!("✅ 卡片添加完成: 成功 {} 张, 失败 {} 张", successful_count, failed_count);
            
            if failed_count > 0 {
                println!("⚠️ 部分卡片添加失败，可能是重复卡片或格式错误");
            }
            
            Ok(note_ids)
        }
        Err(e) => {
            println!("❌ 添加卡片到Anki失败: {}", e);
            Err(AppError::validation(e))
        }
    }
}

/// 导出选定的卡片为.apkg文件
#[tauri::command]
pub async fn export_cards_as_apkg(
    selected_cards: Vec<crate::models::AnkiCard>,
    deck_name: String,
    note_type: String,
) -> Result<String> {
    export_cards_as_apkg_with_template(selected_cards, deck_name, note_type, None).await
}

/// 导出选定的卡片为.apkg文件（支持模板）
#[tauri::command]
pub async fn export_cards_as_apkg_with_template(
    selected_cards: Vec<crate::models::AnkiCard>,
    deck_name: String,
    mut note_type: String,
    template_id: Option<String>,
) -> Result<String> {
    if selected_cards.is_empty() {
        return Err(AppError::validation("没有选择任何卡片".to_string()));
    }

    // 获取模板配置
    let template_config = if let Some(template_id) = template_id {
        Some(get_template_config(&template_id).map_err(|e| AppError::validation(e))?)
    } else {
        None
    };

    if deck_name.trim().is_empty() {
        return Err(AppError::validation("牌组名称不能为空".to_string()));
    }

    if note_type.trim().is_empty() {
        return Err(AppError::validation("笔记类型不能为空".to_string()));
    }

    // 检查是否为填空题
    let is_cloze = selected_cards.iter().any(|card| {
        let text_content = card.text.as_deref().unwrap_or("");
        text_content.contains("{{c") && text_content.contains("}}")
    });

    if is_cloze && note_type != "Cloze" {
        println!("⚠️ 检测到填空题，但笔记类型不是 'Cloze'。导出时将强制使用 'Cloze' 类型。");
        note_type = "Cloze".to_string();
    }

    println!("📦 开始导出 {} 张卡片为.apkg文件 (笔记类型: {})", selected_cards.len(), note_type);

    // 生成默认文件名和路径
    let home_dir = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let output_path = std::path::PathBuf::from(home_dir)
        .join("Downloads")
        .join(format!("{}.apkg", deck_name.replace("/", "_").replace("\\", "_")));

    println!("📁 导出路径: {:?}", output_path);

    match crate::apkg_exporter_service::export_cards_to_apkg_with_template(
        selected_cards,
        deck_name,
        note_type,
        output_path.clone(),
        template_config
    ).await {
        Ok(_) => {
            println!("✅ .apkg文件导出成功: {:?}", output_path);
            Ok(format!("成功导出到: {}", output_path.display()))
        }
        Err(e) => {
            println!("❌ .apkg文件导出失败: {}", e);
            Err(AppError::validation(e))
        }
    }
}

// =================== Enhanced ANKI Commands ===================

/// 开始文档处理 - 增强版ANKI制卡
#[tauri::command]
pub async fn start_enhanced_document_processing(
    document_content: String,
    original_document_name: String,
    subject_name: Option<String>,
    options: AnkiGenerationOptions,
    window: Window,
    state: State<'_, AppState>,
) -> Result<String> {
    // 使用默认科目名称如果未提供
    let actual_subject_name = subject_name.unwrap_or_else(|| "通用学习材料".to_string());
    println!("🚀 开始增强文档处理: 科目={}, 文档名={}, 内容长度={}", 
        actual_subject_name, original_document_name, document_content.len());
    
    // 创建增强ANKI服务实例
    let enhanced_service = crate::enhanced_anki_service::EnhancedAnkiService::new(
        state.database.clone(),
        state.llm_manager.clone(),
    );
    
    // 构建请求
    let request = AnkiDocumentGenerationRequest {
        document_content,
        subject_name: actual_subject_name,
        options: Some(options),
    };
    
    // 开始处理
    let document_id = enhanced_service.start_document_processing(request, window).await?;
    
    println!("✅ 文档处理已启动: {}", document_id);
    Ok(document_id)
}

/// 手动触发任务处理
#[tauri::command]
pub async fn trigger_task_processing(
    task_id: String,
    window: Window,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("🎯 触发任务处理: {}", task_id);
    
    let enhanced_service = crate::enhanced_anki_service::EnhancedAnkiService::new(
        state.database.clone(),
        state.llm_manager.clone(),
    );
    
    enhanced_service.trigger_task_processing(task_id, window).await?;
    Ok(())
}


/// 获取文档的所有任务
#[tauri::command]
pub async fn get_document_tasks(
    documentId: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::DocumentTask>> {
    println!("📋 获取文档任务列表: {}", documentId);
    
    let enhanced_service = crate::enhanced_anki_service::EnhancedAnkiService::new(
        state.database.clone(),
        state.llm_manager.clone(),
    );
    
    let tasks = enhanced_service.get_document_tasks(documentId)?;
    println!("✅ 找到 {} 个任务", tasks.len());
    Ok(tasks)
}

/// 获取任务的所有卡片
#[tauri::command]
pub async fn get_task_cards(
    task_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::AnkiCard>> {
    println!("🃏 获取任务卡片: {}", task_id);
    
    let enhanced_service = crate::enhanced_anki_service::EnhancedAnkiService::new(
        state.database.clone(),
        state.llm_manager.clone(),
    );
    
    let cards = enhanced_service.get_task_cards(task_id)?;
    println!("✅ 找到 {} 张卡片", cards.len());
    Ok(cards)
}

/// 更新ANKI卡片
#[tauri::command]
pub async fn update_anki_card(
    card: crate::models::AnkiCard,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("✏️ 更新ANKI卡片: {}", card.id);
    
    // 验证卡片数据
    if card.front.trim().is_empty() {
        return Err(AppError::validation("卡片正面不能为空"));
    }
    if card.back.trim().is_empty() {
        return Err(AppError::validation("卡片背面不能为空"));
    }
    
    let enhanced_service = crate::enhanced_anki_service::EnhancedAnkiService::new(
        state.database.clone(),
        state.llm_manager.clone(),
    );
    
    enhanced_service.update_anki_card(card)?;
    println!("✅ 卡片更新成功");
    Ok(())
}

/// 删除ANKI卡片
#[tauri::command]
pub async fn delete_anki_card(
    card_id: String,
    state: State<'_, AppState>,
) -> Result<bool> {
    println!("🗑️ 删除ANKI卡片: {}", card_id);
    
    if card_id.is_empty() {
        return Err(AppError::validation("卡片ID不能为空"));
    }
    
    let enhanced_service = crate::enhanced_anki_service::EnhancedAnkiService::new(
        state.database.clone(),
        state.llm_manager.clone(),
    );
    
    enhanced_service.delete_anki_card(card_id)?;
    println!("✅ 卡片删除成功");
    Ok(true)
}

/// 删除文档任务及其所有卡片
#[tauri::command]
pub async fn delete_document_task(
    task_id: String,
    state: State<'_, AppState>,
) -> Result<bool> {
    println!("🗑️ 删除文档任务: {}", task_id);
    
    if task_id.is_empty() {
        return Err(AppError::validation("任务ID不能为空"));
    }
    
    let enhanced_service = crate::enhanced_anki_service::EnhancedAnkiService::new(
        state.database.clone(),
        state.llm_manager.clone(),
    );
    
    enhanced_service.delete_document_task(task_id)?;
    println!("✅ 任务删除成功");
    Ok(true)
}

/// 删除整个文档会话（所有任务和卡片）
#[tauri::command]
pub async fn delete_document_session(
    documentId: String,
    state: State<'_, AppState>,
) -> Result<bool> {
    println!("🗑️ 删除文档会话: {}", documentId);
    
    if documentId.is_empty() {
        return Err(AppError::validation("文档ID不能为空"));
    }
    
    let enhanced_service = crate::enhanced_anki_service::EnhancedAnkiService::new(
        state.database.clone(),
        state.llm_manager.clone(),
    );
    
    enhanced_service.delete_document_session(documentId)?;
    println!("✅ 文档会话删除成功");
    Ok(true)
}

/// 导出选定内容为APKG文件
#[tauri::command]
pub async fn export_apkg_for_selection(
    documentId: Option<String>,
    taskIds: Option<Vec<String>>,
    cardIds: Option<Vec<String>>,
    options: AnkiGenerationOptions,
    state: State<'_, AppState>,
) -> Result<String> {
    println!("📦 导出选定内容为APKG文件");
    
    // 验证至少选择了一种导出内容
    if documentId.is_none() && taskIds.is_none() && cardIds.is_none() {
        return Err(AppError::validation("必须选择要导出的内容（文档、任务或卡片）"));
    }
    
    let enhanced_service = crate::enhanced_anki_service::EnhancedAnkiService::new(
        state.database.clone(),
        state.llm_manager.clone(),
    );
    
    let export_path = enhanced_service.export_apkg_for_selection(
        documentId,
        taskIds,
        cardIds,
        options,
    ).await?;
    
    println!("✅ APKG文件导出成功: {}", export_path);
    Ok(export_path)
}

/// 获取文档的所有卡片（用于导出预览）
#[tauri::command]
pub async fn get_document_cards(
    documentId: String,
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::AnkiCard>> {
    println!("📋 获取文档的所有卡片: {}", documentId);
    
    let cards = state.database.get_cards_for_document(&documentId)
        .map_err(|e| AppError::database(format!("获取文档卡片失败: {}", e)))?;
    
    println!("✅ 找到 {} 张卡片", cards.len());
    Ok(cards)
}

// ==================== RAG知识库管理命令 ====================

use crate::models::{
    KnowledgeBaseStatusPayload, DocumentUploadRequest,
    RagEnhancedAnalysisRequest, RagEnhancedChatRequest
};

/// 添加文档到知识库
#[tauri::command]
pub async fn rag_add_documents(
    request: DocumentUploadRequest,
    window: Window,
    state: State<'_, AppState>,
) -> Result<String> {
    println!("📚 添加文档到知识库: {} 个文件", request.file_paths.len());
    
    // 立即读取文件并转换为内容模式，避免路径失效问题
    let mut document_contents = Vec::new();
    
    for (index, file_path) in request.file_paths.iter().enumerate() {
        println!("📄 文件 {}: {}", index + 1, file_path);
        
        // 尝试规范化路径
        let normalized_path = match std::path::Path::new(file_path).canonicalize() {
            Ok(canonical) => {
                let canonical_str = canonical.display().to_string();
                println!("🔧 规范化路径: {} -> {}", file_path, canonical_str);
                canonical_str
            }
            Err(e) => {
                println!("⚠️ 无法规范化路径 {}: {}，使用原路径", file_path, e);
                file_path.clone()
            }
        };
        
        let path = std::path::Path::new(&normalized_path);
        
        // 检查文件是否存在
        if !path.exists() {
            println!("❌ 文件不存在: {} (原路径: {})", normalized_path, file_path);
            return Err(AppError::file_system(format!("文件不存在: {} (原路径: {})", normalized_path, file_path)));
        }
        
        if !path.is_file() {
            println!("❌ 路径不是文件: {} (原路径: {})", normalized_path, file_path);
            return Err(AppError::file_system(format!("指定路径不是文件: {} (原路径: {})", normalized_path, file_path)));
        }
        
        // 立即读取文件内容，避免后续路径失效
        let file_name = path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
        
        println!("📚 立即读取文件内容: {}", file_name);
        
        let content = match path.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_lowercase().as_str() {
            "txt" | "md" | "markdown" => {
                // 文本文件直接读取
                match std::fs::read_to_string(&normalized_path) {
                    Ok(content) => content,
                    Err(e) => {
                        println!("❌ 读取文本文件失败: {} - {}", file_name, e);
                        return Err(AppError::file_system(format!("读取文本文件失败: {} - {}", file_name, e)));
                    }
                }
            }
            "pdf" | "docx" => {
                // 二进制文件读取为base64
                match std::fs::read(&normalized_path) {
                    Ok(bytes) => {
                        use base64::{Engine as _, engine::general_purpose};
                        general_purpose::STANDARD.encode(&bytes)
                    }
                    Err(e) => {
                        println!("❌ 读取二进制文件失败: {} - {}", file_name, e);
                        return Err(AppError::file_system(format!("读取二进制文件失败: {} - {}", file_name, e)));
                    }
                }
            }
            _ => {
                println!("❌ 不支持的文件类型: {}", file_name);
                return Err(AppError::validation(format!("不支持的文件类型: {}", file_name)));
            }
        };
        
        println!("✅ 文件内容读取成功: {} ({} 字符)", file_name, content.len());
        
        document_contents.push(serde_json::json!({
            "fileName": file_name,
            "content": content
        }));
    }
    
    if document_contents.is_empty() {
        return Err(AppError::validation("文件内容列表不能为空"));
    }
    
    // 使用内容模式添加文档，避免路径依赖
    let result = state.rag_manager.add_documents_from_content(document_contents, window).await?;
    
    println!("✅ 文档添加完成");
    Ok(result)
}

/// 从文件内容添加文档到知识库
#[tauri::command]
pub async fn rag_add_documents_from_content(
    documents: Vec<serde_json::Value>,
    window: Window,
    state: State<'_, AppState>,
) -> Result<String> {
    println!("📚 从内容添加文档到知识库: {} 个文件", documents.len());
    
    if documents.is_empty() {
        return Err(AppError::validation("文档列表不能为空"));
    }
    
    let result = state.rag_manager.add_documents_from_content(documents, window).await?;
    
    println!("✅ 从内容添加文档完成");
    Ok(result)
}

/// 获取知识库状态
#[tauri::command]
pub async fn rag_get_knowledge_base_status(
    state: State<'_, AppState>,
) -> Result<KnowledgeBaseStatusPayload> {
    println!("📊 获取知识库状态");
    
    let status = state.rag_manager.get_knowledge_base_status().await?;
    
    println!("✅ 知识库状态获取完成: {} 个文档, {} 个块", status.total_documents, status.total_chunks);
    Ok(status)
}

/// 删除知识库中的文档
#[tauri::command]
pub async fn rag_delete_document(
    documentId: String,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("🗑️ 删除知识库文档: {}", documentId);
    
    if documentId.is_empty() {
        return Err(AppError::validation("文档ID不能为空"));
    }
    
    state.rag_manager.delete_document_from_knowledge_base(&documentId).await?;
    
    println!("✅ 文档删除完成");
    Ok(())
}

/// 查询知识库
#[tauri::command]
pub async fn rag_query_knowledge_base(
    query: String,
    options: RagQueryOptions,
    state: State<'_, AppState>,
) -> Result<RagQueryResponse> {
    println!("🔍 查询知识库: '{}'", query);
    
    if query.trim().is_empty() {
        return Err(AppError::validation("查询字符串不能为空"));
    }
    
    let response = state.rag_manager.query_knowledge_base(&query, options).await?;
    
    println!("✅ 知识库查询完成: {} 个结果", response.retrieved_chunks.len());
    Ok(response)
}

/// 获取所有文档列表
#[tauri::command]
pub async fn rag_get_all_documents(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>> {
    println!("📋 获取所有文档列表");
    
    let documents = state.rag_manager.get_all_documents().await?;
    
    println!("✅ 文档列表获取完成: {} 个文档", documents.len());
    Ok(documents)
}

/// 清空知识库
#[tauri::command]
pub async fn rag_clear_knowledge_base(
    state: State<'_, AppState>,
) -> Result<()> {
    println!("🧹 清空知识库");
    
    state.rag_manager.clear_knowledge_base().await?;
    
    println!("✅ 知识库清空完成");
    Ok(())
}

// ==================== RAG增强的AI分析命令 ====================

/// RAG增强的流式分析
#[tauri::command]
pub async fn start_rag_enhanced_streaming_answer(
    request: RagEnhancedAnalysisRequest,
    window: Window,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("🚀 启动RAG增强的流式AI解答: {}, RAG: {:?}", 
        request.temp_id, request.enable_rag);
    
    // 获取临时会话
    let temp_session = {
        let sessions = state.temp_sessions.lock().await;
        sessions.get(&request.temp_id).cloned()
    };
    
    let session = temp_session.ok_or("临时会话不存在")?;
    
    // 如果启用RAG，先进行知识库查询
    let retrieved_context = if request.enable_rag.unwrap_or(false) {
        let rag_options = request.rag_options.unwrap_or(RagQueryOptions {
            top_k: 5,
            enable_reranking: Some(false),
        });
        
        // 构建查询字符串（结合用户问题和OCR内容）
        let query = format!("{} {}", session.user_question, session.ocr_text);
        
        println!("🔍 RAG查询: '{}'", query);
        let rag_response = state.rag_manager.query_knowledge_base(&query, rag_options).await?;
        rag_response.retrieved_chunks
    } else {
        Vec::new()
    };
    
    // 启动RAG增强的流式AI解答
    let stream_event = format!("analysis_stream_{}", request.temp_id);
    let model2_result = if !retrieved_context.is_empty() {
        // 构建完整的分析上下文
        let mut analysis_context = std::collections::HashMap::new();
        analysis_context.insert("user_question".to_string(), serde_json::Value::String(session.user_question.clone()));
        analysis_context.insert("ocr_text".to_string(), serde_json::Value::String(session.ocr_text.clone()));
        analysis_context.insert("tags".to_string(), serde_json::Value::Array(
            session.tags.iter().map(|t| serde_json::Value::String(t.clone())).collect()
        ));
        analysis_context.insert("mistake_type".to_string(), serde_json::Value::String(session.mistake_type.clone()));
        analysis_context.insert("subject".to_string(), serde_json::Value::String(session.subject.clone()));
        
        // 使用RAG增强的模型调用
        state.llm_manager.call_rag_enhanced_model_stream(
            &analysis_context,
            retrieved_context.clone(),
            &[], // 空的聊天历史
            &session.subject,
            request.enable_chain_of_thought,
            Some(session.question_images.clone()), // 🎯 修复：传递图片信息
            window,
            &stream_event,
        ).await?
    } else {
        // 使用普通的模型调用
        let mut context = std::collections::HashMap::new();
        context.insert("ocr_text".to_string(), serde_json::json!(session.ocr_text));
        context.insert("tags".to_string(), serde_json::json!(session.tags));
        context.insert("mistake_type".to_string(), serde_json::json!(session.mistake_type));
        context.insert("user_question".to_string(), serde_json::json!(session.user_question));
        
        state.llm_manager.call_unified_model_2_stream(
            &context,
            &[],
            &session.subject,
            request.enable_chain_of_thought,
            Some(session.question_images.clone()), // 🎯 修复：传递图片信息
            None,
            window,
            &stream_event,
        ).await?
    };
    
    // 更新临时会话的聊天历史
    {
        let mut sessions = state.temp_sessions.lock().await;
        if let Some(session) = sessions.get_mut(&request.temp_id) {
            // 转换RAG来源信息
            let rag_sources = if !retrieved_context.is_empty() {
                Some(retrieved_context.iter().map(|chunk| crate::models::RagSourceInfo {
                    document_id: chunk.chunk.document_id.clone(),
                    file_name: chunk.chunk.metadata.get("file_name")
                        .unwrap_or(&"unknown".to_string()).clone(),
                    chunk_text: chunk.chunk.text.clone(),
                    score: chunk.score,
                    chunk_index: chunk.chunk.chunk_index,
                }).collect())
            } else {
                None
            };
            
            session.chat_history.push(ChatMessage {
                role: "assistant".to_string(),
                content: model2_result.assistant_message,
                timestamp: Utc::now(),
                thinking_content: None,
                rag_sources,
                image_paths: None,
                image_base64: None,
            });
        }
    }
    
    println!("✅ RAG增强的流式分析完成");
    Ok(())
}

/// RAG增强的继续对话
#[tauri::command]
pub async fn continue_rag_enhanced_chat_stream(
    request: RagEnhancedChatRequest,
    window: Window,
    state: State<'_, AppState>,
) -> Result<ContinueChatResponse> {
    println!("💬 RAG增强的继续对话: {}, RAG: {:?}", 
        request.temp_id, request.enable_rag);
    
    // 获取临时会话
    let temp_session = {
        let sessions = state.temp_sessions.lock().await;
        sessions.get(&request.temp_id).cloned()
    };
    
    let mut session = temp_session.ok_or("临时会话不存在")?;
    
    // 如果启用RAG，进行知识库查询
    let retrieved_context = if request.enable_rag.unwrap_or(false) {
        let rag_options = request.rag_options.unwrap_or(RagQueryOptions {
            top_k: 5,
            enable_reranking: Some(false),
        });
        
        // 从最新的用户消息中提取查询
        let latest_user_message = request.chat_history.iter()
            .filter(|msg| msg.role == "user")
            .last()
            .map(|msg| msg.content.clone())
            .unwrap_or_else(|| session.user_question.clone());
        
        println!("🔍 RAG查询: '{}'", latest_user_message);
        let rag_response = state.rag_manager.query_knowledge_base(&latest_user_message, rag_options).await?;
        rag_response.retrieved_chunks
    } else {
        Vec::new()
    };
    
    // 启动流式对话
    let stream_event = format!("continue_chat_stream_{}", request.temp_id);
    
    let model2_result = if !retrieved_context.is_empty() {
        // 获取最新用户消息
        let latest_user_query = request.chat_history.iter()
            .filter(|msg| msg.role == "user")
            .last()
            .map(|msg| msg.content.clone())
            .unwrap_or_else(|| "请继续分析".to_string());
        
        // 构建完整的分析上下文（包含原始错题信息）
        let mut analysis_context = std::collections::HashMap::new();
        analysis_context.insert("user_question".to_string(), serde_json::Value::String(session.user_question.clone()));
        analysis_context.insert("latest_user_query".to_string(), serde_json::Value::String(latest_user_query));
        analysis_context.insert("ocr_text".to_string(), serde_json::Value::String(session.ocr_text.clone()));
        analysis_context.insert("tags".to_string(), serde_json::Value::Array(
            session.tags.iter().map(|t| serde_json::Value::String(t.clone())).collect()
        ));
        analysis_context.insert("mistake_type".to_string(), serde_json::Value::String(session.mistake_type.clone()));
        analysis_context.insert("subject".to_string(), serde_json::Value::String(session.subject.clone()));
            
        // 使用RAG增强的模型调用
        state.llm_manager.call_rag_enhanced_model_stream(
            &analysis_context,
            retrieved_context.clone(),
            &request.chat_history,
            &session.subject,
            request.enable_chain_of_thought.unwrap_or(false),
            Some(session.question_images.clone()), // 🎯 修复：传递图片信息
            window,
            &stream_event,
        ).await?
    } else {
        // 使用普通的模型调用
        let mut context = std::collections::HashMap::new();
        context.insert("ocr_text".to_string(), serde_json::json!(session.ocr_text));
        context.insert("tags".to_string(), serde_json::json!(session.tags));
        context.insert("mistake_type".to_string(), serde_json::json!(session.mistake_type));
        context.insert("user_question".to_string(), serde_json::json!(session.user_question));
        
        let model_config = state.llm_manager.get_model2_config().await
            .map_err(|e| format!("获取模型配置失败: {}", e))?;
        
        let enable_chain_of_thought = request.enable_chain_of_thought.unwrap_or(model_config.is_reasoning);
        
        state.llm_manager.call_unified_model_2_stream(
            &context,
            &request.chat_history,
            &session.subject,
            enable_chain_of_thought,
            Some(session.question_images.clone()), // 🎯 修复：传递图片信息
            Some("基于题目信息继续对话解答用户问题"),
            window,
            &stream_event,
        ).await?
    };
    
    // 更新会话的聊天记录
    session.chat_history = request.chat_history;
    
    // 转换RAG来源信息（如果有的话）
    let rag_sources = if !retrieved_context.is_empty() {
        Some(retrieved_context.iter().map(|chunk| crate::models::RagSourceInfo {
            document_id: chunk.chunk.document_id.clone(),
            file_name: chunk.chunk.metadata.get("file_name")
                .unwrap_or(&"unknown".to_string()).clone(),
            chunk_text: chunk.chunk.text.clone(),
            score: chunk.score,
            chunk_index: chunk.chunk.chunk_index,
        }).collect())
    } else {
        None
    };
    
    session.chat_history.push(ChatMessage {
        role: "assistant".to_string(),
        content: model2_result.assistant_message.clone(),
        timestamp: Utc::now(),
        thinking_content: None,
        rag_sources,
        image_paths: None,
        image_base64: None,
    });
    
    // 保存更新后的会话
    {
        let mut sessions = state.temp_sessions.lock().await;
        sessions.insert(request.temp_id, session);
    }
    
    println!("✅ RAG增强的对话完成");
    Ok(ContinueChatResponse {
        new_assistant_message: model2_result.assistant_message,
    })
}

/// LLM基于上下文生成回答
#[tauri::command]
pub async fn llm_generate_answer_with_context(
    user_query: String,
    retrieved_chunks_json: String,
    state: State<'_, AppState>,
) -> Result<String> {
    println!("🤖 开始生成基于上下文的回答");
    
    if user_query.trim().is_empty() {
        return Err(AppError::validation("用户查询不能为空"));
    }
    
    // 解析检索到的文档块
    let retrieved_chunks: Vec<crate::models::RetrievedChunk> = serde_json::from_str(&retrieved_chunks_json)
        .map_err(|e| AppError::validation(format!("解析检索结果失败: {}", e)))?;
    
    if retrieved_chunks.is_empty() {
        return Err(AppError::validation("没有提供知识库上下文"));
    }
    
    // 构建RAG增强的Prompt
    let mut context_text = String::new();
    context_text.push_str("以下是从知识库中检索到的相关信息：\n\n");
    
    for (index, chunk) in retrieved_chunks.iter().enumerate() {
        context_text.push_str(&format!(
            "[文档{}] 来源: {}\n{}\n\n",
            index + 1,
            chunk.chunk.metadata.get("file_name").unwrap_or(&"未知文档".to_string()),
            chunk.chunk.text
        ));
    }
    
    context_text.push_str(&format!(
        "基于以上知识库信息，请回答用户的问题：\n{}\n\n",
        user_query
    ));
    context_text.push_str("请确保回答准确、有用，并在适当时引用具体的来源信息。如果知识库中的信息不足以回答问题，请明确说明。");
    
    // 构建上下文参数
    let mut context_map = std::collections::HashMap::new();
    context_map.insert("context".to_string(), serde_json::Value::String(context_text));
    
    // 调用LLM生成回答
    let answer = state.llm_manager.call_unified_model_2(
        &context_map,
        &[], // 空的聊天历史
        "通用", // 科目
        false, // 不启用思维链
        None, // 没有图片
        Some("RAG增强问答") // 任务上下文
    ).await.map_err(|e| AppError::llm(format!("LLM生成回答失败: {}", e)))?;
    
    println!("✅ 基于上下文的回答生成完成，长度: {} 字符", answer.assistant_message.len());
    Ok(answer.assistant_message)
}

// ==================== RAG配置管理命令 ====================

/// 获取RAG配置
#[tauri::command]
pub async fn get_rag_settings(
    state: State<'_, AppState>,
) -> Result<crate::models::RagConfigResponse> {
    println!("📖 获取RAG配置");
    
    let config = state.database.get_rag_configuration()
        .map_err(|e| AppError::database(format!("获取RAG配置失败: {}", e)))?;
    
    match config {
        Some(config) => {
            println!("✅ RAG配置获取成功");
            Ok(crate::models::RagConfigResponse {
                chunk_size: config.chunk_size,
                chunk_overlap: config.chunk_overlap,
                chunking_strategy: config.chunking_strategy,
                min_chunk_size: config.min_chunk_size,
                default_top_k: config.default_top_k,
                default_rerank_enabled: config.default_rerank_enabled,
            })
        }
        None => {
            // 如果没有配置，返回默认值
            println!("⚠️ 未找到RAG配置，返回默认值");
            Ok(crate::models::RagConfigResponse {
                chunk_size: 512,
                chunk_overlap: 50,
                chunking_strategy: "fixed_size".to_string(),
                min_chunk_size: 20,
                default_top_k: 5,
                default_rerank_enabled: false,
            })
        }
    }
}

/// 更新RAG配置
#[tauri::command]
pub async fn update_rag_settings(
    settings: crate::models::RagConfigRequest,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("🔧 更新RAG配置");
    
    // 验证配置参数
    if settings.chunk_size < 50 || settings.chunk_size > 2048 {
        return Err(AppError::validation("分块大小必须在50-2048之间"));
    }
    
    if settings.chunk_overlap < 0 || settings.chunk_overlap >= settings.chunk_size {
        return Err(AppError::validation("重叠大小必须非负且小于分块大小"));
    }
    
    if settings.min_chunk_size < 10 || settings.min_chunk_size > settings.chunk_size {
        return Err(AppError::validation("最小分块大小必须在10和分块大小之间"));
    }
    
    if settings.default_top_k < 1 || settings.default_top_k > 50 {
        return Err(AppError::validation("默认检索数量必须在1-50之间"));
    }
    
    if !["fixed_size", "semantic"].contains(&settings.chunking_strategy.as_str()) {
        return Err(AppError::validation("分块策略必须是 'fixed_size' 或 'semantic'"));
    }
    
    state.database.update_rag_configuration(&settings)
        .map_err(|e| AppError::database(format!("更新RAG配置失败: {}", e)))?;
    
    println!("✅ RAG配置更新成功");
    Ok(())
}

/// 重置RAG配置为默认值
#[tauri::command]
pub async fn reset_rag_settings(
    state: State<'_, AppState>,
) -> Result<()> {
    println!("🔄 重置RAG配置为默认值");
    
    state.database.reset_rag_configuration()
        .map_err(|e| AppError::database(format!("重置RAG配置失败: {}", e)))?;
    
    println!("✅ RAG配置已重置为默认值");
    Ok(())
}

// ============================================================================
// RAG分库管理相关命令
// ============================================================================

/// 创建新的RAG分库
#[tauri::command]
pub async fn create_rag_sub_library(
    request: CreateSubLibraryRequest,
    state: State<'_, AppState>,
) -> Result<SubLibrary> {
    println!("🏗️ 创建新分库: {}", request.name);
    
    let sub_library = state.database.create_sub_library(&request)
        .map_err(|e| AppError::database(format!("创建分库失败: {}", e)))?;
    
    println!("✅ 分库创建成功: {} (ID: {})", sub_library.name, sub_library.id);
    Ok(sub_library)
}

/// 获取所有RAG分库列表
#[tauri::command]
pub async fn get_rag_sub_libraries(
    state: State<'_, AppState>,
) -> Result<Vec<SubLibrary>> {
    println!("📚 获取分库列表");
    
    let libraries = state.database.list_sub_libraries()
        .map_err(|e| AppError::database(format!("获取分库列表失败: {}", e)))?;
    
    println!("✅ 获取到 {} 个分库", libraries.len());
    Ok(libraries)
}

/// 根据ID获取RAG分库详情
#[tauri::command]
pub async fn get_rag_sub_library_by_id(
    libraryId: String,
    state: State<'_, AppState>,
) -> Result<Option<SubLibrary>> {
    println!("🔍 获取分库详情: {}", libraryId);
    
    let library = state.database.get_sub_library_by_id(&libraryId)
        .map_err(|e| AppError::database(format!("获取分库详情失败: {}", e)))?;
    
    if let Some(ref lib) = library {
        println!("✅ 找到分库: {}", lib.name);
    } else {
        println!("⚠️ 未找到分库: {}", libraryId);
    }
    
    Ok(library)
}

/// 更新RAG分库信息
#[tauri::command]
pub async fn update_rag_sub_library(
    libraryId: String,
    request: UpdateSubLibraryRequest,
    state: State<'_, AppState>,
) -> Result<SubLibrary> {
    println!("📝 更新分库: {}", libraryId);
    
    let updated_library = state.database.update_sub_library(&libraryId, &request)
        .map_err(|e| AppError::database(format!("更新分库失败: {}", e)))?;
    
    println!("✅ 分库更新成功: {}", updated_library.name);
    Ok(updated_library)
}

/// 删除RAG分库
#[tauri::command]
pub async fn delete_rag_sub_library(
    libraryId: String,
    deleteDocuments: Option<bool>,
    state: State<'_, AppState>,
) -> Result<()> {
    let delete_contained_documents = deleteDocuments.unwrap_or(false);
    println!("🗑️ 删除分库: {} (删除文档: {})", libraryId, delete_contained_documents);
    
    state.database.delete_sub_library(&libraryId, delete_contained_documents)
        .map_err(|e| AppError::database(format!("删除分库失败: {}", e)))?;
    
    println!("✅ 分库删除成功");
    Ok(())
}

/// 向指定分库添加文档
#[tauri::command]
pub async fn rag_add_documents_to_library(
    request: RagAddDocumentsRequest,
    window: Window,
    state: State<'_, AppState>,
) -> Result<String> {
    let target_library = request.sub_library_id.as_deref();
    println!("📄 向分库添加文档: {:?}, 文档数: {}", target_library, request.file_paths.len());
    
    // 使用相同的修复方式：立即读取文件内容避免路径失效
    let mut document_contents = Vec::new();
    
    for (index, file_path) in request.file_paths.iter().enumerate() {
        println!("📄 文件 {}: {}", index + 1, file_path);
        
        // 尝试规范化路径
        let normalized_path = match std::path::Path::new(file_path).canonicalize() {
            Ok(canonical) => {
                let canonical_str = canonical.display().to_string();
                println!("🔧 规范化路径: {} -> {}", file_path, canonical_str);
                canonical_str
            }
            Err(e) => {
                println!("⚠️ 无法规范化路径 {}: {}，使用原路径", file_path, e);
                file_path.clone()
            }
        };
        
        let path = std::path::Path::new(&normalized_path);
        
        // 检查文件是否存在
        if !path.exists() {
            println!("❌ 文件不存在: {} (原路径: {})", normalized_path, file_path);
            return Err(AppError::file_system(format!("文件不存在: {} (原路径: {})", normalized_path, file_path)));
        }
        
        if !path.is_file() {
            println!("❌ 路径不是文件: {} (原路径: {})", normalized_path, file_path);
            return Err(AppError::file_system(format!("指定路径不是文件: {} (原路径: {})", normalized_path, file_path)));
        }
        
        // 立即读取文件内容，避免后续路径失效
        let file_name = path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string();
        
        println!("📚 立即读取文件内容: {}", file_name);
        
        let content = match path.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_lowercase().as_str() {
            "txt" | "md" | "markdown" => {
                // 文本文件直接读取
                match std::fs::read_to_string(&normalized_path) {
                    Ok(content) => content,
                    Err(e) => {
                        println!("❌ 读取文本文件失败: {} - {}", file_name, e);
                        return Err(AppError::file_system(format!("读取文本文件失败: {} - {}", file_name, e)));
                    }
                }
            }
            "pdf" | "docx" => {
                // 二进制文件读取为base64
                match std::fs::read(&normalized_path) {
                    Ok(bytes) => {
                        use base64::{Engine as _, engine::general_purpose};
                        general_purpose::STANDARD.encode(&bytes)
                    }
                    Err(e) => {
                        println!("❌ 读取二进制文件失败: {} - {}", file_name, e);
                        return Err(AppError::file_system(format!("读取二进制文件失败: {} - {}", file_name, e)));
                    }
                }
            }
            _ => {
                println!("❌ 不支持的文件类型: {}", file_name);
                return Err(AppError::validation(format!("不支持的文件类型: {}", file_name)));
            }
        };
        
        println!("✅ 文件内容读取成功: {} ({} 字符)", file_name, content.len());
        
        document_contents.push(serde_json::json!({
            "fileName": file_name,
            "content": content
        }));
    }
    
    if document_contents.is_empty() {
        return Err(AppError::validation("文件内容列表不能为空"));
    }
    
    // 需要创建支持分库的内容上传方法
    let result = if let Some(sub_library_id) = request.sub_library_id {
        // 向指定分库添加
        state.rag_manager.add_documents_from_content_to_library(document_contents, window, Some(sub_library_id)).await?
    } else {
        // 添加到默认分库
        state.rag_manager.add_documents_from_content(document_contents, window).await?
    };
    
    Ok(result)
}

/// 从Base64内容向指定分库添加文档
#[tauri::command]
pub async fn rag_add_documents_from_content_to_library(
    request: RagAddDocumentsFromContentRequest,
    window: Window,
    state: State<'_, AppState>,
) -> Result<String> {
    let target_library = request.sub_library_id.as_deref();
    println!("📄 从内容向分库添加文档: {:?}, 文档数: {}", target_library, request.documents.len());
    
    // 转换请求格式
    let documents: Vec<serde_json::Value> = request.documents.into_iter().map(|doc| {
        serde_json::json!({
            "fileName": doc.file_name,
            "content": doc.base64_content
        })
    }).collect();
    
    let result = state.rag_manager.add_documents_from_content_to_library(documents, window, request.sub_library_id).await?;
    Ok(result)
}

/// 获取指定分库的文档列表
#[tauri::command]
pub async fn get_rag_documents_by_library(
    request: GetDocumentsRequest,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>> {
    let library_id = request.sub_library_id.as_deref().unwrap_or("default");
    println!("📑 获取分库文档列表: {}", library_id);
    
    let documents = state.database.get_documents_by_sub_library(
        library_id, 
        request.page, 
        request.page_size
    ).map_err(|e| AppError::database(format!("获取文档列表失败: {}", e)))?;
    
    println!("✅ 获取到 {} 个文档", documents.len());
    Ok(documents)
}

/// 将文档移动到指定分库
#[tauri::command]
pub async fn move_document_to_rag_library(
    documentId: String,
    targetLibraryId: String,
    state: State<'_, AppState>,
) -> Result<()> {
    println!("📦 移动文档到分库: {} -> {}", documentId, targetLibraryId);
    
    state.database.move_document_to_sub_library(&documentId, &targetLibraryId)
        .map_err(|e| AppError::database(format!("移动文档失败: {}", e)))?;
    
    println!("✅ 文档移动成功");
    Ok(())
}

/// 在指定分库中查询知识库
#[tauri::command]
pub async fn rag_query_knowledge_base_in_libraries(
    query: String,
    options: RagQueryOptionsWithLibraries,
    state: State<'_, AppState>,
) -> Result<RagQueryResponse> {
    println!("🔍 在指定分库中查询: '{}', 分库: {:?}", query, options.target_sub_library_ids);
    
    let rag_options = RagQueryOptions {
        top_k: options.top_k,
        enable_reranking: options.enable_reranking,
    };
    
    let response = state.rag_manager.query_knowledge_base_in_libraries(
        &query, 
        rag_options, 
        options.target_sub_library_ids
    ).await?;
    
    println!("✅ 查询完成，返回 {} 个结果", response.retrieved_chunks.len());
    Ok(response)
}

// ============================================================================
// 文档解析相关命令
// ============================================================================

/// 从文件路径解析文档文本
#[tauri::command]
pub async fn parse_document_from_path(
    file_path: String,
) -> std::result::Result<String, String> {
    println!("🔍 开始解析文档: {}", file_path);
    
    let parser = crate::document_parser::DocumentParser::new();
    
    match parser.extract_text_from_path(&file_path) {
        Ok(text) => {
            println!("✅ 文档解析成功，提取文本长度: {} 字符", text.len());
            Ok(text)
        }
        Err(err) => {
            let error_msg = format!("文档解析失败: {}", err);
            println!("❌ {}", error_msg);
            Err(error_msg)
        }
    }
}

/// 从Base64编码内容解析文档文本
#[tauri::command]
pub async fn parse_document_from_base64(
    file_name: String,
    base64_content: String,
) -> std::result::Result<String, String> {
    println!("🔍 开始解析Base64文档: {}", file_name);
    
    let parser = crate::document_parser::DocumentParser::new();
    
    match parser.extract_text_from_base64(&file_name, &base64_content) {
        Ok(text) => {
            println!("✅ Base64文档解析成功，提取文本长度: {} 字符", text.len());
            Ok(text)
        }
        Err(err) => {
            let error_msg = format!("Base64文档解析失败: {}", err);
            println!("❌ {}", error_msg);
            Err(error_msg)
        }
    }
}

/// 生成错题总结 - 使用第二模型基于聊天记录生成结构化总结
#[tauri::command]
pub async fn generate_mistake_summary(
    request: GenerateMistakeSummaryRequest,
    state: State<'_, AppState>,
) -> Result<GenerateMistakeSummaryResponse> {
    println!("🧠 开始生成错题总结: {}", request.mistake_id);
    
    // 从数据库获取错题详情
    let mistake = match state.database.get_mistake_by_id(&request.mistake_id)? {
        Some(mistake) => mistake,
        None => {
            return Ok(GenerateMistakeSummaryResponse {
                success: false,
                mistake_summary: None,
                user_error_analysis: None,
                error_message: Some("错题不存在".to_string()),
            });
        }
    };
    
    // 检查是否需要重新生成
    if !request.force_regenerate.unwrap_or(false) {
        if mistake.mistake_summary.is_some() && mistake.user_error_analysis.is_some() {
            return Ok(GenerateMistakeSummaryResponse {
                success: true,
                mistake_summary: mistake.mistake_summary.clone(),
                user_error_analysis: mistake.user_error_analysis.clone(),
                error_message: None,
            });
        }
    }
    
    // 验证聊天记录存在且非空
    if mistake.chat_history.is_empty() {
        return Ok(GenerateMistakeSummaryResponse {
            success: false,
            mistake_summary: None,
            user_error_analysis: None,
            error_message: Some("没有聊天记录，无法生成总结".to_string()),
        });
    }
    
    // 获取模型配置
    let model_assignments = state.llm_manager.get_model_assignments().await
        .map_err(|e| AppError::configuration(format!("获取模型分配失败: {}", e)))?;

    // 关键修改：优先使用专门的总结模型，如果未配置，则回退到模型二
    let model_config_id = model_assignments.summary_model_config_id
        .or(model_assignments.model2_config_id) // 如果总结模型为空，则使用模型二
        .ok_or_else(|| AppError::configuration("没有为总结或分析配置任何可用模型"))?;
    
    // 获取具体模型配置
    let model_configs = match state.llm_manager.get_api_configs().await {
        Ok(configs) => configs,
        Err(e) => {
            return Ok(GenerateMistakeSummaryResponse {
                success: false,
                mistake_summary: None,
                user_error_analysis: None,
                error_message: Some(format!("获取API配置失败: {}", e)),
            });
        }
    };
    
    let model_config = model_configs.iter()
        .find(|config| config.id == model_config_id)
        .ok_or_else(|| AppError::configuration("第二模型配置不存在"))?;
    
    // 构建总结生成提示词
    let summary_prompt = build_summary_generation_prompt(&mistake);
    
    // 准备统一模型2的调用参数
    let mut context = std::collections::HashMap::new();
    context.insert("task".to_string(), serde_json::Value::String("mistake_summary".to_string()));
    context.insert("prompt".to_string(), serde_json::Value::String(summary_prompt));
    
    // 调用第二模型生成总结
    match state.llm_manager.call_unified_model_2(&context, &[], &mistake.subject, false, None, Some("mistake_summary_generation")).await {
        Ok(response) => {
            // 解析AI回复，提取两个字段
            let (mistake_summary, user_error_analysis) = parse_summary_response(&response.assistant_message);
            
            // 🎯 修复BUG-02：更新数据库并设置状态为已完成
            let mut updated_mistake = mistake.clone();
            updated_mistake.mistake_summary = Some(mistake_summary.clone());
            updated_mistake.user_error_analysis = Some(user_error_analysis.clone());
            updated_mistake.status = "completed".to_string(); // 🎯 修复：生成总结后将状态设为已完成
            updated_mistake.updated_at = Utc::now();
            
            if let Err(e) = state.database.save_mistake(&updated_mistake) {
                println!("⚠️ 保存错题总结失败: {}", e);
                return Ok(GenerateMistakeSummaryResponse {
                    success: false,
                    mistake_summary: None,
                    user_error_analysis: None,
                    error_message: Some(format!("保存总结失败: {}", e)),
                });
            }
            
            println!("✅ 错题总结生成成功");
            Ok(GenerateMistakeSummaryResponse {
                success: true,
                mistake_summary: Some(mistake_summary),
                user_error_analysis: Some(user_error_analysis),
                error_message: None,
            })
        }
        Err(e) => {
            println!("❌ 总结生成失败: {}", e);
            Ok(GenerateMistakeSummaryResponse {
                success: false,
                mistake_summary: None,
                user_error_analysis: None,
                error_message: Some(format!("AI生成失败: {}", e)),
            })
        }
    }
}

/// 构建总结生成提示词
fn build_summary_generation_prompt(mistake: &MistakeItem) -> String {
    let mut prompt = String::new();
    
    prompt.push_str("你是一个专业的教育分析专家。请基于以下错题信息和师生对话记录，生成两个结构化总结：\n\n");
    
    // 题目信息
    prompt.push_str(&format!("【题目信息】\n科目：{}\n题目内容：{}\n学生原始问题：{}\n\n", 
        mistake.subject, mistake.ocr_text, mistake.user_question));
    
    // 聊天记录
    prompt.push_str("【师生对话记录】\n");
    for (i, message) in mistake.chat_history.iter().enumerate() {
        let role_name = match message.role.as_str() {
            "user" => "学生",
            "assistant" => "老师", 
            _ => &message.role,
        };
        prompt.push_str(&format!("{}. {}: {}\n", i + 1, role_name, message.content));
    }
    
    prompt.push_str("\n请基于以上信息，生成以下两个总结（用===分隔）：\n\n");
    prompt.push_str("【错题简要解析】\n");
    prompt.push_str("简要描述题目要点、正确解法和涉及的关键知识点，150字以内。\n\n");
    prompt.push_str("===\n\n");
    prompt.push_str("【用户错误分析】\n");
    prompt.push_str("总结学生在对话中暴露的错误原因、思维误区和薄弱点，150字以内。\n\n");
    prompt.push_str("注意：请直接输出两个总结内容，不要包含其他解释性文字。");
    
    prompt
}

/// 解析AI总结回复，提取两个字段
fn parse_summary_response(response: &str) -> (String, String) {
    let parts: Vec<&str> = response.split("===").collect();
    
    if parts.len() >= 2 {
        let mistake_summary = parts[0].trim()
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.contains("【错题简要解析】"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
            
        let user_error_analysis = parts[1].trim()
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.contains("【用户错误分析】"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();
            
        (mistake_summary, user_error_analysis)
    } else {
        // 解析失败，尝试简单分割
        let fallback_summary = response.lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(150)
            .collect();
            
        let fallback_analysis = response.lines()
            .skip(3)
            .take(3)
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(150)
            .collect();
            
        (fallback_summary, fallback_analysis)
    }
}

// 自定义模板管理命令

/// 获取所有自定义模板
#[tauri::command]
pub async fn get_all_custom_templates(
    state: State<'_, AppState>,
) -> Result<Vec<CustomAnkiTemplate>> {
    let templates = state.database.get_all_custom_templates()
        .map_err(|e| AppError::database(format!("获取模板列表失败: {}", e)))?;
    Ok(templates)
}

/// 获取指定ID的自定义模板
#[tauri::command]
pub async fn get_custom_template_by_id(
    template_id: String,
    state: State<'_, AppState>,
) -> Result<Option<CustomAnkiTemplate>> {
    let template = state.database.get_custom_template_by_id(&template_id)
        .map_err(|e| AppError::database(format!("获取模板失败: {}", e)))?;
    Ok(template)
}

/// 创建自定义模板
#[tauri::command]
pub async fn create_custom_template(
    request: CreateTemplateRequest,
    state: State<'_, AppState>,
) -> Result<String> {
    // 验证模板数据
    validate_template_request(&request)?;
    
    let template_id = state.database.create_custom_template(&request)
        .map_err(|e| AppError::database(format!("创建模板失败: {}", e)))?;
    
    Ok(template_id)
}

/// 更新自定义模板
#[tauri::command]
pub async fn update_custom_template(
    template_id: String,
    request: UpdateTemplateRequest,
    state: State<'_, AppState>,
) -> Result<()> {
    // 验证模板是否存在且不是内置模板
    let existing_template = state.database.get_custom_template_by_id(&template_id)
        .map_err(|e| AppError::database(format!("查询模板失败: {}", e)))?;
    
    match existing_template {
        Some(template) => {
            if template.is_built_in {
                return Err(AppError::validation("不能修改内置模板".to_string()));
            }
        },
        None => {
            return Err(AppError::validation("模板不存在".to_string()));
        }
    }
    
    state.database.update_custom_template(&template_id, &request)
        .map_err(|e| AppError::database(format!("更新模板失败: {}", e)))?;
    
    Ok(())
}

/// 删除自定义模板
#[tauri::command]
pub async fn delete_custom_template(
    template_id: String,
    state: State<'_, AppState>,
) -> Result<()> {
    // 验证模板是否存在且不是内置模板
    let existing_template = state.database.get_custom_template_by_id(&template_id)
        .map_err(|e| AppError::database(format!("查询模板失败: {}", e)))?;
    
    match existing_template {
        Some(template) => {
            if template.is_built_in {
                return Err(AppError::validation("不能删除内置模板".to_string()));
            }
        },
        None => {
            return Err(AppError::validation("模板不存在".to_string()));
        }
    }
    
    state.database.delete_custom_template(&template_id)
        .map_err(|e| AppError::database(format!("删除模板失败: {}", e)))?;
    
    Ok(())
}

/// 导出模板
#[tauri::command]
pub async fn export_template(
    template_id: String,
    state: State<'_, AppState>,
) -> Result<TemplateExportResponse> {
    let template = state.database.get_custom_template_by_id(&template_id)
        .map_err(|e| AppError::database(format!("查询模板失败: {}", e)))?;
    
    match template {
        Some(template) => {
            let template_data = serde_json::to_string_pretty(&template)
                .map_err(|e| AppError::validation(format!("序列化模板失败: {}", e)))?;
            
            let filename = format!("{}_template.json", template.name.replace(" ", "_"));
            
            Ok(TemplateExportResponse {
                template_data,
                filename,
            })
        },
        None => Err(AppError::validation("模板不存在".to_string())),
    }
}

/// 导入模板
#[tauri::command]
pub async fn import_template(
    request: TemplateImportRequest,
    state: State<'_, AppState>,
) -> Result<String> {
    // 解析模板数据
    let template: CustomAnkiTemplate = serde_json::from_str(&request.template_data)
        .map_err(|e| AppError::validation(format!("解析模板数据失败: {}", e)))?;
    
    // 检查是否已存在同名模板
    let existing_templates = state.database.get_all_custom_templates()
        .map_err(|e| AppError::database(format!("查询现有模板失败: {}", e)))?;
    
    if existing_templates.iter().any(|t| t.name == template.name) {
        if !request.overwrite_existing {
            return Err(AppError::validation(format!("模板 '{}' 已存在", template.name)));
        }
        // 找到同名模板并删除（如果不是内置模板）
        if let Some(existing) = existing_templates.iter().find(|t| t.name == template.name) {
            if existing.is_built_in {
                return Err(AppError::validation("不能覆盖内置模板".to_string()));
            }
            state.database.delete_custom_template(&existing.id)
                .map_err(|e| AppError::database(format!("删除旧模板失败: {}", e)))?;
        }
    }
    
    // 创建新模板
    let create_request = CreateTemplateRequest {
        name: template.name,
        description: template.description,
        author: template.author,
        preview_front: template.preview_front,
        preview_back: template.preview_back,
        note_type: template.note_type,
        fields: template.fields,
        generation_prompt: template.generation_prompt,
        front_template: template.front_template,
        back_template: template.back_template,
        css_style: template.css_style,
        field_extraction_rules: template.field_extraction_rules,
    };
    
    validate_template_request(&create_request)?;
    
    let template_id = state.database.create_custom_template(&create_request)
        .map_err(|e| AppError::database(format!("导入模板失败: {}", e)))?;
    
    Ok(template_id)
}

/// 验证模板请求数据
fn validate_template_request(request: &CreateTemplateRequest) -> Result<()> {
    // 验证基本字段
    if request.name.trim().is_empty() {
        return Err(AppError::validation("模板名称不能为空".to_string()));
    }
    
    if request.fields.is_empty() {
        return Err(AppError::validation("模板必须至少包含一个字段".to_string()));
    }
    
    // 验证必须包含 front 和 back 字段
    let has_front = request.fields.iter().any(|f| f.to_lowercase() == "front");
    let has_back = request.fields.iter().any(|f| f.to_lowercase() == "back");
    
    if !has_front {
        return Err(AppError::validation("模板必须包含 'Front' 字段".to_string()));
    }
    
    if !has_back {
        return Err(AppError::validation("模板必须包含 'Back' 字段".to_string()));
    }
    
    // 验证模板语法
    if request.front_template.trim().is_empty() {
        return Err(AppError::validation("正面模板不能为空".to_string()));
    }
    
    if request.back_template.trim().is_empty() {
        return Err(AppError::validation("背面模板不能为空".to_string()));
    }
    
    if request.generation_prompt.trim().is_empty() {
        return Err(AppError::validation("生成提示词不能为空".to_string()));
    }
    
    // 验证字段提取规则
    for field in &request.fields {
        if !request.field_extraction_rules.contains_key(field) {
            return Err(AppError::validation(format!("缺少字段 '{}' 的提取规则", field)));
        }
    }
    
    Ok(())
}


// =================== 图片遮罩卡相关命令 ===================

/// 图片文字坐标识别
#[tauri::command]
pub async fn extract_image_text_coordinates(
    request: ImageOcrRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ImageOcrResponse> {
    Ok(state.image_occlusion_service.extract_text_coordinates(request).await?)
}

/// 创建图片遮罩卡
#[tauri::command]
pub async fn create_image_occlusion_card(
    request: CreateImageOcclusionRequest,
    state: tauri::State<'_, AppState>,
) -> Result<ImageOcclusionResponse> {
    Ok(state.image_occlusion_service.create_image_occlusion_card(request).await?)
}

/// 获取所有图片遮罩卡
#[tauri::command]
pub async fn get_all_image_occlusion_cards(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ImageOcclusionCard>> {
    Ok(state.image_occlusion_service.get_all_image_occlusion_cards()?)
}

/// 根据ID获取图片遮罩卡
#[tauri::command]
pub async fn get_image_occlusion_card(
    card_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<ImageOcclusionCard>> {
    Ok(state.image_occlusion_service.get_image_occlusion_card(&card_id)?)
}

/// 更新图片遮罩卡
#[tauri::command]
pub async fn update_image_occlusion_card(
    card: ImageOcclusionCard,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    Ok(state.image_occlusion_service.update_image_occlusion_card(&card)?)
}

/// 删除图片遮罩卡
#[tauri::command]
pub async fn delete_image_occlusion_card(
    card_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    Ok(state.image_occlusion_service.delete_image_occlusion_card(&card_id)?)
}

/// 设置默认模板
#[tauri::command]
pub async fn set_default_template(
    template_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    Ok(state.database.set_default_template(&template_id)?)
}

/// 获取默认模板ID
#[tauri::command]
pub async fn get_default_template_id(
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>> {
    Ok(state.database.get_default_template()?)
}

// ============= 测试日志相关命令 =============

/// 保存测试日志到文件
#[tauri::command]
pub async fn save_test_log(
    file_name: String,
    content: String,
    log_type: String,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    use std::fs;
    use std::path::PathBuf;
    
    // 创建日志目录路径
    let mut log_dir = state.file_manager.get_app_data_dir().to_path_buf();
    log_dir.push("logs");
    log_dir.push(&log_type);
    
    // 确保目录存在
    if let Err(e) = fs::create_dir_all(&log_dir) {
        return Err(AppError::file_system(format!("创建日志目录失败: {}", e)));
    }
    
    // 构建完整文件路径
    let file_path = log_dir.join(&file_name);
    
    // 写入日志文件
    if let Err(e) = fs::write(&file_path, content) {
        return Err(AppError::file_system(format!("写入日志文件失败: {}", e)));
    }
    
    println!("测试日志已保存: {:?}", file_path);
    Ok(())
}

/// 获取测试日志列表
#[tauri::command]
pub async fn get_test_logs(
    log_type: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<String>> {
    use std::fs;
    
    let mut log_dir = state.file_manager.get_app_data_dir().to_path_buf();
    log_dir.push("logs");
    log_dir.push(&log_type);
    
    if !log_dir.exists() {
        return Ok(vec![]);
    }
    
    let mut log_files = Vec::new();
    
    if let Ok(entries) = fs::read_dir(&log_dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "log") {
                    if let Some(file_name) = path.file_name() {
                        if let Some(file_name_str) = file_name.to_str() {
                            let relative_path = format!("logs/{}/{}", log_type, file_name_str);
                            log_files.push(relative_path);
                        }
                    }
                }
            }
        }
    }
    
    // 按修改时间排序（最新的在前）
    log_files.sort_by(|a, b| {
        let path_a = state.file_manager.get_app_data_dir().join(a);
        let path_b = state.file_manager.get_app_data_dir().join(b);
        
        let time_a = path_a.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        let time_b = path_b.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        
        time_b.cmp(&time_a) // 降序
    });
    
    Ok(log_files)
}

/// 打开指定的日志文件
#[tauri::command]
pub async fn open_log_file(
    log_path: String,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    use std::process::Command;
    
    let full_path = state.file_manager.get_app_data_dir().join(&log_path);
    
    if !full_path.exists() {
        return Err(AppError::not_found(format!("日志文件不存在: {}", log_path)));
    }
    
    // 根据操作系统选择合适的命令打开文件
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = Command::new("notepad").arg(&full_path).spawn() {
            // 如果notepad失败，尝试默认程序
            if let Err(e2) = Command::new("cmd").args(&["/C", "start", "", full_path.to_str().unwrap_or("")]).spawn() {
                return Err(AppError::file_system(format!("打开日志文件失败: {} (备用方案也失败: {})", e, e2)));
            }
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = Command::new("open").arg(&full_path).spawn() {
            return Err(AppError::file_system(format!("打开日志文件失败: {}", e)));
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = Command::new("xdg-open").arg(&full_path).spawn() {
            return Err(AppError::file_system(format!("打开日志文件失败: {}", e)));
        }
    }
    
    Ok(())
}

/// 打开日志文件夹
#[tauri::command]
pub async fn open_logs_folder(
    log_type: String,
    state: tauri::State<'_, AppState>,
) -> Result<()> {
    use std::process::Command;
    
    let mut log_dir = state.file_manager.get_app_data_dir().to_path_buf();
    log_dir.push("logs");
    log_dir.push(&log_type);
    
    // 确保目录存在
    if let Err(_) = std::fs::create_dir_all(&log_dir) {
        return Err(AppError::file_system("创建日志目录失败".to_string()));
    }
    
    // 根据操作系统选择合适的命令打开文件夹
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = Command::new("explorer").arg(&log_dir).spawn() {
            return Err(AppError::file_system(format!("打开日志文件夹失败: {}", e)));
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = Command::new("open").arg(&log_dir).spawn() {
            return Err(AppError::file_system(format!("打开日志文件夹失败: {}", e)));
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = Command::new("xdg-open").arg(&log_dir).spawn() {
            return Err(AppError::file_system(format!("打开日志文件夹失败: {}", e)));
        }
    }
    
    Ok(())
}
