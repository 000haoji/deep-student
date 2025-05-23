#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
测试脚本：模拟请求 /api/problem/update-analysis API
"""

import requests
import json
import sys
import uuid

# 配置参数
BASE_URL = "http://localhost:5001"  # 主程序运行在5001端口
API_ENDPOINT = "/api/problem/update-analysis"
SUBJECT = "math"  # 默认学科为数学

def test_update_analysis(problem_id=None):
    """
    测试更新分析结果API
    
    Args:
        problem_id: 错题ID，如果不提供则使用测试ID
    """
    # 如果未提供problem_id，使用测试ID
    if not problem_id:
        # 尝试生成一个UUID风格的ID
        problem_id = "23319f40-3a2d-492f-9e02-4259d3e98a24"#f"test-{str(uuid.uuid4())[:8]}"
    
    # 构建请求URL
    url = f"{BASE_URL}{API_ENDPOINT}?subject={SUBJECT}"
    
    # 构建测试数据
    payload = {
        "problem_id": problem_id,
        "analysis_result": {
            "题目类型": "高等数学",
            "具体分支": "微积分",
            "错误类型": "概念理解错误",
            "题目原文": "设f(x)是[0,+∞)内的正值连续函数，且∫f(t)dt=ln(1/2)和g(x)<0,g(x)=∫f(t)dt,则(A)-2.1 (B)-2.3 (C)2,-1 (D)2,-3",
            "错误分析": "这是一个测试错误分析内容，通过API请求模拟保存。",
            "正确解法": "这是一个测试的正确解法内容。",
            "难度评估": 3,
            "知识点标签": ["微积分", "定积分", "函数性质"]
        }
    }
    
    # 设置请求头
    headers = {
        'Content-Type': 'application/json'
    }
    
    print(f"正在发送POST请求到 {url}")
    print(f"请求数据: {json.dumps(payload, ensure_ascii=False, indent=2)}")
    
    try:
        # 发送POST请求
        response = requests.post(url, headers=headers, json=payload)
        
        # 检查响应状态码
        print(f"响应状态码: {response.status_code}")
        
        # 解析响应JSON
        try:
            response_json = response.json()
            print(f"响应数据: {json.dumps(response_json, ensure_ascii=False, indent=2)}")
            
            # 检查响应是否成功
            if response_json.get('success', False):
                print("✅ API请求成功！分析结果已更新。")
                return True
            else:
                print(f"❌ API请求失败！错误信息: {response_json.get('error', '未知错误')}")
                return False
                
        except json.JSONDecodeError:
            print(f"❌ 无法解析响应JSON: {response.text}")
            return False
            
    except requests.RequestException as e:
        print(f"❌ 请求异常: {str(e)}")
        return False

if __name__ == "__main__":
    # 检查是否从命令行传入了problem_id
    problem_id = None
    if len(sys.argv) > 1:
        problem_id = sys.argv[1]
        
    test_update_analysis(problem_id)
