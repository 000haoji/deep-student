"""
DeepSeek API适配器模块 - 专门处理DeepSeek API的请求和响应格式
"""
import json
import time
import logging
import requests
import traceback
import re
from config import SUBJECT_ANALYSIS_PROMPTS
from core.ai_analysis.combined_model import get_subject_system_prompt

logger = logging.getLogger(__name__)

class DeepSeekAdapter:
    """DeepSeek API适配器，处理请求、响应和格式化"""
    
    def __init__(self, api_key, api_url, model_name, timeout=60):  # 缩短超时时间
        """初始化适配器
        
        Args:
            api_key: DeepSeek API密钥
            api_url: API端点URL
            model_name: 使用的模型名称
            timeout: 请求超时时间(秒)
        """
        self.api_key = api_key
        self.api_url = api_url
        self.model_name = model_name
        self.timeout = timeout
        # 创建一个持久化的session对象
        self.session = self._create_retry_session()
    
    def _create_retry_session(self, retries=3, backoff_factor=0.5):
        """创建带有重试功能的请求会话"""
        from requests.adapters import HTTPAdapter
        from urllib3.util.retry import Retry
        
        retry = Retry(
            total=retries,
            read=retries,
            connect=retries,
            backoff_factor=backoff_factor,
            status_forcelist=(401, 403, 429, 500, 502, 503, 504),  # 添加401, 403, 429状态码
            allowed_methods=["HEAD", "GET", "POST"]
        )
        session = requests.Session()
        adapter = HTTPAdapter(max_retries=retry)
        session.mount('http://', adapter)
        session.mount('https://', adapter)
        return session
    
    def call_api(self, prompt, system_message=None, temperature=0.5, max_tokens=2000, 
                 json_mode=False, stream=False, enable_reasoning=False, subject=None):
        """
        调用DeepSeek API进行文本生成

        Args:
            prompt: 提示词
            system_message: 系统消息
            temperature: 温度参数
            max_tokens: 最大生成token数
            json_mode: 是否启用JSON模式输出
            stream: 是否使用流式请求
            enable_reasoning: 是否启用推理过程显示
            subject: 学科名称，用于获取特定学科的提示词
        
        Returns:
            tuple: (成功标志, 响应内容)
        """
        # 默认系统消息
        if system_message is None:
            # 如果提供了学科，尝试获取学科特定的提示词
            if subject:
                subject_prompt = get_subject_system_prompt(subject)
                if subject_prompt:
                    system_message = subject_prompt
                    logger.info(f"使用学科 '{subject}' 的特定提示词: {system_message[:30]}...")
                else:
                    # 如果没有找到学科特定的提示词，使用默认提示词
                    if (json_mode):
                        system_message = "你是一个JSON格式的数据API，始终以JSON格式返回用户要求的数据，不要添加任何额外解释。"
                    else:
                        system_message = "你是一位专业的AI助手，请提供准确的回答。"
                    logger.info(f"未找到学科 '{subject}' 的特定提示词，使用默认提示词")
            else:
                # 没有提供学科，使用默认提示词
                if (json_mode):
                    system_message = "你是一个JSON格式的数据API，始终以JSON格式返回用户要求的数据，不要添加任何额外解释。"
                else:
                    system_message = "你是一位专业的AI助手，请提供准确的回答。"
        
        # 构建JSON提示（如果需要）
        if json_mode:
            prompt = f"""
            请严格按照要求返回JSON格式的数据，不要包含任何解释和其他文本。
            
            {prompt}
            """
        
        # 构建API请求头
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self.api_key}"
        }
        
        # API请求体
        request_data = {
            "model": self.model_name,
            "messages": [
                {"role": "system", "content": system_message},
                {"role": "user", "content": prompt}
            ],
            "temperature": temperature,
            "max_tokens": max_tokens,
            "stream": stream
        }
        
        # 如果启用推理过程，添加enable_reasoning参数
        if enable_reasoning:
            request_data["enable_reasoning"] = True
        
        # 处理流式请求
        if stream:
            return self._handle_streaming_request(headers, request_data)
        
        # 处理常规请求
        try:
            logger.info(f"向DeepSeek API发送非流式请求: {self.api_url}")
            start_time = time.time()
            
            response = self.session.post(
                self.api_url,
                headers=headers,
                json=request_data,
                timeout=self.timeout
            )
            
            elapsed_time = time.time() - start_time
            logger.info(f"请求耗时: {elapsed_time:.2f}秒")
            
            if response.status_code != 200:
                logger.error(f"DeepSeek API请求失败，状态码: {response.status_code}")
                logger.error(f"错误响应: {response.text}")
                return False, f"API请求失败，状态码: {response.status_code}"
            
            # 解析响应
            response_data = response.json()
            
            if "choices" in response_data and len(response_data["choices"]) > 0:
                result_text = response_data["choices"][0]["message"]["content"]
                logger.info(f"DeepSeek响应文本: {result_text[:100]}...")
                
                return True, result_text
            else:
                logger.error("DeepSeek响应格式不符合预期")
                return False, "API响应格式不符合预期"
                
        except requests.exceptions.Timeout:
            logger.error(f"DeepSeek API请求超时 ({self.timeout}秒)")
            return False, f"API请求超时 ({self.timeout}秒)"
        except requests.exceptions.ConnectionError as e:
            logger.error(f"DeepSeek API连接错误: {str(e)}")
            return False, f"API连接错误: {str(e)}"
        except Exception as e:
            logger.error(f"DeepSeek API请求异常: {str(e)}")
            logger.error(traceback.format_exc())
            return False, f"API请求异常: {str(e)}"
    
    def _handle_streaming_request(self, headers, request_data):
        """处理流式请求
        
        Args:
            headers: 请求头
            request_data: 请求数据
            
        Returns:
            tuple: (成功标志, 流式响应对象)
        """
        try:
            logger.info(f"向DeepSeek API发送流式请求: {self.api_url}")
            
            # 设置流式响应
            request_data["stream"] = True
            
            # 发送请求，设置stream=True以获取流式响应
            response = self.session.post(
                self.api_url,
                headers=headers,
                json=request_data,
                stream=True,
                timeout=self.timeout
            )
            
            if response.status_code != 200:
                logger.error(f"DeepSeek API流式请求失败，状态码: {response.status_code}")
                logger.error(f"错误响应: {response.text}")
                return False, f"API流式请求失败，状态码: {response.status_code}"
            
            # 返回流式响应对象，由调用方处理
            return True, response
            
        except requests.exceptions.Timeout:
            logger.error(f"DeepSeek API流式请求超时 ({self.timeout}秒)")
            return False, f"API流式请求超时 ({self.timeout}秒)"
        except requests.exceptions.ConnectionError as e:
            logger.error(f"DeepSeek API流式连接错误: {str(e)}")
            return False, f"API流式连接错误: {str(e)}"
        except Exception as e:
            logger.error(f"DeepSeek API流式请求异常: {str(e)}")
            logger.error(traceback.format_exc())
            return False, f"API流式请求异常: {str(e)}"
    
    def request_json(self, prompt, system_message=None, temperature=0.5, max_tokens=2000, subject=None):
        """请求并返回JSON格式数据
        
        Args:
            prompt: 用户提示内容
            system_message: 系统消息
            temperature: 温度参数
            max_tokens: 最大生成token数量
            subject: 学科名称，用于获取特定学科的提示词
        
        Returns:
            tuple: (成功标志, JSON对象或错误消息)
        """
        # 使用JSON模式请求
        success, result_text = self.call_api(
            prompt, system_message, temperature, max_tokens, json_mode=True, subject=subject
        )
        
        if not success:
            return False, result_text
        
        # 尝试解析为JSON
        try:
            # 直接解析
            try:
                json_result = json.loads(result_text)
                return True, json_result
            except json.JSONDecodeError:
                # 尝试提取JSON部分
                json_result = self._extract_json(result_text)
                if json_result:
                    return True, json_result
                return False, "无法解析响应为JSON格式"
        except Exception as e:
            logger.error(f"JSON解析异常: {str(e)}")
            logger.error(traceback.format_exc())
            return False, f"JSON解析异常: {str(e)}"
    
    def _extract_json(self, text):
        """从文本中提取JSON对象
        
        Args:
            text: 包含JSON的文本
        
        Returns:
            dict: 提取的JSON对象，或None
        """
        # 尝试多种JSON提取模式
        # 1. 匹配标准JSON代码块
        json_block_pattern = r'```(?:json)?\s*([\s\S]+?)\s*```'
        matches = re.findall(json_block_pattern, text)
        for match in matches:
            try:
                return json.loads(match.strip())
            except:
                continue
        
        # 2. 查找最外层的大括号对
        json_start = text.find('{')
        json_end = text.rfind('}') + 1
        if json_start >= 0 and json_end > json_start:
            try:
                json_str = text[json_start:json_end]
                # 修复常见错误
                json_str = json_str.replace('\n', '\\n')
                return json.loads(json_str)
            except:
                pass
        
        return None
