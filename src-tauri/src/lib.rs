// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod models;
pub mod llm_manager;
pub mod analysis_service;
pub mod commands;
pub mod crypto;
pub mod database;
pub mod file_manager;
pub mod batch_operations;
pub mod database_optimizations;
pub mod anki_connect_service;
pub mod apkg_exporter_service;
pub mod document_processing_service;
pub mod streaming_anki_service;
pub mod enhanced_anki_service;
pub mod image_occlusion_service;
pub mod vector_store;
pub mod rag_manager;
pub mod document_parser;
pub mod gemini_adapter;
pub mod cogni_graph;

pub use commands::{
    AppState, TempSession,
    analyze_new_mistake, analyze_new_mistake_stream, continue_chat, continue_chat_stream, save_mistake_from_analysis,
    get_mistakes, get_review_analyses, delete_review_analysis, get_mistake_details, update_mistake, delete_mistake, continue_mistake_chat, continue_mistake_chat_stream, continue_mistake_chat_stream_v2,
    analyze_review_session_stream, continue_review_chat, continue_review_chat_stream,
    get_statistics, save_setting, get_setting,
    test_api_connection, get_supported_subjects,
    get_image_as_base64, save_image_from_base64_path, cleanup_orphaned_images, get_image_statistics,
    get_api_configurations, save_api_configurations, get_model_assignments, save_model_assignments,
    analyze_step_by_step, start_streaming_answer, get_model_adapter_options, save_model_adapter_options, reset_model_adapter_options,
    // 科目配置管理
    get_all_subject_configs, get_subject_config_by_id, get_subject_config_by_name,
    create_subject_config, update_subject_config, delete_subject_config, initialize_default_subject_configs,
    // 批量操作
    batch_delete_mistakes, batch_update_mistake_statuses, batch_update_mistake_tags,
    batch_cleanup_database, batch_export_mistakes, batch_save_mistakes,
    // 数据库优化
    get_mistakes_optimized, get_tag_statistics_optimized, search_mistakes_fulltext,
    get_mistakes_by_date_range, create_performance_indexes, analyze_query_performance,
    // 回顾分析功能
    start_consolidated_review_analysis, trigger_consolidated_review_stream, continue_consolidated_review_stream,
    get_consolidated_review_session, get_review_analysis_by_id,
    // ANKI制卡功能
    generate_anki_cards_from_document,
    generate_anki_cards_from_document_file,
    generate_anki_cards_from_document_base64,
    // AnkiConnect集成功能
    check_anki_connect_status, get_anki_deck_names, get_anki_model_names, 
    create_anki_deck, add_cards_to_anki_connect,
    // APKG导出功能
    export_cards_as_apkg, export_cards_as_apkg_with_template,
    // 增强ANKI功能
    start_enhanced_document_processing, trigger_task_processing,
    get_document_tasks, get_task_cards, update_anki_card,
    delete_anki_card, delete_document_task, delete_document_session, 
    export_apkg_for_selection, get_document_cards,
    // RAG知识库管理功能
    rag_add_documents, rag_add_documents_from_content, rag_get_knowledge_base_status, rag_delete_document,
    rag_query_knowledge_base, rag_get_all_documents, rag_clear_knowledge_base,
    // RAG增强的AI分析功能
    start_rag_enhanced_streaming_answer, continue_rag_enhanced_chat_stream,
    // 独立RAG查询功能
    llm_generate_answer_with_context,
    // RAG配置管理功能
    get_rag_settings, update_rag_settings, reset_rag_settings,
    // RAG分库管理功能
    create_rag_sub_library, get_rag_sub_libraries, get_rag_sub_library_by_id,
    update_rag_sub_library, delete_rag_sub_library, rag_add_documents_to_library,
    rag_add_documents_from_content_to_library, get_rag_documents_by_library,
    move_document_to_rag_library, rag_query_knowledge_base_in_libraries,
    // 文档解析功能
    parse_document_from_path, parse_document_from_base64,
    // 错题总结生成功能
    generate_mistake_summary,
    // 自定义模板管理功能
    get_all_custom_templates, get_custom_template_by_id, create_custom_template,
    update_custom_template, delete_custom_template, export_template, import_template,
    // 图片遮罩卡功能
    extract_image_text_coordinates, create_image_occlusion_card, get_all_image_occlusion_cards,
    get_image_occlusion_card, update_image_occlusion_card, delete_image_occlusion_card,
    // 默认模板管理功能
    set_default_template, get_default_template_id,
    // 测试日志相关功能
    save_test_log, get_test_logs, open_log_file, open_logs_folder
};

