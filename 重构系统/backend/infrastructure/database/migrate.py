"""
数据库迁移脚本
运行migrations目录下的所有SQL文件
"""
import asyncio
from pathlib import Path
from sqlalchemy import text
from shared.config import settings
from shared.utils.logger import get_logger
from shared.database import engine

logger = get_logger(__name__)


async def run_migrations():
    """运行所有数据库迁移"""
    try:
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
                if settings.database_url.startswith("sqlite"):
                    sql_content = adapt_sql_for_sqlite(sql_content)
                
                # 分割SQL语句（以分号分隔）
                statements = [s.strip() for s in sql_content.split(';') if s.strip()]
                
                # 执行每个语句
                for statement in statements:
                    try:
                        await conn.execute(text(statement))
                    except Exception as e:
                        logger.warning(f"Statement failed (may be OK if already exists): {e}")
                        # 对于SQLite，某些错误可以忽略（如已存在的表）
                        if not settings.database_url.startswith("sqlite") or "already exists" not in str(e):
                            raise
                
                logger.info(f"Successfully ran migration: {sql_file.name}")
        
        logger.info("All migrations completed successfully")
        
    except Exception as e:
        logger.error(f"Migration failed: {e}")
        raise


def adapt_sql_for_sqlite(sql_content: str) -> str:
    """将PostgreSQL SQL语法转换为SQLite兼容语法"""
    # SQLite不支持的特性替换
    replacements = {
        "UUID PRIMARY KEY DEFAULT gen_random_uuid()": "TEXT PRIMARY KEY",  # SQLite没有自动生成UUID的功能
        "UUID": "TEXT",
        "gen_random_uuid()": "''",  # 空字符串作为默认值，应用层会生成UUID
        "SERIAL": "INTEGER",
        "BIGSERIAL": "INTEGER",
        "JSONB": "JSON",
        "BOOLEAN": "INTEGER",  # SQLite使用0/1表示布尔值
        "TIMESTAMP WITH TIME ZONE": "TIMESTAMP",
        "CURRENT_TIMESTAMP": "CURRENT_TIMESTAMP",
        "ARRAY": "JSON",  # 使用JSON代替数组
        "UUID[]": "JSON",
        "VARCHAR(50)[]": "JSON",
        "VARCHAR(200)[]": "JSON",
        "::jsonb": "",
        "::UUID[]": "",
        "::VARCHAR[]": "",
        # 布尔值转换
        " true": " 1",
        " false": " 0",
        "DEFAULT true": "DEFAULT 1",
        "DEFAULT false": "DEFAULT 0",
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
            "create index using gin",
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
            if settings.database_url.startswith("sqlite"):
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