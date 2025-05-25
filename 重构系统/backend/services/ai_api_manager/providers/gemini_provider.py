"""
Gemini AI提供商实现
"""
import json
import asyncio
import aiohttp
from typing import Dict, Any, Optional, AsyncGenerator, Tuple, List, Union

from .base import BaseAIProvider
from ..schemas import AIRequestSchema
from ..models import TaskType, AIProvider as AIProviderEnum
from shared.utils.logger import LoggerMixin

# Google AI Studio / Vertex AI Gemini API endpoint structure
DEFAULT_GEMINI_API_URL_PREFIX = "https://generativelanguage.googleapis.com/v1beta/models" # model_name then :generateContent or :streamGenerateContent

class GeminiProvider(BaseAIProvider, LoggerMixin):
    """Gemini AI提供商"""

    def __init__(
        self,
        model_name: str, # e.g., "gemini-pro", "gemini-pro-vision"
        provider_name: AIProviderEnum, # Should be AIProviderEnum.GEMINI
        api_key: Optional[str],
        base_url: Optional[str] = None, # If set, overrides DEFAULT_GEMINI_API_URL_PREFIX logic
        timeout_seconds: int = 120,
        max_retries: int = 2,
        rpm_limit: Optional[int] = None,
        tpm_limit: Optional[int] = None,
        custom_headers: Optional[Dict[str, str]] = None,
        max_tokens_limit: Optional[int] = None, # e.g. Gemini 1.0 Pro: 30720 tokens, Vision: 12288
    ):
        super().__init__(
            model_name=model_name,
            provider_name=provider_name,
            api_key=api_key,
            base_url=base_url, # Store provided base_url, specific endpoint constructed later
            timeout_seconds=timeout_seconds,
            max_retries=max_retries,
            rpm_limit=rpm_limit,
            tpm_limit=tpm_limit,
            custom_headers=custom_headers,
            max_tokens_limit=max_tokens_limit
        )
        self._session: Optional[aiohttp.ClientSession] = None
        # Default behavior: if base_url is not explicitly set to a full path, use the prefix.
        self.api_url_prefix = base_url if base_url and ("generateContent" in base_url or "streamGenerateContent" in base_url) else (base_url or DEFAULT_GEMINI_API_URL_PREFIX)


    async def _get_session(self) -> aiohttp.ClientSession:
        if self._session is None or self._session.closed:
            timeout = aiohttp.ClientTimeout(total=self.timeout_seconds)
            # Gemini API doesn't typically use custom Authorization headers in the same way as OpenAI
            # API key is passed as a query parameter.
            # self.custom_headers might be used for other things if necessary.
            self._session = aiohttp.ClientSession(timeout=timeout, headers=self.custom_headers)
        return self._session

    async def close(self) -> None:
        if self._session and not self._session.closed:
            await self._session.close()
            self._session = None

    def _construct_api_url(self, stream: bool = False) -> str:
        """Constructs the full API URL for Gemini."""
        action = "streamGenerateContent" if stream else "generateContent"
        # If self.base_url was set to a full endpoint path by user (in _get_provider_config_dict it's called base_url, here api_url_prefix stores it), use it directly
        if "generateContent" in self.api_url_prefix or "streamGenerateContent" in self.api_url_prefix:
            return self.api_url_prefix 
        
        # Otherwise, append model and action to prefix
        return f"{self.api_url_prefix}/{self.model_name}:{action}"


    def _build_gemini_contents(self, request: AIRequestSchema) -> List[Dict[str, Any]]:
        """构建Gemini API的 'contents' 字段"""
        gemini_contents: List[Dict[str, Any]] = []
        task_type = request.task_type
        context = request.context or {}

        # Handle history first for multi-turn conversations
        if request.history:
            for entry in request.history:
                role = "user" if entry.get("role", "user") == "user" else "model" # Gemini uses 'model' for assistant
                gemini_contents.append({"role": role, "parts": [{"text": entry.get("content", "")}]})
        
        current_user_parts: List[Dict[str, Any]] = []
        
        prompt_text = ""
        if request.prompt:
            prompt_text = request.prompt
        
        # Task-specific prompt construction logic based on AIRequestSchema
        if task_type == TaskType.OCR and not prompt_text: # Default OCR prompt if none given
            prompt_text = context.get("ocr_prompt", "Extract all text from the provided image. If there are formulas, try to represent them in LaTeX.")
        elif task_type == TaskType.PROBLEM_ANALYSIS:
            problem_text_content = context.get("text", "") # From AIRequestSchema.context.text
            user_answer = context.get("user_answer")
            correct_answer = context.get("correct_answer")
            subject = context.get("subject", "通用")
            
            # System-like instructions are part of the user prompt for Gemini
            # Ensure prompt_text from request.prompt is prepended if it exists.
            analysis_instructions = f"""Analyze the following problem. The subject is {subject}.
Problem: {problem_text_content}
"""
            if user_answer: analysis_instructions += f"Student's Answer: {user_answer}\n"
            if correct_answer: analysis_instructions += f"Correct Answer: {correct_answer}\n"
            analysis_instructions += """
Output a single, valid JSON object with keys: "knowledge_points" (List[str]), "difficulty_level" (int 1-5), "error_analysis" (str), "solution" (str), "tags" (List[str]), "suggested_category" (str).
Ensure the entire response is ONLY this JSON object, without any markdown formatting or explanations.
"""
            prompt_text = (request.prompt + "\n" + analysis_instructions) if request.prompt else analysis_instructions

        elif task_type == TaskType.PROBLEM_IMAGE_TO_STRUCTURED_JSON:
            subject_hint = context.get("subject_hint")
            image_instructions = "Analyze the problem in the provided image and provide a structured JSON output. "
            if subject_hint: image_instructions += f"The user suggests the subject is: {subject_hint}. Prioritize this if relevant. "
            image_instructions += """
Output a single, valid JSON object with keys: "session_id" (placeholder), "raw_ocr_text" (str), "extracted_content" (str), "suggested_subject" (enum: "math", "physics", etc.), "preliminary_category" (str), "preliminary_tags" (List[str]), "image_regions_of_interest" (Optional[List[Dict]] or null), "detected_language" (Optional[str]), "original_image_ref" (placeholder).
Ensure the output is ONLY the JSON object. No explanations or markdown formatting.
"""
            prompt_text = (request.prompt + "\n" + image_instructions) if request.prompt else image_instructions


        elif task_type == TaskType.PROBLEM_INTERACTIVE_DEEP_ANALYSIS:
            # History is already added. Current user message is in request.prompt.
            # If it's the first turn after image upload, context might have initial_data_ref.
            initial_data = context.get("initial_data_ref") 
            if initial_data and not request.history and not gemini_contents: # First interactive turn after OCR
                # Prepend context from initial analysis to the user's first actual prompt
                initial_analysis_summary = f"Based on an initial image analysis, we have: {json.dumps(initial_data)}. "
                prompt_text = initial_analysis_summary + (request.prompt or "Please provide a deeper analysis or ask clarifying questions.")
            # else: request.prompt is the user's current message, already set to prompt_text

        # Add text part if constructed
        if prompt_text:
            current_user_parts.append({"text": prompt_text})
        elif not prompt_text and (request.image_base64 or request.image_url) and not current_user_parts:
            # If only image and no prompt, add a default text part for Vision
            current_user_parts.append({"text": "Describe this image."})


        # Image part - ensure model_name is vision capable if image data present
        # The service layer should select a vision model if image_base64 or image_url is present.
        # Here, we just construct the parts if data is available.
        if request.image_base64:
            image_type = "image/png" 
            if "jpeg" in request.image_base64[:30].lower() or "jpg" in request.image_base64[:30].lower():
                image_type = "image/jpeg"
            current_user_parts.append({
                "inline_data": {"mime_type": image_type, "data": request.image_base64}
            })
        elif request.image_url:
            # Gemini's client libraries handle URL fetching. For direct API, inline_data is preferred.
            # This provider will assume base64 is provided for now.
            self.log_warning("Gemini provider received image_url. This implementation prefers image_base64 for inline_data. Image might not be processed if not base64.")
            if not any(p.get("text") and str(request.image_url) in p.get("text") for p in current_user_parts):
                 current_user_parts.append({"text": f"[Image content was expected from URL: {str(request.image_url)} but could not be directly processed by this provider. Please ensure image is sent as base64.]"})


        if current_user_parts:
            # Gemini API structure: history (user, model, user, model...), then current user message.
            # If last in gemini_contents is 'model', or gemini_contents is empty, this is a new 'user' turn.
            if not gemini_contents or gemini_contents[-1].get("role") == "model":
                 gemini_contents.append({"role": "user", "parts": current_user_parts})
            else: # Last was 'user', this means the current_user_parts are a continuation of the last user message.
                  # This can happen if _build_gemini_contents adds text, then image for the same turn.
                gemini_contents[-1]["parts"].extend(current_user_parts)
        
        if not gemini_contents: # Fallback
            gemini_contents.append({"role": "user", "parts": [{"text": "Hello."}]})
            
        return gemini_contents

    def _parse_gemini_response(self, api_response: Dict[str, Any], request: AIRequestSchema) -> Dict[str, Any]:
        """Parses the non-streaming Gemini API response."""
        output_dict: Dict[str, Any] = {
            "content_text": None, "content_json": None, "usage": {}, 
            "error": None, "error_type": None
        }
        try:
            if "candidates" in api_response and api_response["candidates"]:
                candidate = api_response["candidates"][0]
                full_text_content = "".join(part.get("text", "") for part in candidate.get("content", {}).get("parts", []))
                output_dict["content_text"] = full_text_content

                if (request.output_format == "json_object" or \
                    request.task_type == TaskType.PROBLEM_ANALYSIS or \
                    request.task_type == TaskType.PROBLEM_IMAGE_TO_STRUCTURED_JSON) \
                    and full_text_content:
                    try:
                        output_dict["content_json"] = json.loads(full_text_content)
                    except json.JSONDecodeError:
                        self.log_warning(f"Gemini response for JSON task not valid JSON: {full_text_content[:200]}")
                
                usage_meta = api_response.get("usageMetadata", {})
                prompt_tokens = usage_meta.get("promptTokenCount", 0)
                completion_tokens = usage_meta.get("candidatesTokenCount", candidate.get("tokenCount", 0))
                output_dict["usage"] = {
                    "prompt_tokens": prompt_tokens,
                    "completion_tokens": completion_tokens,
                    "total_tokens": usage_meta.get("totalTokenCount", prompt_tokens + completion_tokens)
                }
                
                finish_reason = candidate.get("finishReason")
                if finish_reason and finish_reason != "STOP" and finish_reason != "FINISH_REASON_UNSPECIFIED":
                    output_dict["error"] = f"Generation stopped: {finish_reason}"
                    output_dict["error_type"] = {"MAX_TOKENS": "max_tokens_exceeded", "SAFETY": "safety_violation"}.get(finish_reason, "generation_stopped_error")

            elif "error" in api_response:
                err = api_response["error"]
                output_dict["error"] = err.get("message", "Unknown Gemini API error")
                output_dict["error_type"] = f"gemini_api_error_{err.get('status', 'UNKNOWN')}"
            
            return output_dict
        except Exception as e:
            self.log_error(f"Error parsing Gemini response: {e}. Response: {api_response}", exc_info=True)
            return {"error": f"Internal error parsing response: {str(e)}", "error_type": "response_parsing_error", "usage":{}}


    async def execute_task(self, request: AIRequestSchema) -> Dict[str, Any]:
        if not self.api_key:
            return {"error": "API key not configured.", "error_type": "config_error", "usage": {}}

        gemini_contents = self._build_gemini_contents(request)
        # Determine effective model name (e.g., switch to vision if image data present)
        effective_model_name = self.model_name
        if (request.image_base64 or request.image_url) and "vision" not in self.model_name:
            # Simple logic, might need refinement based on available models
            if self.model_name == "gemini-pro": effective_model_name = "gemini-pro-vision"
            elif self.model_name == "gemini-1.0-pro": effective_model_name = "gemini-1.0-pro-vision" # Example
            elif self.model_name == "gemini-1.5-pro-latest": effective_model_name = "gemini-1.5-pro-latest" # 1.5 pro is multimodal
            elif self.model_name == "gemini-1.5-flash-latest": effective_model_name = "gemini-1.5-flash-latest" # 1.5 flash is multimodal
            else: self.log_warning(f"Image data provided for non-vision model {self.model_name}. Trying with current model.")
        
        payload = {
            "contents": gemini_contents,
            "generationConfig": {
                "temperature": request.temperature if request.temperature is not None else 0.7,
                "maxOutputTokens": request.max_output_tokens or self.max_tokens_limit or 8192, # Gemini 1.5 has larger limits
            },
        }
        # Note: Gemini generationConfig can also take "candidateCount", "stopSequences", "topP", "topK".
        # Add if exposed in AIRequestSchema.

        api_url = self._construct_api_url(stream=False).replace(self.model_name, effective_model_name) # Ensure correct model in URL
        params = {"key": self.api_key}
        headers = {"Content-Type": "application/json"}
        if self.custom_headers: headers.update(self.custom_headers)

        session = await self._get_session()
        retries = 0
        last_exception = None

        while retries <= self.max_retries:
            try:
                async with session.post(api_url, json=payload, params=params, headers=headers) as response:
                    response_text = await response.text()
                    try:
                        response_json = json.loads(response_text)
                    except json.JSONDecodeError:
                        last_exception = Exception(f"Gemini API returned non-JSON response ({response.status}): {response_text[:500]}")
                        # Fall through to retry logic / error reporting
                        response_json = {"error":{"message": str(last_exception), "status": "JSON_DECODE_ERROR"}}


                    if response.status == 200:
                        return self._parse_gemini_response(response_json, request)
                    else:
                        error_detail = response_json.get("error", {})
                        msg = error_detail.get("message", f"Unknown API error: {response_text[:200]}")
                        status = error_detail.get("status", str(response.status))
                        last_exception = Exception(f"Gemini API Error {status}: {msg}")
            except aiohttp.ClientError as e:
                last_exception = e
            
            retries += 1
            self.log_warning(f"Gemini attempt {retries}/{self.max_retries + 1} failed: {last_exception}")
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

        gemini_contents = self._build_gemini_contents(request)
        effective_model_name = self.model_name
        if (request.image_base64 or request.image_url) and "vision" not in self.model_name: # As above
            if self.model_name == "gemini-pro": effective_model_name = "gemini-pro-vision"
            elif self.model_name == "gemini-1.0-pro": effective_model_name = "gemini-1.0-pro-vision"
            elif self.model_name == "gemini-1.5-pro-latest": effective_model_name = "gemini-1.5-pro-latest"
            elif self.model_name == "gemini-1.5-flash-latest": effective_model_name = "gemini-1.5-flash-latest"

        payload = {
            "contents": gemini_contents,
            "generationConfig": {
                "temperature": request.temperature if request.temperature is not None else 0.7,
                "maxOutputTokens": request.max_output_tokens or self.max_tokens_limit or 8192,
            },
        }
        api_url = self._construct_api_url(stream=True).replace(self.model_name, effective_model_name)
        params = {"key": self.api_key, "alt": "sse"}
        headers = {"Content-Type": "application/json", "Accept": "text/event-stream"}
        if self.custom_headers: headers.update(self.custom_headers)

        session = await self._get_session()
        try:
            async with session.post(api_url, json=payload, params=params, headers=headers) as response:
                if response.status != 200:
                    error_text = await response.text()
                    try: error_detail = json.loads(error_text).get("error", {})
                    except: error_detail = {"message": error_text[:200]}
                    msg = error_detail.get("message", "Unknown API streaming error")
                    status = error_detail.get("status", str(response.status))
                    yield {"event": "error", "data": {"message": f"Gemini API Error {status}: {msg}", "type": "api_error"}}
                    return

                buffer = ""
                async for line_bytes in response.content:
                    line = line_bytes.decode('utf-8').strip()
                    if not line: continue
                    if line.startswith("data:"): buffer += line[len("data:"):]
                    elif not line.startswith(":") and buffer: # Event boundary (often empty line, but check if buffer has data)
                        try:
                            # Gemini SSE often sends multiple JSON objects not as an array, but concatenated.
                            # Split by '}{' and wrap with '[' ']' is a common workaround.
                            json_strings = [s for s in buffer.split('}{') if s]
                            for i, js_str in enumerate(json_strings):
                                if i > 0: js_str = "{" + js_str
                                if i < len(json_strings) - 1 : js_str = js_str + "}"
                                
                                chunk_json = json.loads(js_str)
                                if "candidates" in chunk_json:
                                    for candidate in chunk_json["candidates"]:
                                        text_content = "".join(p.get("text", "") for p in candidate.get("content", {}).get("parts", []))
                                        if text_content: yield text_content
                                if chunk_json.get("usageMetadata"):
                                    yield {"event": "usage", "data": chunk_json["usageMetadata"]}
                        except json.JSONDecodeError:
                            self.log_warning(f"Could not decode Gemini stream data: {buffer}")
                        buffer = ""
                if buffer: # Process any trailing buffer content
                    try: # same logic as above for trailing buffer
                        json_strings = [s for s in buffer.split('}{') if s]
                        for i, js_str in enumerate(json_strings):
                            if i > 0: js_str = "{" + js_str
                            if i < len(json_strings) - 1 : js_str = js_str + "}"
                            chunk_json = json.loads(js_str)
                            if "candidates" in chunk_json:
                                for candidate in chunk_json["candidates"]:
                                    text_content = "".join(p.get("text", "") for p in candidate.get("content", {}).get("parts", []))
                                    if text_content: yield text_content
                            if chunk_json.get("usageMetadata"):
                                yield {"event": "usage", "data": chunk_json["usageMetadata"]}
                    except json.JSONDecodeError:
                         self.log_warning(f"Could not decode final Gemini stream data: {buffer}")

        except aiohttp.ClientError as e:
            self.log_error(f"Gemini streaming ClientError: {str(e)}", exc_info=True)
            yield {"event": "error", "data": {"message": f"Streaming ClientError: {str(e)}", "type": "network_error"}}
        except Exception as e:
            self.log_error(f"Gemini unexpected streaming error: {str(e)}", exc_info=True)
            yield {"event": "error", "data": {"message": f"Unexpected streaming error: {str(e)}", "type": "unknown_stream_error"}}


    async def check_health(self) -> Tuple[bool, Optional[str]]:
        if not self.api_key:
            return False, "API key not configured."
        
        payload = {"contents": [{"parts": [{"text": "Hello"}]}], "generationConfig": {"maxOutputTokens": 5}}
        # Use a model that is generally available, like gemini-1.0-pro or gemini-1.5-flash if testing newer ones
        health_model = "gemini-1.0-pro" if "1.5" not in self.model_name else self.model_name # simple default
        api_url = f"{self.api_url_prefix}/{health_model}:generateContent"
        if "generateContent" in self.api_url_prefix or "streamGenerateContent" in self.api_url_prefix : # if full path was given
             api_url = self.api_url_prefix.replace(":streamGenerateContent",":generateContent")


        params = {"key": self.api_key}
        headers = {"Content-Type": "application/json"}

        session = await self._get_session()
        try:
            async with session.post(api_url, json=payload, params=params, headers=headers) as response:
                response_text = await response.text()
                if response.status == 200:
                    # try to parse to confirm valid response
                    json.loads(response_text)
                    return True, f"Successfully connected to Gemini ({health_model})."
                else:
                    try: error_detail = json.loads(response_text).get("error", {})
                    except: error_detail = {"message": response_text[:200]}
                    msg = error_detail.get("message", "Unknown health check error")
                    return False, f"Gemini health check failed ({health_model}): {response.status} - {msg}"
        except aiohttp.ClientError as e:
            self.log_error(f"Gemini health check connection error ({health_model}): {str(e)}", exc_info=True)
            return False, f"Connection error ({health_model}): {str(e)}"
        except Exception as e:
            self.log_error(f"Gemini health check unexpected error ({health_model}): {str(e)}", exc_info=True)
            return False, f"Unexpected error ({health_model}): {str(e)}"