// CogniGraph模块导入
pub use cogni_graph::handlers::{
    initialize_knowledge_graph, create_problem_card, get_problem_card, search_knowledge_graph,
    get_ai_recommendations, search_similar_cards, get_cards_by_tag, get_all_tags,
    get_graph_config, update_graph_config, test_neo4j_connection, process_handwritten_input,
    // Tag管理功能
    create_tag, get_tag_hierarchy, get_tags_by_type, initialize_default_tag_hierarchy
};
use analysis_service::AnalysisService;
use database::Database;
use file_manager::FileManager;
use llm_manager::LLMManager;
use cogni_graph::handlers::GraphState;
use std::sync::Arc;
use std::collections::HashMap;
use tauri::Manager;
use tokio::sync::RwLock;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // 获取系统级应用数据目录
            let app_handle = app.handle().clone();
            let app_data_dir = app_handle.path()
                .app_data_dir()
                .expect("无法获取应用数据目录")
                .join("ai-mistake-manager");
            // 确保目录存在
            std::fs::create_dir_all(&app_data_dir).expect("无法创建应用数据目录");

            // 初始化文件管理器
            let file_manager = Arc::new(
                FileManager::new(app_data_dir.clone())
                    .expect("Failed to initialize file manager")
            );

            // 初始化数据库
            let database_path = file_manager.get_database_path();
            let database = Arc::new(
                Database::new(&database_path)
                    .expect("Failed to initialize database")
            );

            // 初始化默认科目配置
            if let Err(e) = database.initialize_default_subject_configs() {
                println!("警告：初始化默认科目配置失败: {}", e);
            }

            // 初始化LLM管理器
            let llm_manager = Arc::new(LLMManager::new(database.clone(), file_manager.clone()));

            // 初始化分析服务
            let analysis_service = Arc::new(AnalysisService::new(database.clone(), file_manager.clone()));

            // 初始化临时会话存储
            let temp_sessions: Arc<tokio::sync::Mutex<HashMap<String, TempSession>>> = 
                Arc::new(tokio::sync::Mutex::new(HashMap::new()));

            // 初始化回顾分析会话存储
            let review_sessions: Arc<tokio::sync::Mutex<HashMap<String, models::ConsolidatedReviewSession>>> = 
                Arc::new(tokio::sync::Mutex::new(HashMap::new()));

            // 初始化RAG管理器
            let rag_manager = Arc::new(
                rag_manager::RagManager::new(database.clone(), llm_manager.clone(), file_manager.clone())
                    .expect("Failed to initialize RAG manager")
            );

            // 初始化图片遮罩服务
            let image_occlusion_service = Arc::new(
                image_occlusion_service::ImageOcclusionService::new(database.clone(), llm_manager.clone())
            );

            // 初始化CogniGraph状态
            let graph_state = Arc::new(RwLock::new(GraphState::new()));

            let app_state = AppState {
                analysis_service,
                database,
                file_manager,
                temp_sessions,
                llm_manager: llm_manager.clone(),
                review_sessions,
                rag_manager,
                image_occlusion_service,
            };
            // 将状态注册到Tauri应用
            app.manage(app_state);
            app.manage(graph_state);
            app.manage(llm_manager);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            // 错题分析相关
            analyze_new_mistake,
            analyze_new_mistake_stream,
            continue_chat,
            continue_chat_stream,
            save_mistake_from_analysis,
            // 分步骤分析
            analyze_step_by_step,
            start_streaming_answer,
            // 错题库管理
            get_mistakes,
            get_review_analyses,
            delete_review_analysis,
            get_mistake_details,
            update_mistake,
            delete_mistake,
            continue_mistake_chat,
            continue_mistake_chat_stream,
            continue_mistake_chat_stream_v2,
            // 回顾分析
            analyze_review_session_stream,
            continue_review_chat,
            continue_review_chat_stream,
            // 统计和设置
            get_statistics,
            save_setting,
            get_setting,
            test_api_connection,
            get_supported_subjects,
            // 专用配置管理
            get_api_configurations,
            save_api_configurations,
            get_model_assignments,
            save_model_assignments,
            get_model_adapter_options,
            save_model_adapter_options,
            reset_model_adapter_options,
            // 文件管理
            get_image_as_base64,
            save_image_from_base64_path,
            cleanup_orphaned_images,
            get_image_statistics,
            // 科目配置管理
            get_all_subject_configs,
            get_subject_config_by_id,
            get_subject_config_by_name,
            create_subject_config,
            update_subject_config,
            delete_subject_config,
            initialize_default_subject_configs,
            // 批量操作
            batch_delete_mistakes,
            batch_update_mistake_statuses,
            batch_update_mistake_tags,
            batch_cleanup_database,
            batch_export_mistakes,
            batch_save_mistakes,
            // 数据库优化
            get_mistakes_optimized,
            get_tag_statistics_optimized,
            search_mistakes_fulltext,
            get_mistakes_by_date_range,
            create_performance_indexes,
            analyze_query_performance,
            // 回顾分析功能
            start_consolidated_review_analysis,
            trigger_consolidated_review_stream,
            continue_consolidated_review_stream,
            get_consolidated_review_session,
            get_review_analysis_by_id,
            // ANKI制卡功能
            generate_anki_cards_from_document,
            generate_anki_cards_from_document_file,
            generate_anki_cards_from_document_base64,
            // AnkiConnect集成功能
            check_anki_connect_status,
            get_anki_deck_names,
            get_anki_model_names,
            create_anki_deck,
            add_cards_to_anki_connect,
            // APKG导出功能
            export_cards_as_apkg,
            export_cards_as_apkg_with_template,
            // 增强ANKI功能
            start_enhanced_document_processing,
            trigger_task_processing,
            get_document_tasks,
            get_task_cards,
            update_anki_card,
            delete_anki_card,
            delete_document_task,
            delete_document_session,
            export_apkg_for_selection,
            get_document_cards,
            // RAG知识库管理功能
            rag_add_documents,
            rag_add_documents_from_content,
            rag_get_knowledge_base_status,
            rag_delete_document,
            rag_query_knowledge_base,
            rag_get_all_documents,
            rag_clear_knowledge_base,
            // RAG增强的AI分析功能
            start_rag_enhanced_streaming_answer,
            continue_rag_enhanced_chat_stream,
            // 独立RAG查询功能
            llm_generate_answer_with_context,
            // RAG配置管理功能
            get_rag_settings,
            update_rag_settings,
            reset_rag_settings,
            // RAG分库管理功能
            create_rag_sub_library,
            get_rag_sub_libraries,
            get_rag_sub_library_by_id,
            update_rag_sub_library,
            delete_rag_sub_library,
            rag_add_documents_to_library,
            rag_add_documents_from_content_to_library,
            get_rag_documents_by_library,
            move_document_to_rag_library,
            rag_query_knowledge_base_in_libraries,
            // 文档解析功能
            parse_document_from_path,
            parse_document_from_base64,
            // 错题总结生成
            generate_mistake_summary,
            // 自定义模板管理功能
            get_all_custom_templates,
            get_custom_template_by_id,
            create_custom_template,
            update_custom_template,
            delete_custom_template,
            export_template,
            import_template,
            // 图片遮罩卡功能
            extract_image_text_coordinates,
            create_image_occlusion_card,
            get_all_image_occlusion_cards,
            get_image_occlusion_card,
            update_image_occlusion_card,
            delete_image_occlusion_card,
            // 默认模板管理功能
            set_default_template,
            get_default_template_id,
            // 测试日志相关功能
            save_test_log,
            get_test_logs,
            open_log_file,
            open_logs_folder,
            // CogniGraph知识图谱功能
            initialize_knowledge_graph,
            create_problem_card,
            get_problem_card,
            search_knowledge_graph,
            get_ai_recommendations,
            search_similar_cards,
            get_cards_by_tag,
            get_all_tags,
            get_graph_config,
            update_graph_config,
            test_neo4j_connection,
            process_handwritten_input,
            // Tag管理功能
            create_tag,
            get_tag_hierarchy,
            get_tags_by_type,
            initialize_default_tag_hierarchy
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
