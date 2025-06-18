#!/usr/bin/env python3
"""
数据库修复脚本 - 添加缺失的总结字段
"""
import sqlite3
import os
import sys

DB_PATH = "/mnt/h/NEW-DS2/NEW-DS/ai-mistake-manager/src-tauri/app_data/ai-mistake-manager/mistakes.db"

def check_db_exists():
    if not os.path.exists(DB_PATH):
        print(f"❌ 数据库文件不存在: {DB_PATH}")
        return False
    print(f"✅ 数据库文件存在: {DB_PATH}")
    return True

def check_table_schema(cursor):
    """检查mistakes表的当前结构"""
    print("\n🔍 检查mistakes表结构...")
    try:
        cursor.execute("PRAGMA table_info(mistakes)")
        columns = cursor.fetchall()
        print(f"📊 mistakes表当前有 {len(columns)} 列:")
        
        column_names = []
        for col in columns:
            print(f"  - {col[1]} ({col[2]})")
            column_names.append(col[1])
        
        has_mistake_summary = 'mistake_summary' in column_names
        has_user_error_analysis = 'user_error_analysis' in column_names
        
        print(f"\n📋 总结字段检查:")
        print(f"  - mistake_summary: {'✅ 存在' if has_mistake_summary else '❌ 缺失'}")
        print(f"  - user_error_analysis: {'✅ 存在' if has_user_error_analysis else '❌ 缺失'}")
        
        return has_mistake_summary, has_user_error_analysis, column_names
        
    except sqlite3.Error as e:
        print(f"❌ 检查表结构失败: {e}")
        return False, False, []

def check_schema_version(cursor):
    """检查数据库版本"""
    try:
        cursor.execute("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1")
        result = cursor.fetchone()
        if result:
            version = result[0]
            print(f"📦 当前数据库版本: v{version}")
            return version
        else:
            print("⚠️ 未找到版本信息")
            return 0
    except sqlite3.Error as e:
        print(f"⚠️ 检查版本失败: {e}")
        return 0

def add_missing_columns(cursor):
    """添加缺失的列"""
    print("\n🔧 开始添加缺失的总结字段...")
    
    try:
        # 添加 mistake_summary 字段
        cursor.execute("ALTER TABLE mistakes ADD COLUMN mistake_summary TEXT")
        print("✅ 已添加 mistake_summary 字段")
    except sqlite3.Error as e:
        if "duplicate column" in str(e).lower():
            print("ℹ️ mistake_summary 字段已存在")
        else:
            print(f"❌ 添加 mistake_summary 字段失败: {e}")
    
    try:
        # 添加 user_error_analysis 字段
        cursor.execute("ALTER TABLE mistakes ADD COLUMN user_error_analysis TEXT")
        print("✅ 已添加 user_error_analysis 字段")
    except sqlite3.Error as e:
        if "duplicate column" in str(e).lower():
            print("ℹ️ user_error_analysis 字段已存在")
        else:
            print(f"❌ 添加 user_error_analysis 字段失败: {e}")

def update_schema_version(cursor):
    """更新数据库版本到v7"""
    try:
        cursor.execute("INSERT OR REPLACE INTO schema_version (version) VALUES (7)")
        print("✅ 已更新数据库版本到 v7")
    except sqlite3.Error as e:
        print(f"❌ 更新版本失败: {e}")

def check_mistakes_count(cursor):
    """检查错题数量"""
    try:
        cursor.execute("SELECT COUNT(*) FROM mistakes")
        count = cursor.fetchone()[0]
        print(f"📊 数据库中共有 {count} 条错题记录")
        
        if count > 0:
            cursor.execute("SELECT DISTINCT subject FROM mistakes")
            subjects = [row[0] for row in cursor.fetchall()]
            print(f"📚 涉及科目: {', '.join(subjects)}")
            
            # 检查化学科目
            cursor.execute("SELECT COUNT(*) FROM mistakes WHERE subject = '化学'")
            chemistry_count = cursor.fetchone()[0]
            print(f"🧪 化学科目错题: {chemistry_count} 条")
            
        return count
    except sqlite3.Error as e:
        print(f"❌ 查询错题数量失败: {e}")
        return 0

def main():
    print("🚀 开始数据库修复...")
    
    if not check_db_exists():
        return
    
    try:
        conn = sqlite3.connect(DB_PATH)
        cursor = conn.cursor()
        
        # 检查当前状态
        version = check_schema_version(cursor)
        count = check_mistakes_count(cursor)
        has_summary, has_analysis, columns = check_table_schema(cursor)
        
        # 如果字段缺失，则添加
        if not has_summary or not has_analysis:
            add_missing_columns(cursor)
            
            # 更新版本
            if version < 7:
                update_schema_version(cursor)
            
            # 提交更改
            conn.commit()
            print("\n✅ 数据库修复完成！")
            
            # 重新检查
            print("\n🔍 验证修复结果...")
            check_table_schema(cursor)
            
        else:
            print("\n✅ 数据库结构正常，无需修复")
        
        conn.close()
        
    except sqlite3.Error as e:
        print(f"❌ 数据库操作失败: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()