#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
配置模块 - 处理环境变量和命令行参数
"""

import os
import time
import argparse
import logging
from dotenv import load_dotenv

# 记录应用启动时间
startup_time = time.time()

# 加载环境变量
load_dotenv()

# 处理命令行参数
parser = argparse.ArgumentParser(description='DeepSeek知识库查询系统')
parser.add_argument('--use-go-proxy', action='store_true', help='启用Go代理')
parser.add_argument('--go-proxy-url', type=str, help='Go代理URL，默认为http://localhost:8000/proxy/stream')
parser.add_argument('--fast', action='store_true', help='启用快速启动模式，跳过嵌入向量预加载')
parser.add_argument('--debug', action='store_true', help='启用调试模式')
parser.add_argument('--host', type=str, default='0.0.0.0', help='主机地址')
parser.add_argument('--port', type=int, default=5000, help='端口号')
args = parser.parse_args()

# 初始化API密钥和URL
EMBEDDER_API_KEY = os.environ.get("EMBEDDER_API_KEY", "")
EMBEDDER_API_URL = os.environ.get("EMBEDDER_API_URL", "https://api.siliconflow.cn/v1/embeddings")
EMBEDDER_MODEL = os.environ.get("EMBEDDER_MODEL", "Pro/BAAI/bge-m3")
LLM_API_KEY = os.environ.get("LLM_API_KEY", "")
LLM_API_URL = os.environ.get("LLM_API_URL", "https://api.siliconflow.cn/v1/chat/completions")
LLM_MODEL = os.environ.get("LLM_MODEL", "Pro/deepseek-ai/DeepSeek-V3")

# Go代理配置 - 命令行参数优先于环境变量
USE_GO_PROXY = args.use_go_proxy or os.environ.get("USE_GO_PROXY", "false").lower() == "true"
ENABLE_GO_PROXY = USE_GO_PROXY  # 添加ENABLE_GO_PROXY作为USE_GO_PROXY的别名
GO_PROXY_URL = args.go_proxy_url or os.environ.get("GO_PROXY_URL", "http://localhost:8000/proxy/stream")

# 快速启动模式配置
FAST_MODE = args.fast
DEBUG_MODE = args.debug or False  # 修改为默认关闭调试模式
HOST = args.host
PORT = args.port

# 配置日志
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# 持久化存储路径设置
DATA_DIR = os.environ.get("DATA_DIR", "data")
VECTORS_PATH = os.path.join(DATA_DIR, "vectors.sqlite")
DOCUMENTS_PATH = os.path.join(DATA_DIR, "documents.sqlite")
METADATA_PATH = os.path.join(DATA_DIR, "metadata.json")

# 确保数据目录存在
os.makedirs(DATA_DIR, exist_ok=True)

# 备份相关配置
BACKUP_DIR = os.path.join(DATA_DIR, "backups")
os.makedirs(BACKUP_DIR, exist_ok=True)
AUTO_BACKUP_INTERVAL = int(os.environ.get("AUTO_BACKUP_INTERVAL", 86400))  # 默认24小时 