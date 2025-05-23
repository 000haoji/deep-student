-- 创建用户表
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username VARCHAR(50) UNIQUE NOT NULL,
    email VARCHAR(100) UNIQUE NOT NULL,
    full_name VARCHAR(100) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(20) NOT NULL DEFAULT 'student',
    is_active BOOLEAN NOT NULL DEFAULT true,
    is_verified BOOLEAN NOT NULL DEFAULT false,
    avatar_url VARCHAR(500),
    bio VARCHAR(500),
    phone VARCHAR(20),
    grade VARCHAR(20),
    school VARCHAR(100),
    major VARCHAR(50),
    last_login_at TIMESTAMP WITH TIME ZONE,
    login_count INTEGER DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 创建索引
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_role ON users(role);

-- 为problems表添加user_id外键（如果还没有）
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'problems' AND column_name = 'user_id'
    ) THEN
        ALTER TABLE problems ADD COLUMN user_id UUID;
        ALTER TABLE problems ADD CONSTRAINT fk_problems_user 
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
        CREATE INDEX idx_problems_user_id ON problems(user_id);
    END IF;
END $$;

-- 为review_analyses表添加user_id外键（如果还没有）
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'review_analyses' AND column_name = 'user_id'
    ) THEN
        ALTER TABLE review_analyses ADD COLUMN user_id UUID;
        ALTER TABLE review_analyses ADD CONSTRAINT fk_review_analyses_user 
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
        CREATE INDEX idx_review_analyses_user_id ON review_analyses(user_id);
    END IF;
END $$; 