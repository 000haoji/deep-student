"""
DeepSeek AI提供商实现
"""
import json
import asyncio
import aiohttp
from typing import Dict, Any, Optional, AsyncGenerator
from .base import BaseAIProvider
from ..models import TaskType


class DeepSeekProvider(BaseAIProvider):
    """DeepSeek AI提供商"""
    
    def __init__(self, api_key: str, api_url: str = None, model_name: str = "deepseek-chat", 
                 timeout: int = 30, max_retries: int = 3):
        super().__init__(api_key, api_url, model_name, timeout, max_retries)
        self.api_url = api_url or "https://api.deepseek.com/v1"
        self.model_name = model_name or "deepseek-chat"
    
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
        调用DeepSeek API
        
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
        messages = self._build_messages(task_type, content)
        
        # 根据任务类型选择模型
        model_name = self._select_model(task_type)
        
        # 构建请求数据
        request_data = {
            "model": model_name,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
            "stream": stream
        }
        
        # 针对代码任务添加特殊参数
        if task_type in [TaskType.CODE_GENERATION, TaskType.CODE_REVIEW]:
            request_data.update({
                "top_p": 0.95,
                "frequency_penalty": 0.1,
                "presence_penalty": 0.1
            })
        
        url = f"{self.api_url}/chat/completions"
        
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {self.api_key}"
        }
        
        if stream:
            return self._stream_request(url, headers, request_data, timeout)
        else:
            return await self._single_request(url, headers, request_data, timeout)
    
    def _select_model(self, task_type: TaskType) -> str:
        """根据任务类型选择合适的模型"""
        if task_type in [TaskType.CODE_GENERATION, TaskType.CODE_REVIEW]:
            return "deepseek-coder"
        elif task_type == TaskType.MATH_SOLVING:
            return "deepseek-math"
        else:
            return self.model_name
    
    async def _single_request(
        self,
        url: str,
        headers: Dict[str, str],
        data: Dict[str, Any],
        timeout: Optional[int]
    ) -> Dict[str, Any]:
        """发送单次请求"""
        timeout = timeout or self.timeout
        
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=timeout)) as session:
            for attempt in range(self.max_retries + 1):
                try:
                    async with session.post(url, headers=headers, json=data) as response:
                        if response.status == 200:
                            result = await response.json()
                            return self._parse_response(result)
                        else:
                            error_text = await response.text()
                            self.log_error(f"DeepSeek API error: {response.status} - {error_text}")
                            
                            if attempt < self.max_retries:
                                await asyncio.sleep(2 ** attempt)  # 指数退避
                                continue
                            else:
                                raise Exception(f"DeepSeek API error: {response.status} - {error_text}")
                                
                except asyncio.TimeoutError:
                    self.log_error(f"DeepSeek API timeout (attempt {attempt + 1})")
                    if attempt < self.max_retries:
                        await asyncio.sleep(2 ** attempt)
                        continue
                    else:
                        raise Exception("DeepSeek API request timeout")
                        
                except Exception as e:
                    self.log_error(f"DeepSeek API request failed: {e}")
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
        timeout: Optional[int]
    ) -> AsyncGenerator[Dict[str, Any], None]:
        """发送流式请求"""
        timeout = timeout or self.timeout
        
        async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=timeout)) as session:
            try:
                async with session.post(url, headers=headers, json=data) as response:
                    if response.status == 200:
                        async for line in response.content:
                            if line:
                                line_text = line.decode('utf-8').strip()
                                if line_text.startswith("data: "):
                                    data_text = line_text[6:]  # 移除 "data: " 前缀
                                    
                                    if data_text == "[DONE]":
                                        break
                                    
                                    try:
                                        chunk_data = json.loads(data_text)
                                        parsed_chunk = self._parse_stream_chunk(chunk_data)
                                        if parsed_chunk:
                                            yield parsed_chunk
                                    except json.JSONDecodeError:
                                        continue
                    else:
                        error_text = await response.text()
                        raise Exception(f"DeepSeek Stream API error: {response.status} - {error_text}")
                        
            except Exception as e:
                self.log_error(f"DeepSeek stream request failed: {e}")
                yield {"error": str(e)}
    
    def _parse_response(self, response: Dict[str, Any]) -> Dict[str, Any]:
        """解析DeepSeek API响应"""
        try:
            choices = response.get("choices", [])
            if not choices:
                return {
                    "content": "",
                    "usage": {"total_tokens": 0, "prompt_tokens": 0, "completion_tokens": 0},
                    "finish_reason": "no_choices"
                }
            
            choice = choices[0]
            message = choice.get("message", {})
            content = message.get("content", "")
            
            # 提取usage信息
            usage = response.get("usage", {})
            
            return {
                "content": content,
                "usage": {
                    "total_tokens": usage.get("total_tokens", 0),
                    "prompt_tokens": usage.get("prompt_tokens", 0),
                    "completion_tokens": usage.get("completion_tokens", 0)
                },
                "finish_reason": choice.get("finish_reason", "stop")
            }
            
        except Exception as e:
            self.log_error(f"Failed to parse DeepSeek response: {e}")
            return {
                "content": "",
                "usage": {"total_tokens": 0, "prompt_tokens": 0, "completion_tokens": 0},
                "error": str(e)
            }
    
    def _parse_stream_chunk(self, chunk: Dict[str, Any]) -> Optional[Dict[str, Any]]:
        """解析流式响应块"""
        try:
            choices = chunk.get("choices", [])
            if not choices:
                return None
            
            choice = choices[0]
            delta = choice.get("delta", {})
            content = delta.get("content", "")
            
            if content:
                return {
                    "content": content,
                    "delta": content,
                    "finish_reason": choice.get("finish_reason")
                }
            
            return None
            
        except Exception as e:
            self.log_error(f"Failed to parse DeepSeek stream chunk: {e}")
            return None
    
    def _build_messages(self, task_type: TaskType, content: Dict[str, Any]) -> list:
        """构建针对不同任务类型的消息"""
        if task_type == TaskType.PROBLEM_ANALYSIS:
            return self._build_problem_analysis_messages(content)
        elif task_type == TaskType.REVIEW_ANALYSIS:
            return self._build_review_analysis_messages(content)
        elif task_type == TaskType.BATCH_PROBLEM_ANALYSIS:
            return self._build_batch_analysis_messages(content)
        elif task_type == TaskType.CODE_GENERATION:
            return self._build_code_generation_messages(content)
        elif task_type == TaskType.CODE_REVIEW:
            return self._build_code_review_messages(content)
        elif task_type == TaskType.MATH_SOLVING:
            return self._build_math_solving_messages(content)
        elif task_type == TaskType.SUMMARIZATION:
            return self._build_summarization_messages(content)
        elif task_type == TaskType.TRANSLATION:
            return self._build_translation_messages(content)
        else:
            return [{"role": "user", "content": str(content)}]
    
    def _build_problem_analysis_messages(self, content: Dict[str, Any]) -> list:
        """构建错题分析消息"""
        problem_content = content.get("problem_content", "")
        subject = content.get("subject", "")
        user_answer = content.get("user_answer", "")
        correct_answer = content.get("correct_answer", "")
        
        system_prompt = """你是一名资深教师，擅长分析学生的错题并提供有针对性的指导。请以专业的角度分析错题，提供详细的解析和学习建议。"""
        
        user_prompt = f"""请分析以下错题：

**题目内容：** {problem_content}
**学科：** {subject}
**学生答案：** {user_answer}
**正确答案：** {correct_answer}

请以JSON格式返回分析结果，包含以下字段：
1. knowledge_points: 涉及的知识点列表
2. error_analysis: 详细的错误原因分析
3. study_suggestions: 学习建议列表
4. difficulty_level: 难度等级(1-5)
5. solution_steps: 解题步骤（如果适用）
6. key_concepts: 关键概念解释

请确保分析准确、详细且有指导意义。"""
        
        return [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ]
    
    def _build_review_analysis_messages(self, content: Dict[str, Any]) -> list:
        """构建复习分析消息"""
        problems = content.get("problems", [])
        user_prefs = content.get("user_preferences", {})
        
        system_prompt = """你是一名智能学习助手，专门帮助学生制定科学的复习计划。你需要基于艾宾浩斯遗忘曲线理论和学生的错题数据，生成个性化的复习建议。"""
        
        user_prompt = f"""请基于以下错题数据生成个性化复习建议：

**错题数据：**
{json.dumps(problems, ensure_ascii=False, indent=2)}

**用户偏好：**
{json.dumps(user_prefs, ensure_ascii=False, indent=2)}

请以JSON格式返回复习建议，包含：
1. overview: 复习概览统计
2. schedule: 详细复习计划（包含日期、优先级、题目数量等）
3. weak_points: 薄弱知识点分析
4. study_tips: 个性化学习建议
5. memory_strategy: 记忆策略建议

请确保建议科学合理，符合记忆规律。"""
        
        return [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ]
    
    def _build_batch_analysis_messages(self, content: Dict[str, Any]) -> list:
        """构建批量分析消息"""
        problems = content.get("problems", [])
        
        system_prompt = """你是一名教育专家，擅长对多道错题进行综合分析，找出学生的共同问题和薄弱环节。"""
        
        user_prompt = f"""请对以下多道错题进行批量分析：

**题目列表：**
{json.dumps(problems, ensure_ascii=False, indent=2)}

请以JSON格式返回批量分析结果，包含：
1. success_count: 成功分析的题目数量
2. fail_count: 分析失败的题目数量
3. weak_knowledge_points: 主要薄弱知识点
4. error_types: 错误类型分布统计
5. study_suggestions: 整体学习建议
6. details: 每道题的详细分析结果
7. common_patterns: 共同错误模式

请确保分析全面且有针对性。"""
        
        return [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ]
    
    def _build_code_generation_messages(self, content: Dict[str, Any]) -> list:
        """构建代码生成消息"""
        requirement = content.get("requirement", "")
        language = content.get("language", "Python")
        
        system_prompt = f"""你是一名专业的{language}开发工程师，能够根据需求编写高质量、可维护的代码。请确保代码具有良好的结构、适当的注释和错误处理。"""
        
        user_prompt = f"""请根据以下需求生成{language}代码：

{requirement}

请提供：
1. 完整的代码实现
2. 代码解释和注释
3. 使用示例
4. 可能的优化建议"""
        
        return [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ]
    
    def _build_code_review_messages(self, content: Dict[str, Any]) -> list:
        """构建代码审查消息"""
        code = content.get("code", "")
        language = content.get("language", "Python")
        
        system_prompt = f"""你是一名资深的{language}代码审查专家，能够发现代码中的问题并提供改进建议。请从代码质量、性能、安全性、可维护性等方面进行全面评估。"""
        
        user_prompt = f"""请审查以下{language}代码：

```{language.lower()}
{code}
```

请提供：
1. 代码质量评估
2. 发现的问题和建议
3. 性能优化建议
4. 最佳实践建议
5. 改进后的代码（如有必要）"""
        
        return [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ]
    
    def _build_math_solving_messages(self, content: Dict[str, Any]) -> list:
        """构建数学题解题消息"""
        problem = content.get("problem", "")
        
        system_prompt = """你是一名数学专家，能够解决各种数学问题并提供详细的解题步骤。请用清晰的逻辑和严谨的推理来解决问题。"""
        
        user_prompt = f"""请解决以下数学问题：

{problem}

请提供：
1. 完整的解题步骤
2. 每一步的详细解释
3. 关键知识点说明
4. 解题思路总结
5. 类似题目的解题方法"""
        
        return [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ]
    
    def _build_summarization_messages(self, content: Dict[str, Any]) -> list:
        """构建摘要消息"""
        text = content.get("text", "")
        
        system_prompt = """你是一名专业的内容摘要师，能够提取文本的核心信息并生成简洁明了的摘要。"""
        
        user_prompt = f"""请对以下内容进行摘要：

{text}

要求：
1. 突出重点信息
2. 保持逻辑清晰
3. 简洁明了
4. 保留关键细节"""
        
        return [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ]
    
    def _build_translation_messages(self, content: Dict[str, Any]) -> list:
        """构建翻译消息"""
        text = content.get("text", "")
        target_lang = content.get("target_language", "中文")
        source_lang = content.get("source_language", "")
        
        system_prompt = f"""你是一名专业翻译师，能够准确地将文本翻译为{target_lang}，保持原文的意思和语调。"""
        
        lang_info = f"从{source_lang}" if source_lang else ""
        user_prompt = f"""请将以下内容{lang_info}翻译为{target_lang}：

{text}

要求：
1. 准确传达原文意思
2. 符合目标语言习惯
3. 保持原文语调和风格
4. 专业术语准确翻译"""
        
        return [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ]
    
    async def check_health(self) -> bool:
        """检查DeepSeek API健康状态"""
        try:
            test_messages = [
                {"role": "user", "content": "你好，这是一个健康检查测试。"}
            ]
            
            request_data = {
                "model": self.model_name,
                "messages": test_messages,
                "max_tokens": 50
            }
            
            result = await self._single_request(
                f"{self.api_url}/chat/completions",
                {
                    "Content-Type": "application/json",
                    "Authorization": f"Bearer {self.api_key}"
                },
                request_data,
                10
            )
            
            return "content" in result and not result.get("error")
            
        except Exception as e:
            self.log_error(f"DeepSeek health check failed: {e}")
            return False
    
    def calculate_cost(self, usage: Dict[str, Any]) -> float:
        """
        计算DeepSeek API调用成本
        
        DeepSeek定价（示例，实际以官方为准）：
        - deepseek-chat: $0.0014 per 1M tokens (输入), $0.0028 per 1M tokens (输出)
        - deepseek-coder: $0.0014 per 1M tokens (输入), $0.0028 per 1M tokens (输出)
        """
        prompt_tokens = usage.get("prompt_tokens", 0)
        completion_tokens = usage.get("completion_tokens", 0)
        
        # 根据模型调整价格
        if "coder" in self.model_name or "math" in self.model_name:
            input_rate = 0.0014 / 1000000  # 每token价格
            output_rate = 0.0028 / 1000000
        else:
            input_rate = 0.0014 / 1000000
            output_rate = 0.0028 / 1000000
        
        input_cost = prompt_tokens * input_rate
        output_cost = completion_tokens * output_rate
        
        return input_cost + output_cost
    
    def get_model_info(self) -> Dict[str, Any]:
        """获取模型信息"""
        capabilities = ["text"]
        
        if "coder" in self.model_name:
            capabilities.append("code")
        if "math" in self.model_name:
            capabilities.append("math")
        
        return {
            "provider": "DeepSeek",
            "model": self.model_name,
            "capabilities": capabilities,
            "max_tokens": 32768,  # DeepSeek的最大token数
            "supports_streaming": True,
            "supports_vision": False,
            "supports_function_calling": True if "chat" in self.model_name else False
        } 