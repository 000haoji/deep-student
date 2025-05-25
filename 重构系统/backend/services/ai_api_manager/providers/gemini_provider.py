"""
Gemini AI提供商实现
"""
import json
import asyncio
import aiohttp
from typing import Dict, Any, Optional, AsyncGenerator
from .base import BaseAIProvider
from ..models import TaskType
from shared.utils.logger import LoggerMixin


class GeminiProvider(BaseAIProvider, LoggerMixin):
    """Gemini AI提供商"""
    
    def __init__(self, api_key: str, api_url: str = None, model_name: str = "gemini-pro", 
                 timeout: int = 30, max_retries: int = 3):
        super().__init__(api_key, api_url, model_name, timeout, max_retries)
        self.api_url = api_url or "https://generativelanguage.googleapis.com/v1beta"
        self.model_name = model_name or "gemini-pro"
    
    async def call_api(
        self, 
        task_type: TaskType,
        content: Dict[str, Any],
        stream: bool = False,
        max_tokens: int = 1000,
        temperature: float = 0.7,
        timeout: Optional[int] = None
    ) -> Dict[str, Any] | AsyncGenerator[Dict[str, Any], None]:
        """
        调用Gemini API
        
        Args:
            task_type: 任务类型
            content: 请求内容
            stream: 是否流式响应
            max_tokens: 最大token数
            temperature: 温度参数
            timeout: 超时时间
            
        Returns:
            API响应结果或流式生成器
        """
        prompt = self._build_prompt(task_type, content)
        
        # 构建请求数据
        request_data = {
            "contents": [
                {
                    "parts": [
                        {"text": prompt}
                    ]
                }
            ],
            "generationConfig": {
                "temperature": temperature,
                "maxOutputTokens": max_tokens,
                "topP": 0.8,
                "topK": 40
            }
        }
        
        # 根据任务类型选择模型
        if task_type in [TaskType.OCR, TaskType.PROBLEM_ANALYSIS] and "image" in content:
            model_name = "gemini-pro-vision"
            # 添加图像内容
            request_data["contents"][0]["parts"].append({
                "inline_data": {
                    "mime_type": "image/jpeg",
                    "data": content.get("image_data", "")
                }
            })
        else:
            model_name = self.model_name
        
        url = f"{self.api_url}/models/{model_name}:generateContent"
        
        headers = {
            "Content-Type": "application/json"
        }
        
        # Gemini使用URL参数传递API密钥
        params = {"key": self.api_key}
        
        if stream:
            return self._stream_request(url, headers, request_data, params, timeout)
        else:
            return await self._single_request(url, headers, request_data, params, timeout)
    
    async def _single_request(
        self,
        url: str,
        headers: Dict[str, str],
        data: Dict[str, Any],
        params: Dict[str, str],
        timeout: Optional[int]
    ) -> Dict[str, Any]:
        """发送单次请求"""
        timeout = timeout or self.timeout
        
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=timeout)) as session:
            for attempt in range(self.max_retries + 1):
                try:
                    async with session.post(url, headers=headers, json=data, params=params) as response:
                        if response.status == 200:
                            result = await response.json()
                            return self._parse_response(result)
                        else:
                            error_text = await response.text()
                            self.log_error(f"Gemini API error: {response.status} - {error_text}")
                            
                            if attempt < self.max_retries:
                                await asyncio.sleep(2 ** attempt)  # 指数退避
                                continue
                            else:
                                raise Exception(f"Gemini API error: {response.status} - {error_text}")
                                
                except asyncio.TimeoutError:
                    self.log_error(f"Gemini API timeout (attempt {attempt + 1})")
                    if attempt < self.max_retries:
                        await asyncio.sleep(2 ** attempt)
                        continue
                    else:
                        raise Exception("Gemini API request timeout")
                        
                except Exception as e:
                    self.log_error(f"Gemini API request failed: {e}")
                    if attempt < self.max_retries:
                        await asyncio.sleep(2 ** attempt)
                        continue
                    else:
                        raise
    
    async def _stream_request(
        self,
        url: str,
        headers: Dict[str, str],
        data: Dict[str, Any],
        params: Dict[str, str],
        timeout: Optional[int]
    ) -> AsyncGenerator[Dict[str, Any], None]:
        """发送流式请求"""
        # Gemini的流式端点
        stream_url = url.replace(":generateContent", ":streamGenerateContent")
        timeout = timeout or self.timeout
        
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=timeout)) as session:
            try:
                async with session.post(stream_url, headers=headers, json=data, params=params) as response:
                    if response.status == 200:
                        async for line in response.content:
                            if line:
                                try:
                                    chunk_data = json.loads(line.decode('utf-8'))
                                    parsed_chunk = self._parse_stream_chunk(chunk_data)
                                    if parsed_chunk:
                                        yield parsed_chunk
                                except json.JSONDecodeError:
                                    continue
                    else:
                        error_text = await response.text()
                        raise Exception(f"Gemini Stream API error: {response.status} - {error_text}")
                        
            except Exception as e:
                self.log_error(f"Gemini stream request failed: {e}")
                yield {"error": str(e)}
    
    def _parse_response(self, response: Dict[str, Any]) -> Dict[str, Any]:
        """解析Gemini API响应"""
        try:
            candidates = response.get("candidates", [])
            if not candidates:
                return {
                    "content": "",
                    "usage": {"total_tokens": 0, "prompt_tokens": 0, "completion_tokens": 0},
                    "finish_reason": "no_candidates"
                }
            
            candidate = candidates[0]
            content = ""
            
            # 提取文本内容
            if "content" in candidate and "parts" in candidate["content"]:
                parts = candidate["content"]["parts"]
                content = "".join(part.get("text", "") for part in parts)
            
            # 计算token使用量（Gemini不直接返回，需要估算）
            prompt_tokens = len(response.get("prompt", "").split()) * 1.3  # 估算
            completion_tokens = len(content.split()) * 1.3  # 估算
            total_tokens = int(prompt_tokens + completion_tokens)
            
            return {
                "content": content,
                "usage": {
                    "total_tokens": total_tokens,
                    "prompt_tokens": int(prompt_tokens),
                    "completion_tokens": int(completion_tokens)
                },
                "finish_reason": candidate.get("finishReason", "stop").lower()
            }
            
        except Exception as e:
            self.log_error(f"Failed to parse Gemini response: {e}")
            return {
                "content": "",
                "usage": {"total_tokens": 0, "prompt_tokens": 0, "completion_tokens": 0},
                "error": str(e)
            }
    
    def _parse_stream_chunk(self, chunk: Dict[str, Any]) -> Optional[Dict[str, Any]]:
        """解析流式响应块"""
        try:
            candidates = chunk.get("candidates", [])
            if not candidates:
                return None
            
            candidate = candidates[0]
            content = ""
            
            if "content" in candidate and "parts" in candidate["content"]:
                parts = candidate["content"]["parts"]
                content = "".join(part.get("text", "") for part in parts)
            
            if content:
                return {
                    "content": content,
                    "delta": content,
                    "finish_reason": candidate.get("finishReason")
                }
            
            return None
            
        except Exception as e:
            self.log_error(f"Failed to parse Gemini stream chunk: {e}")
            return None
    
    def _build_prompt(self, task_type: TaskType, content: Dict[str, Any]) -> str:
        """构建针对不同任务类型的提示词"""
        if task_type == TaskType.PROBLEM_ANALYSIS:
            return self._build_problem_analysis_prompt(content)
        elif task_type == TaskType.REVIEW_ANALYSIS:
            return self._build_review_analysis_prompt(content)
        elif task_type == TaskType.BATCH_PROBLEM_ANALYSIS:
            return self._build_batch_analysis_prompt(content)
        elif task_type == TaskType.OCR:
            return self._build_ocr_prompt(content)
        elif task_type == TaskType.SUMMARIZATION:
            return self._build_summarization_prompt(content)
        elif task_type == TaskType.TRANSLATION:
            return self._build_translation_prompt(content)
        else:
            return content.get("prompt", str(content))
    
    def _build_problem_analysis_prompt(self, content: Dict[str, Any]) -> str:
        """构建错题分析提示词"""
        problem_content = content.get("problem_content", "")
        subject = content.get("subject", "")
        user_answer = content.get("user_answer", "")
        correct_answer = content.get("correct_answer", "")
        
        prompt = f"""作为一名资深教师，请分析以下错题：

**题目内容：**
{problem_content}

**学科：** {subject}
**学生答案（如果提供）：** {user_answer if user_answer else "未提供"}
**正确答案（如果提供）：** {correct_answer if correct_answer else "未提供"}

请以JSON格式返回分析结果，确保整个输出是一个单一的、合法的JSON对象。JSON对象应包含以下键：
1. "knowledge_points": (List[str]) 一个包含该题目所涉及的核心知识点的字符串列表。
2. "error_analysis": (str) 对学生可能产生的常见错误进行详细分析。如果提供了学生答案，请针对性分析。如果未提供学生答案，请描述该题型的常见易错点。
3. "solution": (str) 详细的解题步骤和思路。
4. "difficulty_level": (int) 对题目难度的评估，范围从1（非常简单）到5（非常困难）。
5. "tags": (List[str]) 一个与题目内容和知识点相关的标签列表，用于分类和检索 (例如: "导数应用", "几何证明", "虚拟语气")。
6. "suggested_category": (str) 根据题目内容和所属学科，建议一个最合适的细分题目分类名称 (例如: 若学科为数学，分类可以是 "函数与极限", "解析几何", "概率统计" 等)。

请确保分析准确、详细且有指导意义。"""
        
        return prompt
    
    def _build_review_analysis_prompt(self, content: Dict[str, Any]) -> str:
        """构建复习分析提示词"""
        problems = content.get("problems", [])
        user_prefs = content.get("user_preferences", {})
        
        prompt = f"""作为一名智能学习助手，请基于以下错题数据生成个性化复习建议：

**错题数据：**
{json.dumps(problems, ensure_ascii=False, indent=2)}

**用户偏好：**
{json.dumps(user_prefs, ensure_ascii=False, indent=2)}

请基于艾宾浩斯遗忘曲线理论，以JSON格式返回复习建议，包含：
1. overview: 复习概览统计
2. schedule: 详细复习计划（包含日期、优先级、题目数量等）
3. weak_points: 薄弱知识点分析
4. study_tips: 个性化学习建议

请确保建议科学合理，符合记忆规律。"""
        
        return prompt
    
    def _build_batch_analysis_prompt(self, content: Dict[str, Any]) -> str:
        """构建批量分析提示词"""
        problems = content.get("problems", [])
        
        prompt = f"""请对以下多道错题进行批量分析：

**题目列表：**
{json.dumps(problems, ensure_ascii=False, indent=2)}

请以JSON格式返回批量分析结果，包含：
1. success_count: 成功分析的题目数量
2. fail_count: 分析失败的题目数量
3. weak_knowledge_points: 主要薄弱知识点
4. error_types: 错误类型分布统计
5. study_suggestions: 整体学习建议
6. details: 每道题的详细分析结果

请确保分析全面且有针对性。"""
        
        return prompt
    
    def _build_ocr_prompt(self, content: Dict[str, Any]) -> str:
        """构建OCR提示词"""
        return "请识别图片中的文字内容，包括题目、公式、表格等，保持原有格式。"
    
    def _build_summarization_prompt(self, content: Dict[str, Any]) -> str:
        """构建摘要提示词"""
        text = content.get("text", "")
        return f"请对以下内容进行摘要：\n\n{text}\n\n要求简洁明了，突出重点。"
    
    def _build_translation_prompt(self, content: Dict[str, Any]) -> str:
        """构建翻译提示词"""
        text = content.get("text", "")
        target_lang = content.get("target_language", "中文")
        return f"请将以下内容翻译为{target_lang}：\n\n{text}"
    
    async def check_health(self) -> bool:
        """检查Gemini API健康状态"""
        try:
            test_content = {
                "problem_content": "测试题目",
                "subject": "数学"
            }
            
            result = await self.call_api(
                task_type=TaskType.PROBLEM_ANALYSIS,
                content=test_content,
                max_tokens=100,
                timeout=10
            )
            
            return "content" in result and not result.get("error")
            
        except Exception as e:
            self.log_error(f"Gemini health check failed: {e}")
            return False
    
    def calculate_cost(self, usage: Dict[str, Any]) -> float:
        """
        计算Gemini API调用成本
        
        Gemini Pro定价（示例，实际以官方为准）：
        - 输入: $0.0005 per 1K tokens
        - 输出: $0.0015 per 1K tokens
        """
        prompt_tokens = usage.get("prompt_tokens", 0)
        completion_tokens = usage.get("completion_tokens", 0)
        
        input_cost = (prompt_tokens / 1000) * 0.0005
        output_cost = (completion_tokens / 1000) * 0.0015
        
        return input_cost + output_cost
    
    def get_model_info(self) -> Dict[str, Any]:
        """获取模型信息"""
        return {
            "provider": "Gemini",
            "model": self.model_name,
            "capabilities": ["text", "vision", "function_calling"],
            "max_tokens": 30720,  # Gemini Pro的最大token数
            "supports_streaming": True,
            "supports_vision": True if "vision" in self.model_name else False
        }
