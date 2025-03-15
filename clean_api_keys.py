#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
清理数据库和配置文件中的API密钥
"""

import sqlite3
import json
import os
import sys
import configparser
import shutil

# 主程序数据库路径
MAIN_DB_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), '主程序', 'math_errors.db')

# 配置文件路径
CONFIG_JSON_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), '主程序', 'config.json')
CONFIG_INI_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), '主程序', 'config.ini')

def clean_db_api_keys():
    """清理数据库中的API密钥"""
    print("=== 清理数据库API密钥 ===")
    
    # 检查数据库文件
    if not os.path.exists(MAIN_DB_PATH):
        print(f"错误: 数据库文件不存在: {MAIN_DB_PATH}")
        return False
    
    # 连接数据库
    conn = sqlite3.connect(MAIN_DB_PATH)
    conn.row_factory = sqlite3.Row
    cursor = conn.cursor()
    
    # 检查表是否存在
    cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='api_config'")
    if not cursor.fetchone():
        print("警告: 数据库中不存在api_config表")
        conn.close()
        return False
    
    # 查询DeepSeek相关配置
    cursor.execute("SELECT key, value FROM api_config WHERE key='deepseek'")
    main_config = cursor.fetchone()
    
    if main_config:
        try:
            config_value = json.loads(main_config['value'])
            if 'api_key' in config_value:
                config_value['api_key'] = "YOUR_DEEPSEEK_API_KEY"
                # 更新配置
                cursor.execute("UPDATE api_config SET value=? WHERE key='deepseek'", 
                            (json.dumps(config_value),))
                print("✓ 已清理主DeepSeek配置中的API密钥")
        except json.JSONDecodeError:
            print(f"错误: 无法解析JSON配置: {main_config['value']}")
    
    # 查询替代API配置
    cursor.execute("SELECT key, value FROM api_config WHERE key='api_alternatives.deepseek'")
    alt_config = cursor.fetchone()
    
    if alt_config:
        try:
            alt_apis = json.loads(alt_config['value'])
            if isinstance(alt_apis, dict) and alt_apis:
                updated = False
                for api_id, api in alt_apis.items():
                    if 'api_key' in api:
                        api['api_key'] = "YOUR_API_KEY_HERE"
                        updated = True
                
                if updated:
                    cursor.execute("UPDATE api_config SET value=? WHERE key='api_alternatives.deepseek'", 
                                (json.dumps(alt_apis),))
                    print("✓ 已清理替代API配置中的API密钥")
        except json.JSONDecodeError:
            print(f"错误: 无法解析JSON配置: {alt_config['value']}")
    
    # 清理其他可能包含API密钥的表
    tables_to_check = [
        "openai_config", 
        "vision_config", 
        "text_config",
        "gemini_config",
        "multimodal_config"
    ]
    
    for table in tables_to_check:
        cursor.execute(f"SELECT name FROM sqlite_master WHERE type='table' AND name='{table}'")
        if cursor.fetchone():
            print(f"检查表 {table}...")
            try:
                cursor.execute(f"SELECT * FROM {table} LIMIT 1")
                columns = [column[0] for column in cursor.description]
                
                # 检查是否有api_key列
                key_columns = [col for col in columns if 'api_key' in col.lower() or 'apikey' in col.lower() or 'secret' in col.lower()]
                
                if key_columns:
                    for key_col in key_columns:
                        cursor.execute(f"UPDATE {table} SET {key_col}=?", ("YOUR_API_KEY_HERE",))
                        print(f"✓ 已清理表 {table} 的 {key_col} 列")
            except sqlite3.Error as e:
                print(f"警告: 处理表 {table} 时出错: {e}")
    
    # 提交更改
    conn.commit()
    conn.close()
    print("数据库API密钥清理完成")
    return True

def clean_config_json():
    """清理config.json中的API密钥"""
    print("\n=== 清理config.json中的API密钥 ===")
    
    if not os.path.exists(CONFIG_JSON_PATH):
        print(f"错误: 配置文件不存在: {CONFIG_JSON_PATH}")
        return False
    
    # 创建备份
    backup_path = f"{CONFIG_JSON_PATH}.bak"
    shutil.copy2(CONFIG_JSON_PATH, backup_path)
    print(f"已创建配置文件备份: {backup_path}")
    
    try:
        with open(CONFIG_JSON_PATH, 'r', encoding='utf-8') as f:
            config = json.load(f)
        
        # 清理各个部分的API密钥
        # 处理vision_api
        if 'vision_api' in config and 'api_key' in config['vision_api']:
            config['vision_api']['api_key'] = "YOUR_VISION_API_KEY"
        
        # 处理gemini_api
        if 'gemini_api' in config and 'api_key' in config['gemini_api']:
            config['gemini_api']['api_key'] = "YOUR_GEMINI_API_KEY"
        
        # 处理text_api
        if 'text_api' in config and 'api_key' in config['text_api']:
            config['text_api']['api_key'] = "YOUR_TEXT_API_KEY"
        
        # 处理aliyun_ocr
        if 'aliyun_ocr' in config:
            if 'access_key_id' in config['aliyun_ocr']:
                config['aliyun_ocr']['access_key_id'] = "YOUR_ALIYUN_ACCESS_KEY_ID"
            if 'access_key_secret' in config['aliyun_ocr']:
                config['aliyun_ocr']['access_key_secret'] = "YOUR_ALIYUN_ACCESS_KEY_SECRET"
        
        # 处理multimodal_models
        if 'multimodal_models' in config:
            for model_name, model_config in config['multimodal_models'].items():
                if 'api_key' in model_config:
                    config['multimodal_models'][model_name]['api_key'] = f"YOUR_{model_name.upper()}_API_KEY"
        
        # 处理api_alternatives
        if 'api_alternatives' in config and 'deepseek' in config['api_alternatives']:
            for api_id, api_config in config['api_alternatives']['deepseek'].items():
                if 'api_key' in api_config:
                    config['api_alternatives']['deepseek'][api_id]['api_key'] = "YOUR_DEEPSEEK_ALTERNATIVE_API_KEY"
        
        # 处理openai
        if 'openai' in config and 'api_key' in config['openai']:
            config['openai']['api_key'] = "YOUR_OPENAI_API_KEY"
        
        # 处理deepseek
        if 'deepseek' in config and 'api_key' in config['deepseek']:
            config['deepseek']['api_key'] = "YOUR_DEEPSEEK_API_KEY"
        
        # 保存更改
        with open(CONFIG_JSON_PATH, 'w', encoding='utf-8') as f:
            json.dump(config, f, ensure_ascii=False, indent=4)
        
        print("✓ 已清理config.json中的所有API密钥")
        return True
    
    except Exception as e:
        print(f"错误: 处理config.json时出错: {e}")
        # 如果出错，恢复备份
        if os.path.exists(backup_path):
            shutil.copy2(backup_path, CONFIG_JSON_PATH)
            print(f"已从备份恢复config.json")
        return False

def clean_config_ini():
    """清理config.ini中的API密钥"""
    print("\n=== 清理config.ini中的API密钥 ===")
    
    if not os.path.exists(CONFIG_INI_PATH):
        print(f"错误: 配置文件不存在: {CONFIG_INI_PATH}")
        return False
    
    # 创建备份
    backup_path = f"{CONFIG_INI_PATH}.bak"
    shutil.copy2(CONFIG_INI_PATH, backup_path)
    print(f"已创建配置文件备份: {backup_path}")
    
    try:
        config = configparser.ConfigParser()
        config.read(CONFIG_INI_PATH, encoding='utf-8')
        
        # 清理[API]部分
        if 'API' in config:
            if 'openai_api_key' in config['API']:
                config['API']['openai_api_key'] = "YOUR_OPENAI_API_KEY"
            if 'deepseek_api_key' in config['API']:
                config['API']['deepseek_api_key'] = "YOUR_DEEPSEEK_API_KEY"
        
        # 清理[VISION_API]部分
        if 'VISION_API' in config and 'api_key' in config['VISION_API']:
            config['VISION_API']['api_key'] = "YOUR_VISION_API_KEY"
        
        # 清理[TEXT_API]部分
        if 'TEXT_API' in config and 'api_key' in config['TEXT_API']:
            config['TEXT_API']['api_key'] = "YOUR_TEXT_API_KEY"
        
        # 保存更改
        with open(CONFIG_INI_PATH, 'w', encoding='utf-8') as f:
            config.write(f)
        
        print("✓ 已清理config.ini中的所有API密钥")
        return True
    
    except Exception as e:
        print(f"错误: 处理config.ini时出错: {e}")
        # 如果出错，恢复备份
        if os.path.exists(backup_path):
            shutil.copy2(backup_path, CONFIG_INI_PATH)
            print(f"已从备份恢复config.ini")
        return False

def create_gitignore_entry():
    """创建或更新.gitignore文件，确保敏感文件不会被提交"""
    print("\n=== 更新.gitignore ===")
    
    gitignore_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), '.gitignore')
    
    # 要添加到.gitignore的条目
    entries_to_add = [
        "# API密钥和配置文件",
        "*.env",
        "主程序/config.ini",
        "主程序/config.json",
        "主程序/config.ini.bak",
        "主程序/config.json.bak",
        "RAG2025/.env",
        "",
        "# 数据库文件",
        "主程序/*.db",
        "RAG2025/*.db",
        "",
        "# 备份文件",
        "*.bak",
        ""
    ]
    
    existing_entries = []
    if os.path.exists(gitignore_path):
        with open(gitignore_path, 'r', encoding='utf-8') as f:
            existing_entries = f.read().splitlines()
    
    # 合并现有条目和新条目，避免重复
    new_entries = []
    for entry in entries_to_add:
        if entry not in existing_entries:
            new_entries.append(entry)
    
    if new_entries:
        with open(gitignore_path, 'a', encoding='utf-8') as f:
            f.write('\n' + '\n'.join(new_entries))
        print(f"✓ 已添加 {len(new_entries)} 个条目到.gitignore")
    else:
        print("✓ .gitignore已包含所有必要条目")

def create_example_configs():
    """创建配置文件的示例版本，用于示例和文档目的"""
    print("\n=== 创建示例配置文件 ===")
    
    # 创建config.json.example
    if os.path.exists(CONFIG_JSON_PATH):
        example_path = f"{CONFIG_JSON_PATH}.example"
        # 如果已存在示例文件，先不覆盖
        if not os.path.exists(example_path):
            try:
                with open(CONFIG_JSON_PATH, 'r', encoding='utf-8') as f:
                    config = json.load(f)
                
                # 创建示例配置
                with open(example_path, 'w', encoding='utf-8') as f:
                    json.dump(config, f, ensure_ascii=False, indent=4)
                print(f"✓ 已创建示例配置文件: {example_path}")
            except Exception as e:
                print(f"警告: 创建示例配置文件时出错: {e}")
    
    # 创建config.ini.example
    if os.path.exists(CONFIG_INI_PATH):
        example_path = f"{CONFIG_INI_PATH}.example"
        # 如果已存在示例文件，先不覆盖
        if not os.path.exists(example_path):
            try:
                shutil.copy2(CONFIG_INI_PATH, example_path)
                print(f"✓ 已创建示例配置文件: {example_path}")
            except Exception as e:
                print(f"警告: 创建示例配置文件时出错: {e}")

if __name__ == "__main__":
    print("开始清理API密钥...")
    
    # 清理数据库中的API密钥
    db_result = clean_db_api_keys()
    
    # 清理配置文件中的API密钥
    json_result = clean_config_json()
    ini_result = clean_config_ini()
    
    # 创建/更新.gitignore
    create_gitignore_entry()
    
    # 创建示例配置文件
    create_example_configs()
    
    print("\n=== 清理完成 ===")
    if db_result and json_result and ini_result:
        print("所有API密钥已成功清理。")
        print("注意：已创建原配置文件的备份：config.json.bak 和 config.ini.bak")
        print("上传到GitHub前，请确认所有敏感信息已被清理。")
    else:
        print("警告：部分清理操作可能未完成，请检查上述日志。")
