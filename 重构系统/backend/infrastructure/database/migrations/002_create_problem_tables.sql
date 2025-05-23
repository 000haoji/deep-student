-- 错题管理相关表

-- 错题表
CREATE TABLE IF NOT EXISTS problems (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- 基本信息
    title VARCHAR(200) NOT NULL,
    content TEXT NOT NULL,
    subject VARCHAR(50) NOT NULL,
    category VARCHAR(100),
    source VARCHAR(200),
    
    -- 错误相关
    error_analysis TEXT,
    user_answer TEXT,
    correct_answer TEXT,
    solution TEXT,
    
    -- AI分析结果
    ai_analysis JSONB,
    knowledge_points JSONB DEFAULT '[]'::jsonb,
    difficulty_level INTEGER DEFAULT 3 CHECK (difficulty_level >= 1 AND difficulty_level <= 5),
    
    -- 图片相关
    image_urls JSONB DEFAULT '[]'::jsonb,
    ocr_result TEXT,
    
    -- 统计信息
    review_count INTEGER DEFAULT 0,
    last_review_at TIMESTAMP WITH TIME ZONE,
    mastery_level FLOAT DEFAULT 0.0 CHECK (mastery_level >= 0 AND mastery_level <= 1),
    
    -- 标签和备注
    tags JSONB DEFAULT '[]'::jsonb,
    notes TEXT,
    
    -- 软删除
    deleted_at TIMESTAMP WITH TIME ZONE,
    
    -- 时间戳
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 复习记录表
CREATE TABLE IF NOT EXISTS review_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    problem_id UUID NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
    
    -- 复习信息
    review_result VARCHAR(20) CHECK (review_result IN ('correct', 'incorrect', 'partial')),
    confidence_level INTEGER CHECK (confidence_level >= 1 AND confidence_level <= 5),
    time_spent INTEGER, -- 秒
    notes TEXT,
    
    -- AI反馈
    ai_feedback JSONB,
    
    -- 时间戳
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 题目模板表
CREATE TABLE IF NOT EXISTS problem_templates (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    subject VARCHAR(50) NOT NULL,
    template_data JSONB NOT NULL,
    description TEXT,
    usage_count INTEGER DEFAULT 0,
    
    -- 时间戳
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 创建索引
CREATE INDEX idx_problems_subject ON problems(subject) WHERE deleted_at IS NULL;
CREATE INDEX idx_problems_category ON problems(category) WHERE deleted_at IS NULL;
CREATE INDEX idx_problems_difficulty ON problems(difficulty_level) WHERE deleted_at IS NULL;
CREATE INDEX idx_problems_mastery ON problems(mastery_level) WHERE deleted_at IS NULL;
CREATE INDEX idx_problems_created_at ON problems(created_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX idx_problems_tags ON problems USING GIN (tags) WHERE deleted_at IS NULL;
CREATE INDEX idx_problems_knowledge_points ON problems USING GIN (knowledge_points) WHERE deleted_at IS NULL;

CREATE INDEX idx_review_records_problem_id ON review_records(problem_id);
CREATE INDEX idx_review_records_created_at ON review_records(created_at DESC);

-- 创建触发器更新updated_at
CREATE TRIGGER update_problems_updated_at BEFORE UPDATE
    ON problems FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_review_records_updated_at BEFORE UPDATE
    ON review_records FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_problem_templates_updated_at BEFORE UPDATE
    ON problem_templates FOR EACH ROW EXECUTE FUNCTION update_updated_at_column(); 