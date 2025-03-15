"""
数据库迁移脚本 - 添加多学科支持
将error_problems表更新以支持多学科错题管理
"""
import os
import sys
import sqlite3
import datetime
import logging
import traceback
import shutil

# 设置日志
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        logging.FileHandler("database_migration.log"),
        logging.StreamHandler()
    ]
)
logger = logging.getLogger("db_migration")

# 导入数据库路径
sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from 主程序.config import DATABASE_PATH
from 主程序.core.database import backup_database

def check_table_exists(cursor, table_name):
    """检查表是否存在"""
    cursor.execute(
        "SELECT name FROM sqlite_master WHERE type='table' AND name=?", 
        (table_name,)
    )
    return cursor.fetchone() is not None

def check_column_exists(cursor, table_name, column_name):
    """检查列是否存在"""
    try:
        cursor.execute(f"SELECT {column_name} FROM {table_name} LIMIT 1")
        return True
    except sqlite3.OperationalError:
        return False

def create_backup():
    """创建数据库备份"""
    timestamp = datetime.datetime.now().strftime("%Y%m%d_%H%M")
    backup_path = os.path.join(os.path.dirname(DATABASE_PATH), f"pre_multisubject_backup_{timestamp}.db")
    
    # 首先尝试使用现有备份函数
    try:
        success = backup_database(backup_path)
        if success:
            logger.info(f"使用内置备份函数创建备份成功: {backup_path}")
            return backup_path, True
    except Exception as e:
        logger.warning(f"使用内置备份函数失败: {str(e)}")
    
    # 如果内置备份失败，使用直接文件复制
    try:
        shutil.copy2(DATABASE_PATH, backup_path)
        logger.info(f"使用文件复制创建备份成功: {backup_path}")
        return backup_path, True
    except Exception as e:
        logger.error(f"创建备份失败: {str(e)}")
        return None, False

def add_subject_column():
    """添加subject列到error_problems表"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        cursor = conn.cursor()
        
        # 检查表是否存在
        if not check_table_exists(cursor, "error_problems"):
            logger.error("error_problems表不存在，无法添加列")
            conn.close()
            return False
        
        # 检查subject列是否已存在
        if check_column_exists(cursor, "error_problems", "subject"):
            logger.info("subject列已存在，无需添加")
            conn.close()
            return True
        
        # 添加subject列，默认值为'math'
        cursor.execute("ALTER TABLE error_problems ADD COLUMN subject TEXT DEFAULT 'math'")
        logger.info("成功添加subject列到error_problems表")
        
        # 为新列创建索引以提高查询性能
        cursor.execute("CREATE INDEX IF NOT EXISTS idx_error_problems_subject ON error_problems(subject)")
        logger.info("成功为subject列创建索引")
        
        # 确保所有现有记录都设置了subject值
        cursor.execute("UPDATE error_problems SET subject = 'math' WHERE subject IS NULL")
        updated_rows = cursor.rowcount
        logger.info(f"更新了 {updated_rows} 条记录的subject值为'math'")
        
        conn.commit()
        conn.close()
        return True
    except Exception as e:
        logger.error(f"添加subject列失败: {str(e)}\n{traceback.format_exc()}")
        return False

def verify_migration():
    """验证迁移是否成功"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        cursor = conn.cursor()
        
        # 验证列是否存在
        if not check_column_exists(cursor, "error_problems", "subject"):
            logger.error("验证失败: subject列不存在")
            conn.close()
            return False
        
        # 验证索引是否存在
        cursor.execute(
            "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_error_problems_subject'"
        )
        if cursor.fetchone() is None:
            logger.warning("验证警告: subject列的索引不存在")
        
        # 验证所有记录都有subject值
        cursor.execute("SELECT COUNT(*) FROM error_problems WHERE subject IS NULL")
        null_count = cursor.fetchone()[0]
        if null_count > 0:
            logger.warning(f"验证警告: 有 {null_count} 条记录的subject值为NULL")
        
        logger.info("迁移验证完成")
        conn.close()
        return True
    except Exception as e:
        logger.error(f"验证迁移失败: {str(e)}")
        return False

def rollback(backup_path):
    """回滚到备份"""
    try:
        if not os.path.exists(backup_path):
            logger.error(f"备份文件不存在: {backup_path}")
            return False
            
        # 复制备份文件回原位置
        shutil.copy2(backup_path, DATABASE_PATH)
        logger.info(f"成功回滚到备份: {backup_path}")
        return True
    except Exception as e:
        logger.error(f"回滚失败: {str(e)}")
        return False

def main():
    """主函数"""
    logger.info("==== 开始数据库迁移: 添加多学科支持 ====")
    
    # 1. 创建备份
    logger.info("步骤1: 创建数据库备份")
    backup_path, backup_success = create_backup()
    if not backup_success:
        logger.error("创建备份失败，中止迁移")
        return False
    
    # 2. 添加subject列
    logger.info("步骤2: 添加subject列到error_problems表")
    column_added = add_subject_column()
    if not column_added:
        logger.error("添加subject列失败")
        if input("是否回滚到备份? (y/n): ").lower() == 'y':
            rollback(backup_path)
        return False
    
    # 3. 验证迁移
    logger.info("步骤3: 验证迁移")
    verified = verify_migration()
    if not verified:
        logger.error("迁移验证失败")
        if input("是否回滚到备份? (y/n): ").lower() == 'y':
            rollback(backup_path)
        return False
    
    logger.info("==== 数据库迁移完成: 成功添加多学科支持 ====")
    logger.info(f"备份文件位置: {backup_path}")
    return True

if __name__ == "__main__":
    success = main()
    if success:
        print("数据库迁移成功完成！")
    else:
        print("数据库迁移失败，请查看日志获取详细信息。")
