"""
应用程序入口点，用于启动Flask服务器
"""
import os
import logging
import traceback
import json
from flask import Flask, render_template, send_from_directory, request, redirect
from api import problem_api, review_api, tag_api, backup_api, settings_api
from core import database, config_ini_manager
import config  # Import the entire config module
# 导入FastGPT和CherryStudio集成
from fastgpt_integration import fastgpt_bp
from cherrystudio_integration import cherrystudio_bp
# 导入学科Prompt设置API
from api.prompt_settings_api import prompt_settings_api
# 导入流式分析API
from api.stream_analysis_api import stream_analysis_api
# 导入回顾分析API
from api.review_analysis_api import review_analysis_api
# 导入AI分析API
from api.ai_analysis_api import ai_analysis_api
# 导入Markdown测试API
from api.markdown_test_api import markdown_test_api

# 配置日志
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
    handlers=[
        logging.FileHandler("app.log"),
        logging.StreamHandler()
    ]
)

logger = logging.getLogger(__name__)

# 导入新的路由模块
try:
    from routes import register_blueprints
    from routes.settings_routes import settings_blueprint
    has_new_routes = True
except ImportError as e:
    logger.warning(f"无法导入某些路由模块，原因: {str(e)}，某些功能可能不可用")
    has_new_routes = False
    settings_blueprint = None

# 创建Flask应用
app = Flask(__name__, 
            template_folder=os.path.join(os.path.dirname(os.path.abspath(__file__)), 'templates'),
            static_folder=os.path.join(os.path.dirname(os.path.abspath(__file__)), 'static'),
            static_url_path='/static')  # 显式设置静态URL路径
app.config['SECRET_KEY'] = os.environ.get('SECRET_KEY', 'TEMP_SECRET_KEY')  # Use environment variable for secret key in production
app.config['UPLOAD_FOLDER'] = os.path.join(os.getcwd(), 'uploads')

# 确保文件上传目录存在
os.makedirs(app.config['UPLOAD_FOLDER'], exist_ok=True)

# 为uploads目录添加静态文件服务路由
@app.route('/uploads/<path:filename>')
def uploaded_file(filename):
    """提供上传文件的访问"""
    return send_from_directory(app.config['UPLOAD_FOLDER'], filename)

# 注册API蓝图
app.register_blueprint(problem_api.problem_api)
app.register_blueprint(review_api.review_api)
app.register_blueprint(tag_api.tag_api)
app.register_blueprint(backup_api.backup_api)
app.register_blueprint(settings_api.settings_api)
app.register_blueprint(prompt_settings_api)  # 注册学科Prompt设置API蓝图
app.register_blueprint(stream_analysis_api)  # 注册流式分析API蓝图
app.register_blueprint(review_analysis_api)  # 注册回顾分析API蓝图
app.register_blueprint(ai_analysis_api)  # 注册AI分析API蓝图
app.register_blueprint(markdown_test_api)  # 注册Markdown测试API蓝图

# 注册FastGPT和CherryStudio蓝图
app.register_blueprint(fastgpt_bp)
app.register_blueprint(cherrystudio_bp)

# 注册新的路由和模块（LangChain RAG等）
if has_new_routes:
    try:
        register_blueprints(app)
        logger.info("已成功注册新的路由模块")
    except Exception as e:
        logger.error(f"注册新路由模块时出错: {str(e)}")
        logger.error(traceback.format_exc())

# 注册设置蓝图
if settings_blueprint:
    app.register_blueprint(settings_blueprint)

# 添加全局上下文处理器，确保所有模板都能访问学科数据
@app.context_processor
def inject_subjects():
    return {
        'subjects': config.SUBJECTS, 
        'current_subject': request.args.get('subject', list(config.SUBJECTS.keys())[0] if config.SUBJECTS else 'math')
    }

# 首页路由
@app.route('/')
def index():
    # 获取学科参数，默认为math
    subject = request.args.get('subject', config.DEFAULT_SUBJECT)
    
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
    
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    # 提供所有支持的学科列表
    return render_template(
        'index.html', 
        active_nav='home', 
        subjects=config.SUBJECTS,
        current_subject=subject,
        subject_info=current_subject
    )

@app.route('/problems')
@app.route('/problems/<subject>')
def problems(subject=None):
    # 如果未指定学科，则使用默认学科
    if not subject:
        subject = config.DEFAULT_SUBJECT
        
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
        
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    return render_template(
        'problems.html', 
        active_nav='problems', 
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    )

