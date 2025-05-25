"""
数据库迁移脚本
运行migrations目录下的所有SQL文件
"""
import sys
import os
import asyncio
from pathlib import Path

# Add project root to sys.path to allow importing 'shared'
project_root = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', '..'))
if project_root not in sys.path:
    sys.path.insert(0, project_root)

from sqlalchemy import text
from shared.config import settings
from shared.utils.logger import get_logger
from shared.database import engine

logger = get_logger(__name__)


async def run_migrations():
    """运行所有数据库迁移"""
    try:
        # 添加这行调试代码
        logger.info(f"DEBUG: settings.DATABASE_URL in migrate.py is: {settings.DATABASE_URL}")
        
        # 获取迁移文件目录
        migrations_dir = Path(__file__).parent / "migrations"
        
        # 获取所有SQL文件并排序
        sql_files = sorted(migrations_dir.glob("*.sql"))
        
        logger.info(f"Found {len(sql_files)} migration files")
        
        # 使用SQLAlchemy引擎执行迁移
        async with engine.begin() as conn:
            for sql_file in sql_files:
                logger.info(f"Running migration: {sql_file.name}")
                
                # 读取SQL内容
                with open(sql_file, "r", encoding="utf-8") as f:
                    sql_content = f.read()
                
                # 如果是SQLite，需要调整SQL语法
                if settings.DATABASE_URL.startswith("sqlite"):
                    sql_content = adapt_sql_for_sqlite(sql_content)
                
                # 分割SQL语句（以分号分隔）
                statements = [s.strip() for s in sql_content.split(';') if s.strip()]
                
                # 执行每个语句
                for statement in statements:
                    try:
                        # 检查是否为孤立的 "ALTER TABLE <table_name>" 语句 (更精确的检查)
                        # 移除所有注释并清理语句，然后判断
                        lines_of_statement_no_comments = []
                        for line_in_stmt in statement.strip().splitlines():
                            line_no_inline_comment = line_in_stmt.split('--')[0].strip() # 移除行内 '--' 注释
                            if line_no_inline_comment: # 仅当移除行内注释后非空时添加
                                lines_of_statement_no_comments.append(line_no_inline_comment)
                        
                        # TODO: 更完善地处理 /* ... */ 块注释 (如果需要)
                        full_cleaned_statement_for_check = " ".join(lines_of_statement_no_comments).strip()

                        if full_cleaned_statement_for_check: # 确保清理后的语句不为空
                            s_parts = full_cleaned_statement_for_check.lower().split()
                            # 只有当清理后的完整SQL语句确实只有 "alter table <name>" 三个部分时才跳过
                            if len(s_parts) == 3 and s_parts[0] == "alter" and s_parts[1] == "table":
                                logger.warning(
                                    f"Skipping statement whose effective SQL after comment removal is only 'ALTER TABLE {s_parts[2]}': '{statement.strip()}'"
                                )
                                continue
                        
                        await conn.execute(text(statement))
                    except Exception as e:
                        logger.warning(f"Statement failed (may be OK if already exists or column duplicate): {e}")
                        # 对于SQLite，某些错误可以忽略
                        if settings.DATABASE_URL.startswith("sqlite"):
                            error_lower = str(e).lower()
                            # "already exists" for tables/indexes
                            # "duplicate column name" for ADD COLUMN
                            if "already exists" in error_lower or "duplicate column name" in error_lower:
                                pass  # Treat as non-fatal for SQLite
                            else:
                                raise # Other SQLite errors are fatal
                        else: # For non-SQLite databases
                            raise # Re-raise all errors for other DBs
                
                logger.info(f"Successfully ran migration: {sql_file.name}")
        
        logger.info("All migrations completed successfully")
        
    except Exception as e:
        logger.error(f"Migration failed: {e}")
        raise


