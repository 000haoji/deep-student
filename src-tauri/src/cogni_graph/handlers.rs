use crate::cogni_graph::{
    GraphConfig, GraphService, SearchService, CreateCardRequest, SearchRequest, 
    RecommendationRequest, ProblemCard, SearchResult, Recommendation, Tag,
    CreateTagRequest, TagHierarchy, TagType
};
use crate::commands::AppState;
use crate::llm_manager::LLMManager;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::State;

// Global state for the graph services
pub struct GraphState {
    pub graph_service: Option<GraphService>,
    pub search_service: Option<SearchService>,
    pub config: GraphConfig,
}

impl GraphState {
    pub fn new() -> Self {
        Self {
            graph_service: None,
            search_service: None,
            config: GraphConfig::default(),
        }
    }
}

// Tauri commands for the knowledge graph

#[tauri::command]
pub async fn initialize_knowledge_graph(
    config: GraphConfig,
    graph_state: State<'_, Arc<RwLock<GraphState>>>,
    app_state: State<'_, AppState>,
) -> Result<String, String> {
    let mut state = graph_state.write().await;
    
    // Initialize services
    let graph_service = GraphService::new(config.clone(), app_state.llm_manager.clone())
        .await
        .map_err(|e| format!("Failed to initialize graph service: {}", e))?;
    
    let search_service = SearchService::new(config.clone(), app_state.llm_manager.clone())
        .await
        .map_err(|e| format!("Failed to initialize search service: {}", e))?;

    state.graph_service = Some(graph_service);
    state.search_service = Some(search_service);
    state.config = config;

    Ok("Knowledge graph initialized successfully".to_string())
}

#[tauri::command]
pub async fn create_problem_card(
    request: CreateCardRequest,
    graph_state: State<'_, Arc<RwLock<GraphState>>>,
) -> Result<String, String> {
    let state = graph_state.read().await;
    
    let service = state.graph_service.as_ref()
        .ok_or("Graph service not initialized")?;

    service.create_problem_card(request)
        .await
        .map_err(|e| format!("Failed to create problem card: {}", e))
}

#[tauri::command]
pub async fn get_problem_card(
    card_id: String,
    graph_state: tauri::State<'_, Arc<RwLock<GraphState>>>,
) -> Result<Option<ProblemCard>, String> {
    let state = graph_state.read().await;
    
    let service = state.graph_service.as_ref()
        .ok_or("Graph service not initialized")?;

    service.get_problem_card(&card_id)
        .await
        .map_err(|e| format!("Failed to get problem card: {}", e))
}

#[tauri::command]
pub async fn search_knowledge_graph(
    request: SearchRequest,
    graph_state: tauri::State<'_, Arc<RwLock<GraphState>>>,
) -> Result<Vec<SearchResult>, String> {
    let state = graph_state.read().await;
    
    let service = state.search_service.as_ref()
        .ok_or("Search service not initialized")?;

    service.search(request)
        .await
        .map_err(|e| format!("Failed to search knowledge graph: {}", e))
}

#[tauri::command]
pub async fn get_ai_recommendations(
    request: RecommendationRequest,
    graph_state: tauri::State<'_, Arc<RwLock<GraphState>>>,
) -> Result<Vec<Recommendation>, String> {
    let state = graph_state.read().await;
    
    let service = state.graph_service.as_ref()
        .ok_or("Graph service not initialized")?;

    service.get_recommendations(request)
        .await
        .map_err(|e| format!("Failed to get recommendations: {}", e))
}

#[tauri::command]
pub async fn search_similar_cards(
    card_id: String,
    limit: Option<usize>,
    graph_state: tauri::State<'_, Arc<RwLock<GraphState>>>,
) -> Result<Vec<SearchResult>, String> {
    let state = graph_state.read().await;
    
    let service = state.search_service.as_ref()
        .ok_or("Search service not initialized")?;

    service.search_similar_cards(&card_id, limit)
        .await
        .map_err(|e| format!("Failed to search similar cards: {}", e))
}

