"""
DeepSeek AI提供商实现
"""
import json
import asyncio
import aiohttp
from typing import Dict, Any, Optional, AsyncGenerator, Tuple, List, Union

from .base import BaseAIProvider
from ..schemas import AIRequestSchema
from ..models import TaskType, AIProvider as AIProviderEnum
from shared.utils.logger import LoggerMixin

DEFAULT_DEEPSEEK_API_URL = "https://api.deepseek.com/v1"

class DeepSeekProvider(BaseAIProvider, LoggerMixin):
    """DeepSeek AI提供商"""

    def __init__(
        self,
        model_name: str, # e.g., "deepseek-chat", "deepseek-coder"
        provider_name: AIProviderEnum, # Should be AIProviderEnum.DEEPSEEK
        api_key: Optional[str],
        base_url: Optional[str] = None,
        timeout_seconds: int = 120,
        max_retries: int = 3, # Deepseek current default was 5, can adjust
        rpm_limit: Optional[int] = None,
        tpm_limit: Optional[int] = None,
        custom_headers: Optional[Dict[str, str]] = None,
        max_tokens_limit: Optional[int] = None, # e.g., 32768
    ):
        super().__init__(
            model_name=model_name,
            provider_name=provider_name,
            api_key=api_key,
            base_url=base_url or DEFAULT_DEEPSEEK_API_URL,
            timeout_seconds=timeout_seconds,
            max_retries=max_retries,
            rpm_limit=rpm_limit,
            tpm_limit=tpm_limit,
            custom_headers=custom_headers,
            max_tokens_limit=max_tokens_limit
        )
        self._session: Optional[aiohttp.ClientSession] = None
        # DeepSeek API endpoint is typically /v1/chat/completions
        self.api_endpoint = f"{self.base_url}/chat/completions"


    async def _get_session(self) -> aiohttp.ClientSession:
        if self._session is None or self._session.closed:
            timeout = aiohttp.ClientTimeout(total=self.timeout_seconds)
            self._session = aiohttp.ClientSession(timeout=timeout) # Headers added per-request
        return self._session

    async def close(self) -> None:
        if self._session and not self._session.closed:
            await self._session.close()
            self._session = None

    def _select_effective_model(self, requested_task_type: TaskType) -> str:
        """根据任务类型选择实际使用的模型名称。"""
        # This uses self.model_name (the configured model for this provider instance)
        # as a base, and potentially overrides it for specific task types if the
        # base model isn't specialized (e.g. if base is deepseek-chat).
        if requested_task_type in [TaskType.CODE_GENERATION, TaskType.CODE_REVIEW] and "coder" not in self.model_name:
            return "deepseek-coder" # Prefer coder model for code tasks
        # Add other specific model selections if needed, e.g., for 'deepseek-math'
        # if requested_task_type == TaskType.MATH_SOLVING and "math" not in self.model_name:
        #     return "deepseek-math"
        return self.model_name # Default to the configured model


    def _build_messages(self, request: AIRequestSchema) -> List[Dict[str, Any]]:
        """构建与OpenAI兼容的消息列表。"""
        messages: List[Dict[str, Any]] = []
        task_type = request.task_type
        context = request.context or {}
        
        # System Prompt
        system_content = "You are a helpful AI assistant." # Default
        if task_type == TaskType.PROBLEM_ANALYSIS:
            subject = context.get("subject", "通用")
            system_content = f"""You are an expert educational assistant specializing in {subject} problems. 
Your task is to analyze a student's problem and provide a detailed, structured JSON output.
The JSON object must include: "knowledge_points" (List[str]), "difficulty_level" (int 1-5), "error_analysis" (str), "solution" (str), "tags" (List[str]), "suggested_category" (str).
Ensure your entire response is a single, valid JSON object without any markdown or explanations outside the JSON structure.
"""
        elif task_type == TaskType.PROBLEM_IMAGE_TO_STRUCTURED_JSON:
            # DeepSeek (non-visual models) can only process text descriptions.
            # The prompt should reflect this limitation.
            system_content = """You are an AI assistant analyzing a textual description of a problem (potentially transcribed from an image).
Your task is to output a single, valid JSON object with fields: "session_id" (placeholder), "raw_ocr_text" (str, if the input is OCR text), "extracted_content" (str), "suggested_subject" (enum), "preliminary_category" (str), "preliminary_tags" (List[str]), "detected_language" (Optional[str]), "original_image_ref" (placeholder, if applicable).
Prioritize "subject_hint" from user context if relevant.
Ensure the output is ONLY the JSON object. No explanations or markdown formatting.
"""
        elif task_type == TaskType.PROBLEM_INTERACTIVE_DEEP_ANALYSIS:
            system_content = """You are an expert AI assistant for interactive problem refinement and deep analysis.
Respond conversationally and help the user complete all necessary fields for a problem record.
If the user asks for a JSON representation of the current state of the problem, provide it based on the conversation.
"""
        elif task_type == TaskType.CODE_GENERATION:
            language = context.get("language", "Python")
            system_content = f"You are an expert {language} programmer. Generate clean, efficient, and well-documented code."
        elif task_type == TaskType.CODE_REVIEW:
            language = context.get("language", "Python")
            system_content = f"You are a meticulous code reviewer. Analyze the given {language} code for quality, bugs, performance, and best practices."
        
        if system_content:
            messages.append({"role": "system", "content": system_content})

        # History
        if request.history:
            for entry in request.history:
                role = "assistant" if entry.get("role") == "ai" else entry.get("role", "user")
                messages.append({"role": role, "content": entry.get("content", "")})
        
        # Current User Prompt / Context
        # For DeepSeek, image data is not directly supported by standard chat/coder models.
        # If image_base64 or image_url is present, it should ideally be converted to text by OCR first,
        # and that text should be in request.prompt or request.context.
        user_prompt_text = request.prompt or ""

        if task_type == TaskType.PROBLEM_ANALYSIS and not user_prompt_text: # Construct from context
            problem_text_content = context.get("text", "")
            user_answer = context.get("user_answer")
            correct_answer = context.get("correct_answer")
            user_prompt_text = f"Problem: {problem_text_content}\n"
            if user_answer: user_prompt_text += f"Student's Answer: {user_answer}\n"
            if correct_answer: user_prompt_text += f"Correct Answer: {correct_answer}"

        elif task_type == TaskType.PROBLEM_IMAGE_TO_STRUCTURED_JSON and not user_prompt_text:
            # Expects OCR text in request.prompt or context.text
            ocr_text = context.get("text", context.get("raw_ocr_text", ""))
            subject_hint = context.get("subject_hint")
            user_prompt_text = f"Analyze this problem description (from OCR): {ocr_text}\n"
            if subject_hint: user_prompt_text += f"User's subject hint: {subject_hint}\n"
            user_prompt_text += "Provide the structured JSON output as per system instructions."
            if request.image_base64 or request.image_url:
                 self.log_warning("DeepSeekProvider received image data for PROBLEM_IMAGE_TO_STRUCTURED_JSON, but will only process textual description. Ensure OCR text is in request.prompt or context.text.")


        if request.image_base64 or request.image_url:
            # Note: This provider does not process images directly.
            # If text description of image is not in prompt, this might be an issue.
            if not user_prompt_text: user_prompt_text = "An image was provided, but I can only process text. Please describe the image or provide its OCR text."
            self.log_info("DeepSeekProvider: Image data found in request, but this provider primarily handles text. Ensure textual description is available.")


        if user_prompt_text:
            messages.append({"role": "user", "content": user_prompt_text})
        elif not messages: # Ensure at least one user message if history and system prompt are also empty
            messages.append({"role": "user", "content": "Hello."})
            
        return messages

    def _parse_deepseek_response(self, api_response: Dict[str, Any], request: AIRequestSchema) -> Dict[str, Any]:
        """Parses the non-streaming DeepSeek API response (similar to OpenAI)."""
        output_dict: Dict[str, Any] = {
            "content_text": None, "content_json": None, "usage": {}, 
            "error": None, "error_type": None
        }
        try:
            if "choices" in api_response and api_response["choices"]:
                choice = api_response["choices"][0]
                message_content = choice.get("message", {}).get("content")
                output_dict["content_text"] = message_content

                if (request.output_format == "json_object" or \
                    request.task_type == TaskType.PROBLEM_ANALYSIS or \
                    request.task_type == TaskType.PROBLEM_IMAGE_TO_STRUCTURED_JSON) \
                    and message_content:
                    try:
                        output_dict["content_json"] = json.loads(message_content)
                    except json.JSONDecodeError:
                        self.log_warning(f"DeepSeek response for JSON task not valid JSON: {message_content[:200]}")
                
                output_dict["usage"] = api_response.get("usage", {
                    "prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0
                })
                
                finish_reason = choice.get("finish_reason")
                if finish_reason and finish_reason != "stop":
                    output_dict["error"] = f"Generation stopped due to: {finish_reason}"
                    output_dict["error_type"] = "length" if finish_reason == "length" else "generation_stopped_error"

            elif "error" in api_response: # Top-level error
                err = api_response["error"]
                output_dict["error"] = err.get("message", "Unknown DeepSeek API error")
                output_dict["error_type"] = f"deepseek_api_error_{err.get('type', 'UNKNOWN')}"
            
            return output_dict
        except Exception as e:
            self.log_error(f"Error parsing DeepSeek response: {e}. Response: {api_response}", exc_info=True)
            return {"error": f"Internal error parsing response: {str(e)}", "error_type": "response_parsing_error", "usage":{}}


    async def execute_task(self, request: AIRequestSchema) -> Dict[str, Any]:
        if not self.api_key:
            return {"error": "API key not configured.", "error_type": "config_error", "usage": {}}

        effective_model_name = self._select_effective_model(request.task_type)
        messages = self._build_messages(request)
        
        payload: Dict[str, Any] = {
            "model": effective_model_name,
            "messages": messages,
            "temperature": request.temperature if request.temperature is not None else 0.7,
            "stream": False
        }
        if request.max_output_tokens:
            payload["max_tokens"] = request.max_output_tokens
        
        # DeepSeek specific: JSON mode via prompt, not a response_format parameter usually.
        # The system prompts for JSON tasks already instruct this.

        headers = {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json"
        }
        if self.custom_headers: headers.update(self.custom_headers)

        session = await self._get_session()
        retries = 0
        last_exception = None

        while retries <= self.max_retries:
            try:
                async with session.post(self.api_endpoint, json=payload, headers=headers) as response:
                    response_text = await response.text()
                    try: response_json = json.loads(response_text)
                    except json.JSONDecodeError:
                        last_exception = Exception(f"DeepSeek API non-JSON response ({response.status}): {response_text[:500]}")
                        response_json = {"error": {"message": str(last_exception), "type": "JSON_DECODE_ERROR"}}

                    if response.status == 200:
                        return self._parse_deepseek_response(response_json, request)
                    else:
                        error_detail = response_json.get("error", {})
                        msg = error_detail.get("message", f"Unknown API error: {response_text[:200]}")
                        err_type = error_detail.get("type", str(response.status))
                        last_exception = Exception(f"DeepSeek API Error {err_type}: {msg}")
            except aiohttp.ClientError as e:
                last_exception = e
            
            retries += 1
            self.log_warning(f"DeepSeek attempt {retries}/{self.max_retries + 1} for model {effective_model_name} failed: {last_exception}")
            if retries <= self.max_retries:
                await asyncio.sleep(1 * retries)
            else:
                error_msg_final = str(last_exception) if last_exception else "Max retries reached."
                err_type_final = "network_error" if isinstance(last_exception, aiohttp.ClientError) else "api_error"
                return {"error": error_msg_final, "error_type": err_type_final, "usage": {}}
        
        return {"error": "Max retries reached (logic error).", "error_type": "max_retries_exceeded", "usage": {}}


    async def execute_task_stream(
        self, request: AIRequestSchema
    ) -> AsyncGenerator[Union[str, Dict[str, Any]], None]:
        if not self.api_key:
            yield {"event": "error", "data": {"message": "API key not configured.", "type": "config_error"}}
            return

        effective_model_name = self._select_effective_model(request.task_type)
        messages = self._build_messages(request)
        payload: Dict[str, Any] = {
            "model": effective_model_name,
            "messages": messages,
            "temperature": request.temperature if request.temperature is not None else 0.7,
            "stream": True
        }
        if request.max_output_tokens:
            payload["max_tokens"] = request.max_output_tokens

        headers = {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json",
            "Accept": "text/event-stream"
        }
        if self.custom_headers: headers.update(self.custom_headers)
        
        session = await self._get_session()
        try:
            async with session.post(self.api_endpoint, json=payload, headers=headers) as response:
                if response.status != 200:
                    error_text = await response.text()
                    try: error_detail = json.loads(error_text).get("error", {})
                    except: error_detail = {"message": error_text[:200]}
                    msg = error_detail.get("message", "Unknown API streaming error")
                    err_type = error_detail.get("type", str(response.status))
                    yield {"event": "error", "data": {"message": f"DeepSeek API Error {err_type}: {msg}", "type": "api_error"}}
                    return

                async for line in response.content:
                    line_str = line.decode('utf-8').strip()
                    if line_str.startswith("data:"):
                        data_content = line_str[len("data:"):].strip()
                        if data_content == "[DONE]": break
                        try:
                            chunk_json = json.loads(data_content)
                            delta = chunk_json.get("choices", [{}])[0].get("delta", {})
                            content_chunk = delta.get("content")
                            if content_chunk: yield content_chunk
                            
                            finish_reason = chunk_json.get("choices", [{}])[0].get("finish_reason")
                            if finish_reason == "stop" or finish_reason == "length":
                                final_usage = chunk_json.get("usage")
                                if final_usage: yield {"event": "usage", "data": final_usage}
                                break 
                        except json.JSONDecodeError:
                            self.log_warning(f"Could not decode DeepSeek stream chunk: {data_content}")
        except aiohttp.ClientError as e:
            self.log_error(f"DeepSeek streaming ClientError: {str(e)}", exc_info=True)
            yield {"event": "error", "data": {"message": f"Streaming ClientError: {str(e)}", "type": "network_error"}}
        except Exception as e:
            self.log_error(f"DeepSeek unexpected streaming error: {str(e)}", exc_info=True)
            yield {"event": "error", "data": {"message": f"Unexpected streaming error: {str(e)}", "type": "unknown_stream_error"}}


    async def check_health(self) -> Tuple[bool, Optional[str]]:
        if not self.api_key:
            return False, "API key not configured."
        
        effective_model_name = self._select_effective_model(TaskType.SUMMARIZATION) # A generic task
        payload = {
            "model": effective_model_name,
            "messages": [{"role": "user", "content": "Hello"}],
            "max_tokens": 5
        }
        headers = {
            "Authorization": f"Bearer {self.api_key}",
            "Content-Type": "application/json"
        }
        session = await self._get_session()
        try:
            async with session.post(self.api_endpoint, json=payload, headers=headers) as response:
                response_text = await response.text()
                if response.status == 200:
                    json.loads(response_text) # check if valid json
                    return True, f"Successfully connected to DeepSeek ({effective_model_name})."
                else:
                    try: error_detail = json.loads(response_text).get("error", {})
                    except: error_detail = {"message": response_text[:200]}
                    msg = error_detail.get("message", "Unknown health check error")
                    return False, f"DeepSeek health check failed ({effective_model_name}): {response.status} - {msg}"
        except aiohttp.ClientError as e:
            self.log_error(f"DeepSeek health check connection error ({effective_model_name}): {str(e)}", exc_info=True)
            return False, f"Connection error ({effective_model_name}): {str(e)}"
        except Exception as e:
            self.log_error(f"DeepSeek health check unexpected error ({effective_model_name}): {str(e)}", exc_info=True)
            return False, f"Unexpected error ({effective_model_name}): {str(e)}"
