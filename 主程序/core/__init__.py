"""
核心模块包初始化
"""
# 导入核心模块
# 避免显式导入可能导致循环引用的模块，而是让调用者在需要时导入
from . import database
# 下面两行注释掉，避免循环导入
# from . import config_ini_manager
# from . import config_manager