#[tauri::command]
pub async fn get_cards_by_tag(
    tag_name: String,
    limit: Option<usize>,
    graph_state: tauri::State<'_, Arc<RwLock<GraphState>>>,
) -> Result<Vec<ProblemCard>, String> {
    let state = graph_state.read().await;
    
    let service = state.search_service.as_ref()
        .ok_or("Search service not initialized")?;

    service.get_cards_by_tag(&tag_name, limit)
        .await
        .map_err(|e| format!("Failed to get cards by tag: {}", e))
}

#[tauri::command]
pub async fn get_all_tags(
    graph_state: tauri::State<'_, Arc<RwLock<GraphState>>>,
) -> Result<Vec<Tag>, String> {
    let state = graph_state.read().await;
    
    let service = state.search_service.as_ref()
        .ok_or("Search service not initialized")?;

    service.get_all_tags()
        .await
        .map_err(|e| format!("Failed to get all tags: {}", e))
}

#[tauri::command]
pub async fn get_graph_config(
    graph_state: tauri::State<'_, Arc<RwLock<GraphState>>>,
) -> Result<GraphConfig, String> {
    let state = graph_state.read().await;
    Ok(state.config.clone())
}

#[tauri::command]
pub async fn update_graph_config(
    config: GraphConfig,
    graph_state: tauri::State<'_, Arc<RwLock<GraphState>>>,
    app_state: State<'_, AppState>,
) -> Result<String, String> {
    // Reinitialize services with new config
    initialize_knowledge_graph(config, graph_state, app_state).await
}

#[tauri::command]
pub async fn test_neo4j_connection(
    config: GraphConfig,
) -> Result<String, String> {
    use crate::cogni_graph::Neo4jService;
    
    match Neo4jService::new(config).await {
        Ok(_) => Ok("Connection successful".to_string()),
        Err(e) => Err(format!("Connection failed: {}", e)),
    }
}

// OCR integration command for handwritten input
#[tauri::command]
pub async fn process_handwritten_input(
    _image_data: String, // Base64 encoded image
    _graph_state: tauri::State<'_, Arc<RwLock<GraphState>>>,
    app_state: State<'_, AppState>,
) -> Result<CreateCardRequest, String> {
    // Use existing LLM manager for OCR processing
    let llm_manager = &app_state.llm_manager;
    
    // Convert base64 image to temporary file for OCR processing
    // Note: For now, we'll create a simple text extraction since image processing needs proper file handling
    let ocr_result = llm_manager.call_unified_model_1(
        vec![], // Empty image paths for now
        "Extract mathematical problem and solution from the provided content. Format as JSON with 'problem' and 'insight' fields.",
        "数学", // Default subject
        Some("OCR extraction task")
    ).await.map_err(|e| format!("OCR processing failed: {}", e))?;

    // Parse OCR result from StandardModel1Output
    let content_problem = if !ocr_result.ocr_text.is_empty() {
        ocr_result.ocr_text.clone()
    } else {
        "手写内容识别中...".to_string()
    };

    let content_insight = "请在此处添加解题洞察和思路".to_string();

    // Auto-generate tags based on content
    let tags = generate_auto_tags(&content_problem, &content_insight, llm_manager)
        .await
        .unwrap_or_default();

    Ok(CreateCardRequest {
        content_problem,
        content_insight,
        tags,
        source_excalidraw_path: None,
    })
}

async fn generate_auto_tags(problem: &str, insight: &str, llm_manager: &Arc<LLMManager>) -> Result<Vec<String>> {
    let prompt = format!(
        r#"Analyze this mathematical problem and solution to generate relevant tags:

Problem: {}
Solution: {}

Generate 3-5 tags that categorize this content. Focus on:
- Mathematical topics (e.g., "calculus", "algebra", "geometry")
- Solution methods (e.g., "substitution", "integration_by_parts")
- Difficulty level (e.g., "basic", "intermediate", "advanced")

Respond with a JSON array of strings: ["tag1", "tag2", "tag3"]"#,
        problem, insight
    );

    let response = llm_manager.call_unified_model_1(
        vec![], // No images
        &prompt,
        "数学", // Default subject
        Some("Tag generation task")
    ).await?;
    
    // Extract tags from OCR text or use default tags
    let tags = if !response.tags.is_empty() {
        response.tags
    } else {
        vec!["数学".to_string(), "解题".to_string()]
    };
    
    Ok(tags)
}

