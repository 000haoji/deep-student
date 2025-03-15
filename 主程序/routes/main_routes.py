import sys
import os
from flask import Blueprint, render_template

# 添加当前目录到Python路径
current_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if current_dir not in sys.path:
    sys.path.append(current_dir)

# 移除LangChain RAG Blueprint导入和相关代码
# try:
#     from langchain_rag import langchain_rag_bp
# except ImportError as e:
#     print(f"警告: 无法导入LangChain RAG Blueprint，原因: {str(e)}，某些功能可能不可用")
#     langchain_rag_bp = None

# 移除deepseek_stream导入部分
# try:
#     from deepseek_stream import deepseek_stream_bp
# except ImportError:
#     print("警告: 无法导入DeepSeek Stream Blueprint，某些功能可能不可用")
#     deepseek_stream_bp = None

# 创建主路由蓝图
main_bp = Blueprint('main', __name__)

# 注释掉langchain_rag路由
# @main_bp.route('/langchain_rag')
# def langchain_rag_page():
#     """RAG增强流式请求页面"""
#     return render_template('langchain_rag_stream.html')

def register_blueprints(app):
    """注册所有蓝图"""
    # 注册主路由蓝图
    app.register_blueprint(main_bp)
    
    # 注册LangChain RAG蓝图（如果可用）
    # if langchain_rag_bp:
    #     app.register_blueprint(langchain_rag_bp)
    #     print("已注册LangChain RAG Blueprint")
    
    # 注册DeepSeek Stream蓝图（如果可用）
    # if deepseek_stream_bp:
    #     app.register_blueprint(deepseek_stream_bp)
    #     print("已注册DeepSeek Stream Blueprint") 