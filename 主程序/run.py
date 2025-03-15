"""
应用程序启动脚本，设置适当的环境配置
"""
import os
import sys
import json
import logging
import locale
import traceback
from config import API_CONFIG
from app import app

# 强制使用UTF-8编码模式，避免Windows GBK编码问题
if sys.platform.startswith('win'):
    # 设置控制台编码为UTF-8
    os.system('chcp 65001')
    # 修复Python io编码
    sys.stdout.reconfigure(encoding='utf-8')
    sys.stderr.reconfigure(encoding='utf-8')

# 配置日志
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        # 确保文件日志使用UTF-8编码
        logging.FileHandler("app.log", encoding='utf-8'),
        logging.StreamHandler()
    ]
)

logger = logging.getLogger()
logger.info(f"当前系统默认编码: {locale.getpreferredencoding()}")
logger.info(f"Python默认编码: {sys.getdefaultencoding()}")
logger.info(f"文件系统编码: {sys.getfilesystemencoding()}")
logger.info(f"标准输出编码: {sys.stdout.encoding}")

if __name__ == '__main__':
    logger.info("启动Flask应用程序")
    try:
        # 在所有系统上设置app.config，不仅限于Windows
        app.config['JSON_AS_ASCII'] = False  # 允许JSON响应包含非ASCII字符
        
        # 添加额外的配置以确保正确处理中文
        app.config['SQLALCHEMY_ENGINE_OPTIONS'] = {
            'connect_args': {'charset': 'utf8mb4'}
        }
        
        # 设置SQLite连接配置 - 不使用自动类型检测，避免日期时间解析错误
        import sqlite3
        original_connect = sqlite3.connect
        sqlite3.connect = lambda *args, **kwargs: original_connect(*args, **kwargs)
        
        # 启动Flask应用
        app.run(debug=True, port=5001)
    except Exception as e:
        logger.error(f"启动应用程序时出错: {str(e)}")
        logger.error(traceback.format_exc())
