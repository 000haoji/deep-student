-- 文件服务相关表

-- 文件记录表
CREATE TABLE IF NOT EXISTS file_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    
    -- 文件信息
    filename VARCHAR(255) NOT NULL,
    original_name VARCHAR(255),
    file_type VARCHAR(20) NOT NULL,
    mime_type VARCHAR(100),
    size INTEGER NOT NULL, -- 字节
    
    -- 存储信息
    bucket_name VARCHAR(100) NOT NULL,
    object_name VARCHAR(500) NOT NULL,
    storage_path VARCHAR(1000) NOT NULL,
    
    -- 访问信息
    access_url VARCHAR(1000),
    is_public BOOLEAN DEFAULT FALSE,
    
    -- 元数据
    category VARCHAR(50), -- 分类：problems, avatars, etc
    related_id VARCHAR(50), -- 关联的资源ID
    description VARCHAR(500),
    
    -- 软删除
    deleted_at TIMESTAMP WITH TIME ZONE,
    
    -- 时间戳
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP
);

-- 创建索引
CREATE INDEX idx_file_records_category ON file_records(category) WHERE deleted_at IS NULL;
CREATE INDEX idx_file_records_related_id ON file_records(related_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_file_records_file_type ON file_records(file_type) WHERE deleted_at IS NULL;
CREATE INDEX idx_file_records_created_at ON file_records(created_at DESC) WHERE deleted_at IS NULL;

-- 创建触发器更新updated_at
CREATE TRIGGER update_file_records_updated_at BEFORE UPDATE
    ON file_records FOR EACH ROW EXECUTE FUNCTION update_updated_at_column(); 