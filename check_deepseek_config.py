#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
检查并修复DeepSeek API配置
"""

import sqlite3
import json
import os
import sys

# 配置数据库路径
DATABASE_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), '主程序', 'math_errors.db')

def check_deepseek_config():
    """检查DeepSeek API配置"""
    print("=== DeepSeek API配置检查工具 ===\n")
    
    # 检查数据库文件
    if not os.path.exists(DATABASE_PATH):
        print(f"错误: 数据库文件不存在: {DATABASE_PATH}")
        return False
    
    # 连接数据库
    conn = sqlite3.connect(DATABASE_PATH)
    conn.row_factory = sqlite3.Row
    cursor = conn.cursor()
    
    # 检查表是否存在
    cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='api_config'")
    if not cursor.fetchone():
        print("错误: 数据库中不存在api_config表")
        conn.close()
        return False
    
    # 查询DeepSeek相关配置
    print("当前DeepSeek API配置:\n")
    
    # 查询主配置
    cursor.execute("SELECT key, value FROM api_config WHERE key='deepseek'")
    main_config = cursor.fetchone()
    
    if main_config:
        try:
            config_value = json.loads(main_config['value'])
            print(f"1. 主配置 (deepseek):")
            print(f"   - API URL: {config_value.get('api_url', '未设置')}")
            print(f"   - API Key: {mask_api_key(config_value.get('api_key', '未设置'))}")
            print(f"   - 模型: {config_value.get('model', '未设置')}")
            
            # 检查API密钥是否为空
            if not config_value.get('api_key'):
                print("\n⚠️ 警告: DeepSeek API密钥未设置!")
        except json.JSONDecodeError:
            print(f"错误: 无法解析主配置 JSON: {main_config['value']}")
    else:
        print("1. 主配置 (deepseek): 未找到")
    
    # 查询替代API配置
    cursor.execute("SELECT key, value FROM api_config WHERE key='api_alternatives.deepseek'")
    alt_config = cursor.fetchone()
    
    if alt_config:
        try:
            alt_apis = json.loads(alt_config['value'])
            if isinstance(alt_apis, dict) and alt_apis:
                print(f"\n2. 替代API配置 ({len(alt_apis)} 个):")
                for api_id, api in alt_apis.items():
                    print(f"   - ID: {api_id}")
                    print(f"     API Key: {mask_api_key(api.get('api_key', '未设置'))}")
                    print(f"     优先级: {api.get('priority', '未设置')}")
            else:
                print("\n2. 替代API配置: 空")
        except json.JSONDecodeError:
            print(f"错误: 无法解析替代API配置 JSON: {alt_config['value']}")
    else:
        print("\n2. 替代API配置: 未找到")
    
    conn.close()
    return True

def fix_deepseek_config():
    """修复DeepSeek API配置"""
    print("\n=== 更新DeepSeek API配置 ===\n")
    
    # 询问用户是否要更新配置
    choice = input("是否更新DeepSeek API配置? (y/n): ").strip().lower()
    if choice != 'y':
        print("操作已取消")
        return
    
    # 获取新的API密钥
    new_api_key = input("\n请输入DeepSeek API密钥: ").strip()
    if not new_api_key:
        print("错误: API密钥不能为空")
        return
    
    # 连接数据库
    conn = sqlite3.connect(DATABASE_PATH)
    cursor = conn.cursor()
    
    # 检查主配置是否存在
    cursor.execute("SELECT value FROM api_config WHERE key='deepseek'")
    main_row = cursor.fetchone()
    
    if main_row:
        try:
            # 更新现有配置
            current_config = json.loads(main_row[0])
            current_config['api_key'] = new_api_key
            # 确保api_url设置正确
            if not current_config.get('api_url'):
                current_config['api_url'] = 'https://api.deepseek.com/v1'
            
            # 保存更新后的配置
            cursor.execute("UPDATE api_config SET value=? WHERE key='deepseek'", 
                           (json.dumps(current_config),))
            conn.commit()
            print("\n✅ 已成功更新主DeepSeek配置")
        except json.JSONDecodeError:
            # 创建新配置
            new_config = {
                'api_key': new_api_key,
                'api_url': 'https://api.deepseek.com/v1',
                'model': 'deepseek-chat'
            }
            cursor.execute("UPDATE api_config SET value=? WHERE key='deepseek'", 
                           (json.dumps(new_config),))
            conn.commit()
            print("\n✅ 已成功创建新的DeepSeek配置")
    else:
        # 创建新配置
        new_config = {
            'api_key': new_api_key,
            'api_url': 'https://api.deepseek.com/v1',
            'model': 'deepseek-chat'
        }
        cursor.execute("INSERT INTO api_config (key, value) VALUES (?, ?)", 
                       ('deepseek', json.dumps(new_config)))
        conn.commit()
        print("\n✅ 已成功创建新的DeepSeek配置")
    
    conn.close()
    print("\n配置已更新，请重启应用以应用新配置")

def mask_api_key(api_key):
    """遮蔽API密钥，只显示前4位和后4位"""
    if not api_key or len(api_key) < 8:
        return api_key
    
    masked_length = len(api_key) - 8
    return api_key[:4] + '*' * masked_length + api_key[-4:]

if __name__ == "__main__":
    if check_deepseek_config():
        fix_deepseek_config()