def adapt_sql_for_sqlite(sql_content: str) -> str:
    """将PostgreSQL SQL语法转换为SQLite兼容语法"""
    # SQLite不支持的特性替换
    # Order matters for some of these: more specific patterns first.
    replacements = {
        # 1. Specific DEFAULT ARRAY[] constructs. Most specific first.
        "DEFAULT ARRAY[]::TEXT[]": "DEFAULT '[]'",
        "DEFAULT ARRAY[]::TEXT": "DEFAULT '[]'",
        "DEFAULT ARRAY[]::VARCHAR[]": "DEFAULT '[]'",
        "DEFAULT ARRAY[]::VARCHAR": "DEFAULT '[]'",
        "DEFAULT ARRAY[]::JSONB": "DEFAULT '[]'",
        "DEFAULT ARRAY[]::JSON": "DEFAULT '[]'",
        "DEFAULT ARRAY[]::UUID[]": "DEFAULT '[]'",
        "DEFAULT ARRAY[]::UUID": "DEFAULT '[]'",
        "DEFAULT ARRAY[]": "DEFAULT '[]'",      # General fallback for DEFAULT ARRAY[]

        # 2. Handle TYPE[] column type conversions
        "TEXT[]": "TEXT",
        "UUID[]": "TEXT",
        "VARCHAR(50)[]": "TEXT",
        "VARCHAR(200)[]": "TEXT",
        "INTEGER[]": "TEXT",
        "FLOAT[]": "TEXT",
        "BOOLEAN[]": "TEXT",
        "JSON[]": "JSON", 
        "JSONB[]": "JSON",

        # 3. Handle general type name changes (Postgres to SQLite)
        "UUID PRIMARY KEY DEFAULT gen_random_uuid()": "TEXT PRIMARY KEY", # Before general UUID -> TEXT
        "UUID": "TEXT",
        "SERIAL": "INTEGER",
        "BIGSERIAL": "INTEGER",
        "JSONB": "JSON",
        "BOOLEAN": "INTEGER",
        "TIMESTAMP WITH TIME ZONE": "TIMESTAMP",
        "CURRENT_TIMESTAMP": "CURRENT_TIMESTAMP", # SQLite supports this

        # 4. Remove ::casts (case variations for robustness). Applied after DEFAULT rules.
        "::TEXT[]": "", "::text[]": "",
        "::TEXT": "",   "::text": "",
        "::VARCHAR[]": "", "::varchar[]": "",
        "::VARCHAR": "",   "::varchar": "",
        "::JSONB": "", "::jsonb": "",
        "::JSON": "",  "::json": "",
        "::UUID[]": "","::uuid[]": "",
        "::UUID": "",  "::uuid": "",
        "::INTEGER[]": "", "::integer[]": "",
        "::INTEGER": "",   "::integer": "",
        "::FLOAT[]": "", "::float[]": "",
        "::FLOAT": "",   "::float": "",
        "::BOOLEAN[]": "", "::boolean[]": "",
        "::BOOLEAN": "",   "::boolean": "",

        # 5. Default boolean and other value conversions
        "DEFAULT true": "DEFAULT 1",
        "DEFAULT false": "DEFAULT 0",
        " true": " 1", 
        " false": " 0",
        "gen_random_uuid()": "''", # Should be after UUID PK def

        # Specific ALTER TABLE modifications for SQLite
        "ADD COLUMN IF NOT EXISTS": "ADD COLUMN", # SQLite doesn't support IF NOT EXISTS here
    }
    
    for old, new in replacements.items():
        sql_content = sql_content.replace(old, new)
    
    # 移除SQLite不支持的语句
    lines = sql_content.split('\n')
    filtered_lines = []
    skip_next = False
    
    for line in lines:
        line_lower = line.lower().strip()
        
        # 跳过不支持的语句
        if any(keyword in line_lower for keyword in [
            "create extension",
            "create index using gin", # Keep for specificity if ever needed
            "using gin",              # Add this to catch lines with GIN index type
            "comment on",             # Add this to remove COMMENT ON statements
            "alter column",           # Add this to skip ALTER COLUMN statements (limited SQLite support)
            "add constraint",         # Add this to skip ADD CONSTRAINT on existing tables (limited SQLite support)
            "do $$",                  # Skip start of DO $$ blocks
            "end $$",                 # Skip end of DO $$ blocks
            "create trigger",
            "execute function",
            "create function",
            "plpgsql"
        ]):
            skip_next = True
            continue
        
        if skip_next and ';' in line:
            skip_next = False
            continue
        
        if not skip_next:
            filtered_lines.append(line)
    
    return '\n'.join(filtered_lines)


async def check_database():
    """检查数据库连接和表结构"""
    try:
        async with engine.connect() as conn:
            if settings.DATABASE_URL.startswith("sqlite"):
                # SQLite查询表
                result = await conn.execute(text(
                    "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
                ))
                tables = [row[0] for row in result]
            else:
                # PostgreSQL查询表
                result = await conn.execute(text("""
                    SELECT tablename 
                    FROM pg_tables 
                    WHERE schemaname = 'public'
                    ORDER BY tablename
                """))
                tables = [row[0] for row in result]
            
            logger.info("Existing tables:")
            for table in tables:
                logger.info(f"  - {table}")
            
            # 检查表数量
            expected_tables = [
                "ai_models", "ai_call_logs",
                "problems", "review_records", "problem_templates",
                "review_analyses", "analysis_follow_ups", "learning_patterns",
                "file_records"
            ]
            
            missing_tables = [t for t in expected_tables if t not in tables]
            
            if missing_tables:
                logger.warning(f"Missing tables: {missing_tables}")
            else:
                logger.info("All expected tables exist")
                
    except Exception as e:
        logger.error(f"Database check failed: {e}")
        raise


if __name__ == "__main__":
    # 运行迁移
    asyncio.run(run_migrations())
    
    # 检查结果
    asyncio.run(check_database())