@app.route('/problem/<problem_id>')
def problem_detail(problem_id):
    # 从数据库获取问题详情
    from core.database import get_error_problem_by_id
    problem = get_error_problem_by_id(problem_id)
    
    # 如果问题不存在，返回404
    if not problem:
        # 使用自定义的错误页面而不是404.html
        return render_template(
            '404.html', 
            message="未找到指定的错题", 
            active_nav='problems',
            current_subject=config.DEFAULT_SUBJECT,
            subject_info=config.SUBJECTS[config.DEFAULT_SUBJECT],
            subjects=config.SUBJECTS
        ), 404
    
    # 获取问题的学科
    subject = problem.get('subject', config.DEFAULT_SUBJECT)
    
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
    
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
        
    return render_template(
        'problem_detail.html', 
        problem=problem, 
        active_nav='problems',
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    )

@app.route('/reviews')
@app.route('/reviews/<subject>')
def reviews_list(subject=None):
    # 如果未指定学科，则使用默认学科
    if not subject:
        subject = config.DEFAULT_SUBJECT
        
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
        
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    return render_template(
        'reviews.html', 
        active_nav='reviews',
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    )

@app.route('/review/<review_id>')
def review_detail(review_id):
    # 获取学科参数，默认为math
    subject = request.args.get('subject', config.DEFAULT_SUBJECT)
    
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
        
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    return render_template(
        'review_detail.html', 
        active_nav='reviews',
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    )

@app.route('/tags')
def tags_management():
    return render_template('tags.html', active_nav='tags')

@app.route('/backup_management')
def backup_management():
    # 获取学科参数，默认为math
    subject = request.args.get('subject', config.DEFAULT_SUBJECT)
    
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
    
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    return render_template(
        'backup.html', 
        active_nav='backup',
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    )

@app.route('/backup')
def backup():
    # 获取学科参数，默认为math
    subject = request.args.get('subject', config.DEFAULT_SUBJECT)
    
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
    
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    return render_template(
        'backup.html', 
        active_nav='backup',
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    )

@app.route('/api-settings')
def api_settings():
    # 获取学科参数，默认为math
    subject = request.args.get('subject', config.DEFAULT_SUBJECT)
    
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
    
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    return render_template(
        'api_settings.html', 
        active_nav='api-settings',
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    )

# 复习计划路由
@app.route('/review')
def review():
    # 获取学科参数，默认为math
    subject = request.args.get('subject', config.DEFAULT_SUBJECT)
    
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
    
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    return render_template(
        'review.html', 
        active_nav='review',
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    )

# 回顾分析页面路由
@app.route('/review-analysis/<analysis_id>')
def review_analysis(analysis_id):
    """重定向旧版分析到新版页面"""
    # 重定向到新系统的分析页面
    return redirect(f'/reviews', code=301)

# API设置路由
@app.route('/api_config')
def api_config():
    return render_template(
        'api_config.html', 
        active_nav='api_config',
        current_subject=config.DEFAULT_SUBJECT,
        subject_info=config.SUBJECTS[config.DEFAULT_SUBJECT],
        subjects=config.SUBJECTS
    )

# 添加学科Prompt设置路由
@app.route('/prompt_settings')
def prompt_settings():
    """
    学科Prompt设置页面
    """
    try:
        # 默认选择第一个学科
        current_subject = list(config.SUBJECTS.keys())[0]
        
        # 准备模板数据
        template_data = {
            'active_nav': 'prompt_settings',
            'subjects': config.SUBJECTS,
            'current_subject': current_subject,
            'analysis_prompts': config.SUBJECT_ANALYSIS_PROMPTS,
        }
        
        return render_template('prompt_settings.html', **template_data)
    except Exception as e:
        logger.error(f"加载prompt设置页面出错: {str(e)}\n{traceback.format_exc()}")
        return render_template('error.html', error=str(e)), 500

# 移除硅基流动流式请求页面路由
# @app.route('/deepseek-stream')
# def deepseek_stream():
#     return render_template('deepseek_stream.html', active_nav='deepseek_stream')

# 添加静态文件支持
@app.route('/uploads/<filename>')
def serve_upload(filename):
    return send_from_directory(config.UPLOAD_FOLDER, filename)  # Access UPLOAD_FOLDER using config.UPLOAD_FOLDER

