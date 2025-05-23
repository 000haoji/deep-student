"""
错题管理系统API演示客户端
展示如何使用API进行错题管理
"""
import requests
import json
from datetime import datetime

BASE_URL = "http://localhost:8001"


def print_section(title):
    """打印分隔线"""
    print(f"\n{'='*50}")
    print(f" {title}")
    print('='*50)


def demo_create_problems():
    """演示创建错题"""
    print_section("1. 创建错题示例")
    
    # 数学错题
    math_problem = {
        "title": "高等数学 - 极限计算",
        "content": "求极限 lim(x→0) (sin x - x) / x³",
        "subject": "math",
        "category": "极限",
        "user_answer": "-1/3",
        "notes": "忘记了泰勒展开的高阶项"
    }
    
    response = requests.post(f"{BASE_URL}/problems", json=math_problem)
    if response.status_code == 200:
        result = response.json()
        print(f"✓ 创建数学错题成功！")
        print(f"  ID: {result['id']}")
        print(f"  标题: {result['title']}")
        print(f"  掌握度: {result['mastery_level']}")
        math_id = result['id']
    else:
        print(f"✗ 创建失败: {response.text}")
        return
    
    # 英语错题
    english_problem = {
        "title": "英语语法 - 时态",
        "content": "I ___ (go) to school yesterday.",
        "subject": "english",
        "category": "语法-时态",
        "user_answer": "go",
        "notes": "过去时态使用错误"
    }
    
    response = requests.post(f"{BASE_URL}/problems", json=english_problem)
    if response.status_code == 200:
        result = response.json()
        print(f"\n✓ 创建英语错题成功！")
        print(f"  ID: {result['id']}")
        print(f"  标题: {result['title']}")
        english_id = result['id']
    
    # 政治错题
    politics_problem = {
        "title": "马克思主义基本原理",
        "content": "生产力的构成要素包括哪些？",
        "subject": "politics",
        "category": "基本原理",
        "user_answer": "劳动工具",
        "notes": "答案不完整，遗漏了劳动者和劳动对象"
    }
    
    response = requests.post(f"{BASE_URL}/problems", json=politics_problem)
    if response.status_code == 200:
        result = response.json()
        print(f"\n✓ 创建政治错题成功！")
        print(f"  ID: {result['id']}")
        politics_id = result['id']
    
    return [math_id, english_id, politics_id]


def demo_list_problems():
    """演示获取错题列表"""
    print_section("2. 获取错题列表")
    
    # 获取所有错题
    response = requests.get(f"{BASE_URL}/problems")
    if response.status_code == 200:
        problems = response.json()
        print(f"\n所有错题（共{len(problems)}个）：")
        for p in problems:
            print(f"  - [{p['id']}] {p['title']} ({p['subject']}) - 掌握度: {p['mastery_level']}")
    
    # 按学科筛选
    response = requests.get(f"{BASE_URL}/problems?subject=math")
    if response.status_code == 200:
        math_problems = response.json()
        print(f"\n数学错题（共{len(math_problems)}个）：")
        for p in math_problems:
            print(f"  - {p['title']}")


def demo_get_problem(problem_id):
    """演示获取单个错题详情"""
    print_section("3. 获取错题详情")
    
    response = requests.get(f"{BASE_URL}/problems/{problem_id}")
    if response.status_code == 200:
        problem = response.json()
        print(f"\n错题详情:")
        print(f"  标题: {problem['title']}")
        print(f"  内容: {problem['content']}")
        print(f"  学科: {problem['subject']}")
        print(f"  分类: {problem['category']}")
        print(f"  用户答案: {problem['user_answer']}")
        print(f"  备注: {problem['notes']}")
        print(f"  创建时间: {problem['created_at']}")
        print(f"  掌握度: {problem['mastery_level']}")


def demo_create_analysis(problem_ids):
    """演示创建分析"""
    print_section("4. 创建批量分析")
    
    analysis_data = {
        "problem_ids": problem_ids,
        "title": "综合错题分析 - " + datetime.now().strftime("%Y-%m-%d")
    }
    
    response = requests.post(f"{BASE_URL}/analyses", json=analysis_data)
    if response.status_code == 200:
        analysis = response.json()
        print(f"\n✓ 分析创建成功！")
        print(f"  分析ID: {analysis['id']}")
        print(f"  标题: {analysis['title']}")
        print(f"  题目数量: {analysis['problem_count']}")
        print(f"  涉及学科: {', '.join(analysis['subjects'])}")
        print(f"  分析摘要: {analysis['summary']}")
        print(f"\n  改进建议:")
        for i, suggestion in enumerate(analysis['suggestions'], 1):
            print(f"    {i}. {suggestion}")
        return analysis['id']


def demo_list_analyses():
    """演示获取分析列表"""
    print_section("5. 获取分析列表")
    
    response = requests.get(f"{BASE_URL}/analyses")
    if response.status_code == 200:
        analyses = response.json()
        print(f"\n所有分析（共{len(analyses)}个）：")
        for a in analyses:
            print(f"  - [{a['id']}] {a['title']}")
            print(f"    题目数: {a['problem_count']}, 学科: {', '.join(a['subjects'])}")
            print(f"    创建时间: {a['created_at']}")


def main():
    """主函数"""
    print("="*50)
    print(" 错题管理系统API演示")
    print("="*50)
    print(f"\n连接到: {BASE_URL}")
    
    # 检查服务是否运行
    try:
        response = requests.get(f"{BASE_URL}/health")
        if response.status_code == 200:
            print("✓ 服务运行正常")
        else:
            print("✗ 服务异常")
            return
    except requests.exceptions.ConnectionError:
        print("✗ 无法连接到服务，请确保API已启动")
        print("  运行: python test_simple_api.py")
        return
    
    # 执行演示
    problem_ids = demo_create_problems()
    demo_list_problems()
    
    if problem_ids:
        demo_get_problem(problem_ids[0])
        demo_create_analysis(problem_ids)
        demo_list_analyses()
    
    print("\n" + "="*50)
    print(" 演示完成！")
    print("="*50)
    print("\n提示：")
    print("- 访问 http://localhost:8001/docs 查看完整API文档")
    print("- 这是简化版本，完整版本包含更多功能")


if __name__ == "__main__":
    main() 