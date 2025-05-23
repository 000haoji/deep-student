-- AI模型配置表
CREATE TABLE IF NOT EXISTS ai_models (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider VARCHAR(50) NOT NULL,
    model_name VARCHAR(100) NOT NULL,
    api_key_encrypted VARCHAR(500) NOT NULL,
    api_url VARCHAR(500) NOT NULL,
    priority INTEGER DEFAULT 1,
    is_active BOOLEAN DEFAULT true,
    capabilities JSONB NOT NULL,
    cost_per_1k_tokens FLOAT DEFAULT 0.0,
    max_tokens INTEGER DEFAULT 4096,
    timeout INTEGER DEFAULT 30,
    max_retries INTEGER DEFAULT 3,
    custom_headers JSONB,
    
    -- 统计字段
    total_requests INTEGER DEFAULT 0,
    successful_requests INTEGER DEFAULT 0,
    failed_requests INTEGER DEFAULT 0,
    total_tokens_used INTEGER DEFAULT 0,
    total_cost FLOAT DEFAULT 0.0,
    average_response_time FLOAT DEFAULT 0.0,
    last_used_at TIMESTAMP,
    last_error VARCHAR(500),
    last_error_at TIMESTAMP,
    
    -- 时间戳
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    
    -- 唯一约束
    UNIQUE(provider, model_name)
);

-- AI调用日志表
CREATE TABLE IF NOT EXISTS ai_call_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    model_id UUID REFERENCES ai_models(id),
    task_type VARCHAR(50) NOT NULL,
    request_content JSONB NOT NULL,
    response_content JSONB,
    success BOOLEAN NOT NULL,
    error_message TEXT,
    tokens_used INTEGER DEFAULT 0,
    cost FLOAT DEFAULT 0.0,
    response_time FLOAT,
    retry_count INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- 索引
CREATE INDEX idx_ai_models_provider ON ai_models(provider);
CREATE INDEX idx_ai_models_active ON ai_models(is_active);
CREATE INDEX idx_ai_call_logs_model_id ON ai_call_logs(model_id);
CREATE INDEX idx_ai_call_logs_task_type ON ai_call_logs(task_type);
CREATE INDEX idx_ai_call_logs_created_at ON ai_call_logs(created_at); 