@app.route('/static/<path:filename>')
def serve_static(filename):
    return send_from_directory('static', filename)

# 添加一个简单的测试路由
@app.route('/test')
def test_route():
    return """
    <html>
        <head>
            <title>测试页面</title>
        </head>
        <body>
            <h1>测试成功</h1>
            <p>如果您能看到此消息，说明Flask服务器正在运行。</p>
            <ul>
                <li><a href="/">首页</a></li>
                <li><a href="/problems">错题库</a></li>
                <li><a href="/reviews">回顾记录</a></li>
            </ul>
        </body>
    </html>
    """

@app.route('/problem/add')
def add_problem():
    """添加错题页面"""
    # 获取学科参数
    subject = request.args.get('subject', config.DEFAULT_SUBJECT)
    
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
    
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    return render_template(
        'edit_problem.html',  # 复用编辑页面
        problem_id='new',     # 使用'new'表示新建错题
        active_nav='problems',
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    )

@app.route('/problem/edit/<problem_id>')
def edit_problem(problem_id):
    """编辑错题页面"""
    # 获取学科参数
    subject = request.args.get('subject', config.DEFAULT_SUBJECT)
    
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
    
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    return render_template(
        'edit_problem.html', 
        problem_id=problem_id, 
        active_nav='problems',
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    )

# 添加全局错误处理
@app.errorhandler(404)
def page_not_found(e):
    """处理404错误"""
    # 获取学科参数，如果没有则使用默认学科
    subject = request.args.get('subject', config.DEFAULT_SUBJECT)
    
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
    
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    return render_template(
        '404.html', 
        message="页面未找到", 
        active_nav='problems',
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    ), 404

@app.errorhandler(500)
def internal_server_error(e):
    """处理500错误"""
    # 获取学科参数，如果没有则使用默认学科
    subject = request.args.get('subject', config.DEFAULT_SUBJECT)
    
    # 验证学科是否存在于配置中
    if subject not in config.SUBJECTS:
        subject = config.DEFAULT_SUBJECT
    
    # 从配置获取当前学科信息
    current_subject = config.SUBJECTS[subject]
    
    return render_template(
        '404.html',  # 复用404模板
        message="服务器内部错误，请联系管理员", 
        active_nav='problems',
        current_subject=subject,
        subject_info=current_subject,
        subjects=config.SUBJECTS
    ), 500

# 初始化数据库
with app.app_context():
    # 移除旧系统review_analysis表
    try:
        conn = database.get_db()
        cursor = conn.cursor()
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='review_analysis'")
        if cursor.fetchone():
            cursor.execute("DROP TABLE review_analysis")
            conn.commit()
            logger.info("已成功移除旧系统review_analysis表")
    except Exception as e:
        logger.error(f"移除旧系统review_analysis表时出错: {str(e)}")
        logger.error(traceback.format_exc())
        
    database.init_db()
    
    # 加载保存的学科提示词配置
    try:
        prompts_file = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'data', 'subject_prompts.json')
        if os.path.exists(prompts_file):
            with open(prompts_file, 'r', encoding='utf-8') as f:
                saved_prompts = json.loads(f.read())
                logger.info(f"从文件加载学科提示词配置: {prompts_file}")
                logger.info(f"加载的学科: {list(saved_prompts.keys())}")
                # 更新配置中的提示词
                for subject, prompts in saved_prompts.items():
                    if subject in config.SUBJECT_ANALYSIS_PROMPTS:
                        # 更新现有学科的提示词
                        config.SUBJECT_ANALYSIS_PROMPTS[subject].update(prompts)
                        logger.info(f"更新学科 {subject} 的提示词配置")
                    else:
                        # 添加新学科的提示词
                        config.SUBJECT_ANALYSIS_PROMPTS[subject] = prompts
                        logger.info(f"添加新学科 {subject} 的提示词配置")
    except Exception as e:
        logger.error(f"加载学科提示词配置失败: {str(e)}")
        logger.error(traceback.format_exc())

if __name__ == '__main__':
    try:
        logger.info("启动Flask应用程序")
        app.run(debug=True, port=5001)
    except Exception as e:
        logger.error(f"启动应用程序时出错: {str(e)}")
        logger.error(traceback.format_exc())