// Tag management commands
#[tauri::command]
pub async fn create_tag(
    request: CreateTagRequest,
    graph_state: State<'_, Arc<RwLock<GraphState>>>,
) -> Result<String, String> {
    let state = graph_state.read().await;
    
    if let Some(ref graph_service) = state.graph_service {
        graph_service.create_tag(request).await
            .map_err(|e| format!("Failed to create tag: {}", e))
    } else {
        Err("Graph service not initialized".to_string())
    }
}

#[tauri::command]
pub async fn get_tag_hierarchy(
    root_tag_id: Option<String>,
    graph_state: State<'_, Arc<RwLock<GraphState>>>,
) -> Result<Vec<TagHierarchy>, String> {
    let state = graph_state.read().await;
    
    if let Some(ref graph_service) = state.graph_service {
        graph_service.get_tag_hierarchy(root_tag_id).await
            .map_err(|e| format!("Failed to get tag hierarchy: {}", e))
    } else {
        Err("Graph service not initialized".to_string())
    }
}

#[tauri::command]
pub async fn get_tags_by_type(
    tag_type: TagType,
    graph_state: State<'_, Arc<RwLock<GraphState>>>,
) -> Result<Vec<Tag>, String> {
    let state = graph_state.read().await;
    
    if let Some(ref graph_service) = state.graph_service {
        graph_service.get_tags_by_type(tag_type).await
            .map_err(|e| format!("Failed to get tags by type: {}", e))
    } else {
        Err("Graph service not initialized".to_string())
    }
}

#[tauri::command]
pub async fn initialize_default_tag_hierarchy(
    graph_state: State<'_, Arc<RwLock<GraphState>>>,
) -> Result<String, String> {
    let state = graph_state.read().await;
    
    if let Some(ref graph_service) = state.graph_service {
        // Create default hierarchy for mathematics
        let math_areas = vec![
            ("代数", TagType::KnowledgeArea, None, Some("代数学相关内容".to_string())),
            ("几何", TagType::KnowledgeArea, None, Some("几何学相关内容".to_string())),
            ("解析几何", TagType::KnowledgeArea, None, Some("解析几何相关内容".to_string())),
            ("概率统计", TagType::KnowledgeArea, None, Some("概率与统计相关内容".to_string())),
        ];

        let mut created_ids: Vec<(String, String)> = Vec::new();
        
        // Create knowledge areas
        for (name, tag_type, parent_id, description) in math_areas {
            let request = CreateTagRequest {
                name: name.to_string(),
                tag_type,
                parent_id: parent_id.clone(),
                description,
            };
            
            match graph_service.create_tag(request).await {
                Ok(id) => {
                    created_ids.push((name.to_string(), id));
                },
                Err(e) => eprintln!("Failed to create tag {}: {}", name, e),
            }
        }

        // Create some topics under algebra
        if let Some((_, algebra_id)) = created_ids.iter().find(|(name, _)| name == "代数") {
            let algebra_topics = vec![
                ("函数", TagType::Topic, Some("函数相关概念和方法".to_string())),
                ("方程", TagType::Topic, Some("方程求解方法".to_string())),
                ("不等式", TagType::Topic, Some("不等式相关内容".to_string())),
            ];

            for (name, tag_type, description) in algebra_topics {
                let request = CreateTagRequest {
                    name: name.to_string(),
                    tag_type,
                    parent_id: Some(algebra_id.clone()),
                    description,
                };
                
                if let Err(e) = graph_service.create_tag(request).await {
                    eprintln!("Failed to create topic {}: {}", name, e);
                }
            }
        }

        Ok(format!("Created default tag hierarchy with {} top-level areas", created_ids.len()))
    } else {
        Err("Graph service not initialized".to_string())
    }
}