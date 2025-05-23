-- 回顾分析相关表

-- 回顾分析记录表
CREATE TABLE IF NOT EXISTS review_analyses (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- 分析基本信息
    title VARCHAR(200) NOT NULL,
    analysis_type VARCHAR(50) NOT NULL,
    description TEXT,
    
    -- 分析范围
    problem_ids UUID[] DEFAULT ARRAY[]::UUID[],
    subjects VARCHAR(50)[] DEFAULT ARRAY[]::VARCHAR[],
    knowledge_points VARCHAR(200)[] DEFAULT ARRAY[]::VARCHAR[],
    date_range JSONB,
    
    -- AI分析结果
    ai_analysis JSONB NOT NULL,
    
    -- 关键发现
    error_patterns JSONB,
    weakness_areas JSONB,
    improvement_suggestions JSONB,
    study_plan JSONB,
    
    -- 统计数据
    total_problems INTEGER DEFAULT 0,
    avg_difficulty FLOAT,
    avg_mastery FLOAT,
    common_mistakes JSONB,
    
    -- 评分和优先级
    importance_score FLOAT CHECK (importance_score >= 0 AND importance_score <= 10),
    urgency_score FLOAT CHECK (urgency_score >= 0 AND urgency_score <= 10),
    
    -- 时间戳
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 分析后续跟进记录表
CREATE TABLE IF NOT EXISTS analysis_follow_ups (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    analysis_id UUID NOT NULL REFERENCES review_analyses(id) ON DELETE CASCADE,
    
    -- 跟进信息
    action_taken TEXT NOT NULL,
    result TEXT,
    effectiveness_score FLOAT CHECK (effectiveness_score >= 0 AND effectiveness_score <= 10),
    notes TEXT,
    
    -- 相关题目
    reviewed_problem_ids UUID[] DEFAULT ARRAY[]::UUID[],
    new_mastery_levels JSONB,
    
    -- 时间戳
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 学习模式记录表
CREATE TABLE IF NOT EXISTS learning_patterns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- 模式信息
    pattern_name VARCHAR(100) NOT NULL,
    pattern_type VARCHAR(50),
    description TEXT,
    
    -- 数据支持
    supporting_data JSONB NOT NULL,
    confidence_score FLOAT CHECK (confidence_score >= 0 AND confidence_score <= 1),
    
    -- 适用范围
    applicable_subjects VARCHAR(50)[] DEFAULT ARRAY[]::VARCHAR[],
    applicable_knowledge_points VARCHAR(200)[] DEFAULT ARRAY[]::VARCHAR[],
    
    -- 建议
    recommendations JSONB,
    
    -- 时间戳
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 创建索引
CREATE INDEX idx_review_analyses_type ON review_analyses(analysis_type);
CREATE INDEX idx_review_analyses_created_at ON review_analyses(created_at DESC);
CREATE INDEX idx_review_analyses_importance ON review_analyses(importance_score DESC);
CREATE INDEX idx_review_analyses_urgency ON review_analyses(urgency_score DESC);

CREATE INDEX idx_analysis_follow_ups_analysis_id ON analysis_follow_ups(analysis_id);
CREATE INDEX idx_analysis_follow_ups_created_at ON analysis_follow_ups(created_at DESC);

CREATE INDEX idx_learning_patterns_type ON learning_patterns(pattern_type);
CREATE INDEX idx_learning_patterns_confidence ON learning_patterns(confidence_score DESC);

-- 创建触发器更新updated_at
CREATE TRIGGER update_review_analyses_updated_at BEFORE UPDATE
    ON review_analyses FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_analysis_follow_ups_updated_at BEFORE UPDATE
    ON analysis_follow_ups FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_learning_patterns_updated_at BEFORE UPDATE
    ON learning_patterns FOR EACH ROW EXECUTE FUNCTION update_updated_at_column(); 