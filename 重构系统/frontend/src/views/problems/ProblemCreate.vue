<template>
  <div class="problem-create">
    <div class="page-header">
      <h1>创建错题</h1>
    </div>

    <el-card>
      <el-form
        ref="formRef"
        :model="form"
        :rules="rules"
        label-width="100px"
      >
        <el-form-item label="题目标题" prop="title">
          <el-input
            v-model="form.title"
            placeholder="请输入题目标题"
            maxlength="200"
            show-word-limit
          />
        </el-form-item>

        <el-form-item label="学科" prop="subject">
          <el-select v-model="form.subject" placeholder="请选择学科">
            <el-option label="数学" value="math" />
            <el-option label="英语" value="english" />
            <el-option label="政治" value="politics" />
            <el-option label="专业课" value="professional" />
          </el-select>
        </el-form-item>

        <el-form-item label="分类" prop="category">
          <el-input
            v-model="form.category"
            placeholder="如：微积分、语法等"
          />
        </el-form-item>

        <el-form-item label="题目内容" prop="content">
          <el-input
            v-model="form.content"
            type="textarea"
            :rows="5"
            placeholder="请输入题目内容"
          />
          <!-- 图片上传 -->
          <el-upload
            class="problem-image-upload"
            :auto-upload="false"
            :on-change="handleImageFileChange"
            :on-remove="handleImageFileRemove"
            :file-list="uploadedImageFile ? [uploadedImageFile] : []"
            list-type="picture-card"
            accept="image/*"
            :limit="1"
            :on-exceed="handleUploadExceed"
          >
            <el-icon><Plus /></el-icon>
            <template #tip>
              <div class="el-upload__tip">
                请上传错题图片 (仅限1张)
              </div>
            </template>
          </el-upload>
          <!-- AI图片识别导入按钮 -->
          <div class="ai-import-section" v-if="uploadedImageFile && !isAIImported">
            <el-button 
              type="primary" 
              :icon="UploadFilled"
              @click="handleStartAIImageImport" 
              :loading="aiProcessing"
              class="ai-import-btn"
            >
              {{ aiProcessing ? 'AI识别处理中...' : '开始AI图片识别导入' }}
            </el-button>
            <span v-if="!aiProcessing" class="ai-tip">AI将尝试从图片中识别题目信息并填充表单</span>
          </div>
          <div v-if="isAIImported && sessionId" class="ai-import-status">
            <el-alert type="success" show-icon :closable="false">
              AI已初步处理图片。会话ID: {{ sessionId }}. 请检查下方表单内容，可进行修改。
              后续可进行交互式分析。
            </el-alert>
          </div>
        </el-form-item>
        
        <!-- AI交互式分析聊天界面 -->
        <el-form-item label="AI交互分析" v-if="isAIImported && sessionId">
          <div class="ai-chat-container">
            <div class="chat-history" ref="chatHistoryContainerRef">
              <div v-for="(message, index) in chatHistory" :key="index" :class="['chat-message', message.role]">
                <div class="message-bubble">
                  <strong v-if="message.role === 'user'">你:</strong>
                  <strong v-if="message.role === 'ai'">AI助手:</strong>
                  <div v-if="message.type === 'error_message'" class="error-text">
                    错误: {{ message.content }}
                  </div>
                  <div v-else class="message-content-text">
                    <span v-html="formatMessageContent(message.content)"></span>
                    <el-icon v-if="message.isLoading" class="is-loading"><Loading /></el-icon>
                  </div>
                  <div class="message-meta" v-if="message.timestamp">
                    {{ new Date(message.timestamp).toLocaleTimeString() }}
                  </div>
                </div>
              </div>
            </div>
            <div class="chat-input-area">
              <el-input
                v-model="currentUserMessage"
                type="textarea"
                :rows="2"
                placeholder="和AI对话，例如：请帮我分析这道题的知识点，或者我的答案为什么错了？"
                @keyup.enter.prevent="handleSendMessageToAI"
                :disabled="isAIChatProcessing"
              />
              <el-button 
                type="primary" 
                @click="handleSendMessageToAI()" 
                :loading="isAIChatProcessing"
                :disabled="!currentUserMessage.trim() && !isAIChatProcessing" 
                class="send-chat-btn"
              >
                发送
              </el-button>
            </div>
            <div v-if="!chatHistory.length && !isAIChatProcessing && !autoStartAIChatDone" class="chat-starter">
              <el-button @click="initiateAIChatWithPrompt('请详细分析这道题目，包括题目的主要内容、涉及的知识点、可能的解题步骤和常见的易错点。')">
                让AI开始分析题目
              </el-button>
            </div>
          </div>
        </el-form-item>

        <el-form-item label="我的答案">
          <el-input
            v-model="form.user_answer"
            type="textarea"
            :rows="3"
            placeholder="记录你的错误答案"
          />
        </el-form-item>

        <el-form-item label="正确答案">
          <el-input
            v-model="form.correct_answer"
            type="textarea"
            :rows="3"
            placeholder="输入正确答案"
          />
        </el-form-item>

        <!-- 知识点字段 -->
        <el-form-item label="知识点">
          <el-select
            v-model="form.knowledge_points"
            multiple
            filterable
            allow-create
            default-first-option
            placeholder="选择或创建知识点"
          >
            <el-option
              v-for="point in commonKnowledgePoints"
              :key="point"
              :label="point"
              :value="point"
            />
          </el-select>
          <div class="form-tip">AI分析后会自动提取知识点</div>
        </el-form-item>

        <el-form-item label="错误分析">
          <el-input
            v-model="form.error_analysis"
            type="textarea"
            :rows="4"
            placeholder="分析错误原因"
          />
          <div class="form-tip">AI可以帮助生成详细的错误分析</div>
        </el-form-item>

        <!-- AI建议的相似题目 -->
        <el-form-item label="相似题目" v-if="similarProblems.length > 0">
          <div class="similar-problems">
            <div 
              v-for="(problem, index) in similarProblems" 
              :key="index"
              class="similar-problem-item"
            >
              <el-card class="similar-card">
                <div class="similar-content">
                  <h4>{{ problem.title }}</h4>
                  <p class="similar-preview">{{ problem.content.substring(0, 100) }}...</p>
                  <div class="similar-meta">
                    <el-tag size="small">{{ problem.subject }}</el-tag>
                    <el-tag size="small" type="info">{{ problem.category }}</el-tag>
                  </div>
                </div>
                <div class="similar-actions">
                  <el-button size="small" @click="viewSimilarProblem(problem)">查看</el-button>
                </div>
              </el-card>
            </div>
          </div>
        </el-form-item>

        <el-form-item label="标签">
          <el-select
            v-model="form.tags"
            multiple
            filterable
            allow-create
            default-first-option
            placeholder="选择或创建标签"
          >
            <el-option
              v-for="tag in commonTags"
              :key="tag"
              :label="tag"
              :value="tag"
            />
          </el-select>
        </el-form-item>

        <el-form-item>
          <el-button type="primary" @click="handleSubmit" :loading="loading">
            创建错题
          </el-button>
          <el-button @click="$router.back()">取消</el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <!-- AI分析结果对话框 -->
    <el-dialog
      v-model="aiResultVisible"
      title="AI分析结果"
      width="600px"
      :close-on-click-modal="false"
    >
      <div class="ai-result-content">
        <el-tabs v-model="activeTab">
          <el-tab-pane label="知识点分析" name="knowledge">
            <div class="analysis-section">
              <h4>提取的知识点：</h4>
              <div class="knowledge-points">
                <el-tag 
                  v-for="point in aiAnalysisResult.knowledge_points" 
                  :key="point"
                  class="knowledge-tag"
                >
                  {{ point }}
                </el-tag>
              </div>
            </div>
          </el-tab-pane>
          
          <el-tab-pane label="错误分析" name="analysis">
            <div class="analysis-section">
              <h4>AI生成的错误分析：</h4>
              <div class="error-analysis-content">
                {{ aiAnalysisResult.error_analysis }}
              </div>
            </div>
          </el-tab-pane>
          
          <el-tab-pane label="学习建议" name="suggestions">
            <div class="analysis-section">
              <h4>学习建议：</h4>
              <ul class="suggestions-list" v-if="aiAnalysisResult.study_suggestions && aiAnalysisResult.study_suggestions.length > 0">
                <li v-for="(suggestion, index) in aiAnalysisResult.study_suggestions" :key="index">
                  {{ suggestion }}
                </li>
              </ul>
              <p v-else>暂无学习建议。</p>
            </div>
          </el-tab-pane>

          <el-tab-pane label="其他信息" name="other_info">
            <div class="analysis-section">
              <h4>原始识别文本 (OCR):</h4>
              <div class="error-analysis-content" style="max-height: 150px; overflow-y: auto; border: 1px solid #eee; padding: 5px; background-color: #f9f9f9;">
                {{ aiSessionData.raw_ocr_text || 'AI未提供原始识别文本。' }}
              </div>
            </div>
            <div class="analysis-section">
              <h4>AI建议标题：</h4>
              <p>{{ aiSessionData.title || 'AI未建议标题。' }}</p>
            </div>
            <div class="analysis-section">
              <h4>建议分类：</h4>
              <p>{{ aiAnalysisResult.suggested_category || aiSessionData.preliminary_category || '未提供' }}</p>
            </div>
            <div class="analysis-section">
              <h4>建议标签：</h4>
              <div class="knowledge-points" v-if="aiAnalysisResult.tags && aiAnalysisResult.tags.length > 0">
                <el-tag 
                  v-for="tag in aiAnalysisResult.tags" 
                  :key="tag"
                  type="success"
                  class="knowledge-tag"
                >
                  {{ tag }}
                </el-tag>
              </div>
              <p v-else>暂无建议标签。</p>
            </div>
            <div class="analysis-section">
              <h4>解题思路：</h4>
              <div class="error-analysis-content">
                {{ aiAnalysisResult.solution || '此部分将在AI交互式分析后填充。' }}
              </div>
            </div>
            <div class="analysis-section">
              <h4>难度评估：</h4>
              <p>{{ aiAnalysisResult.difficulty_level ? `级别 ${aiAnalysisResult.difficulty_level}` : (aiSessionData.difficulty ? `级别 ${aiSessionData.difficulty}` : '未评估') }}</p>
            </div>
             <div class="analysis-section">
              <h4>识别语言：</h4>
              <p>{{ aiSessionData.detected_language || '未检测' }}</p>
            </div>
          </el-tab-pane>
        </el-tabs>
      </div>
      
      <template #footer>
        <div class="dialog-footer">
          <el-button @click="aiResultVisible = false">取消</el-button>
          <el-button type="primary" @click="applyAIResult">应用分析结果</el-button>
        </div>
      </template>
    </el-dialog>

    <!-- 分析方式选择对话框 (REMOVED) -->
    <!--
    <el-dialog
      v-model="analysisMethodVisible"
      title="选择分析方式"
      width="400px"
      :close-on-click-modal="false"
      :show-close="false"
    >
      <el-form label-width="100px">
        <el-form-item label="分析方式">
          <el-radio-group v-model="selectedAnalysisMethod">
            <el-radio label="multimodal_large">多模态大模型分析</el-radio>
            <el-radio label="multimodal_small_text">多模态小模型 + 纯文字模型</el-radio>
          </el-radio-group>
        </el-form-item>
      </el-form>
      <template #footer>
        <div class="dialog-footer">
          <el-button @click="analysisMethodVisible = false">取消</el-button>
          <el-button type="primary" @click="performAIAnalysis(selectedAnalysisMethod)">确定</el-button>
        </div>
      </template>
    </el-dialog>
    -->
  </div>
</template>

<script setup>
// Original imports were duplicated, cleaning up
import { ref, reactive, computed, nextTick, watch, onUnmounted, onMounted } from 'vue' // Added nextTick, watch, onUnmounted, onMounted
import { useRouter, useRoute } from 'vue-router' // Added useRoute
import { ElMessage, ElMessageBox } from 'element-plus'
import { MagicStick, Plus, ChatDotRound, UploadFilled, Loading } from '@element-plus/icons-vue' // Added Loading
import { problemAPI } from '@/utils/api'

const router = useRouter()
const route = useRoute() // Added route instance
const chatHistoryContainerRef = ref(null); // For scrolling chat
const formRef = ref()

// Utility to convert file to base64
const fileToBase64 = (file) => {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.readAsDataURL(file);
    reader.onload = () => resolve(reader.result); // reader.result includes "data:image/png;base64," prefix
    reader.onerror = (error) => reject(error);
  });
};
const loading = ref(false) // For final submission
const aiResultVisible = ref(false) // Re-purpose for showing initial AI structured data or chat
const activeTab = ref('other_info') // Default tab for AI result dialog (changed)

// --- New state for AI-driven creation ---
const aiProcessing = ref(false) // Generic AI processing flag (e.g., for image import, chat)
const sessionId = ref(null)
const aiSessionData = reactive({ // Holds ProblemAIStructuredData from backend
  // Fields from ProblemAIStructuredData
  problem_id: null, // if an existing problem is being edited via AI
  session_id: null, // Will be same as sessionId.value
  raw_ocr_text: '',
  extracted_content: '', // This will primarily populate form.content
  suggested_subject: '',
  preliminary_category: '',
  preliminary_tags: [],
  image_regions_of_interest: [],
  detected_language: '',
  // Fields from ProblemSchema base (that AI might suggest or fill)
  title: '', 
  // No user_answer, correct_answer, error_analysis directly from initial image scan,
  // these would come from interactive analysis or manual input
  knowledge_points: [],
  difficulty: null, // Or whatever your difficulty field is named
  // Potentially add solution steps if AI can suggest them early
  // solution_steps: [], 
})
const isAIImported = ref(false) // True after initial image processing (Stage 1 done)
const uploadedImageFile = ref(null) // Holds the UploadFile object from Element Plus

// --- Chat state ---
const chatHistory = ref([]);
const currentUserMessage = ref('');
const isAIChatProcessing = ref(false); // Specifically for AI chat responses
const autoStartAIChatDone = ref(false); // To hide the "Let AI start analysis" button after first auto-start
let eventSource = null; 
// --- End Chat state ---

// --- End of new state ---

const form = reactive({
  title: '',
  subject: '',
  category: '',
  content: '',
  user_answer: '',
  correct_answer: '',
  error_analysis: '',
  knowledge_points: [],
  tags: []
})

const rules = {
  title: [
    { required: true, message: '请输入题目标题', trigger: 'blur' }
  ],
  subject: [
    { required: true, message: '请选择学科', trigger: 'change' }
  ],
  content: [
    { required: true, message: '请输入题目内容', trigger: 'blur' }
  ]
}

const commonTags = [
  '重点',
  '难点',
  '易错',
  '常考',
  '基础',
  '提高',
  '概念题',
  '计算题',
  '应用题'
]

const commonKnowledgePoints = [
  '函数',
  '导数',
  '积分',
  '极限',
  '微分方程',
  '线性代数',
  '概率论',
  '统计学',
  '几何',
  '代数'
]

const similarProblems = ref([])
const aiAnalysisResult = reactive({
  knowledge_points: [], // This is part of aiSessionData as well
  error_analysis: '',   // This will likely come from interactive analysis later
  study_suggestions: [],// This may come from interactive analysis
  solution: '',         // This may come from interactive analysis
  difficulty_level: null, // This will be aiSessionData.difficulty
  tags: [],             // This will be aiSessionData.preliminary_tags initially
  suggested_category: ''// This will be aiSessionData.preliminary_category
})

// Old fileList ref, replaced by uploadedImageFile logic
// const fileList = ref([]); 

// --- New methods for AI-driven creation ---
const handleImageFileChange = (uploadFile) => {
  // ElUpload's on-change gives the current file and the file list.
  // Since limit is 1, uploadFile is the one we care about.
  uploadedImageFile.value = uploadFile; // Stores the UploadFile object
  // If an image is changed after import, reset the AI import state
  if (isAIImported.value) {
    isAIImported.value = false;
    sessionId.value = null;
    // Optionally clear AI-derived fields in form or aiSessionData if a new image means a fresh start
    // For now, just resetting flags. User must click import again.
  }
  console.log('Image file selected:', uploadedImageFile.value);
};

const handleImageFileRemove = () => {
  uploadedImageFile.value = null;
  isAIImported.value = false;
  sessionId.value = null;
  // Clear relevant form fields that were AI-populated, or reset aiSessionData
  Object.keys(aiSessionData).forEach(key => {
    if (Array.isArray(aiSessionData[key])) {
      aiSessionData[key] = [];
    } else if (typeof aiSessionData[key] === 'object' && aiSessionData[key] !== null) {
      aiSessionData[key] = {};
    } else {
      aiSessionData[key] = null;
    }
  });
  // Also reset form fields that were derived from aiSessionData
  // form.title = ''; // Or decide if manual entries persist
  // form.content = '';
  // form.subject = ''; 
  // form.category = '';
  // form.tags = [];
  // form.knowledge_points = [];
  console.log('Image file removed.');
};

const handleUploadExceed = () => {
  ElMessage.warning('只能上传一张图片。请先移除现有图片再尝试上传。');
};

const handleStartAIImageImport = async () => {
  if (!uploadedImageFile.value || !uploadedImageFile.value.raw) {
    ElMessage.error('请先选择一个图片文件。');
    return;
  }

  aiProcessing.value = true;
  try {
    const base64ImageString = await fileToBase64(uploadedImageFile.value.raw);
    const payload = {
      image_base64: base64ImageString, // Send the full data URL string
      subject_hint: form.subject || null,
    };

    const response = await problemAPI.initiateAICreation(payload); // Use the updated API method
    
    if (response.data && response.data.success && response.data.data) { // response.data.data is ProblemAIStructuredData
      const structuredData = response.data.data;
      sessionId.value = structuredData.session_id; // session_id is part of ProblemAIStructuredData
      
      Object.assign(aiSessionData, structuredData); // aiSessionData now holds the ProblemAIStructuredData

      // Populate form with AI extracted data
      form.title = structuredData.title || aiSessionData.title || ''; // title might be in structuredData directly
      form.content = structuredData.extracted_content || '';
      form.subject = structuredData.suggested_subject || form.subject || '';
      form.category = structuredData.preliminary_category || '';
      form.tags = Array.isArray(structuredData.preliminary_tags) ? [...structuredData.preliminary_tags] : [];
      form.knowledge_points = Array.isArray(structuredData.knowledge_points) ? [...structuredData.knowledge_points] : [];
      // aiSessionData.difficulty is the correct field from ProblemAIStructuredData schema if AI suggests it.
      // form.difficulty = structuredData.difficulty; 

      // Populate aiAnalysisResult for the dialog display using aiSessionData
      aiAnalysisResult.knowledge_points = aiSessionData.knowledge_points || [];
      aiAnalysisResult.tags = aiSessionData.preliminary_tags || [];
      aiAnalysisResult.suggested_category = aiSessionData.preliminary_category || '';
      aiAnalysisResult.solution = ''; // Solution comes from interactive analysis
      aiAnalysisResult.difficulty_level = aiSessionData.difficulty; // Use 'difficulty' from aiSessionData
      aiAnalysisResult.error_analysis = ''; 
      aiAnalysisResult.study_suggestions = [];

      isAIImported.value = true;
      ElMessage.success('AI图片识别初步完成！请检查并完善表单信息。点击查看AI分析结果可看到原始识别文本等。');
      aiResultVisible.value = true; 
      activeTab.value = 'other_info'; 
    } else {
      ElMessage.error('AI图片识别失败：' + (response.data.message || '无法获取有效的会话或数据。'));
    }
  } catch (error) {
    console.error("AI Image Import Error:", error);
    ElMessage.error('AI图片识别请求失败: ' + (error.response?.data?.detail || error.message || '未知错误'));
  } finally {
    aiProcessing.value = false;
  }
};

// --- End of new methods ---

// --- Chat Methods ---
const formatMessageContent = (content) => {
  if (typeof content !== 'string') return '';
  // Basic: escape HTML and replace newlines with <br>
  // More advanced: use a library like marked.js for markdown
  return content.replace(/&/g, '&')
                .replace(/</g, '<')
                .replace(/>/g, '>')
                .replace(/"/g, '"')
                .replace(/'/g, '&#039;')
                .replace(/\n/g, '<br />');
};

const scrollToBottom = () => {
  nextTick(() => {
    const container = chatHistoryContainerRef.value;
    if (container) {
      container.scrollTop = container.scrollHeight;
    }
  });
};

watch(chatHistory, scrollToBottom, { deep: true });


const initiateAIChatWithPrompt = (prompt) => {
  if (!prompt || !sessionId.value) {
    ElMessage.info('AI会话尚未初始化或无提示信息。');
    return;
  }
  currentUserMessage.value = prompt;
  handleSendMessageToAI(true); 
  autoStartAIChatDone.value = true; // Mark that auto-start was attempted/done
};


const handleSendMessageToAI = async (isInitialPrompt = false) => {
  if (!currentUserMessage.value.trim() && !isInitialPrompt) return;
  if (isAIChatProcessing.value) return;
  if (!sessionId.value) {
    ElMessage.error('AI会话ID丢失，无法发送消息。请尝试重新上传图片。');
    return;
  }

  const userMessageContent = currentUserMessage.value.trim();
  
  chatHistory.value.push({
    role: 'user',
    content: userMessageContent,
    timestamp: new Date().toISOString(),
  });
  
  if (!isInitialPrompt) {
    currentUserMessage.value = ''; 
  }

  isAIChatProcessing.value = true;
  let currentAIResponseContent = '';
  const aiMessagePlaceholder = {
    role: 'ai',
    content: '',
    isLoading: true,
    timestamp: new Date().toISOString(),
  };
  chatHistory.value.push(aiMessagePlaceholder);
  const aiMessageIndex = chatHistory.value.length - 1;


  try {
    const streamRequestData = {
      user_message: userMessageContent,
      // Send a limited number of previous messages to keep context manageable
      // Adjust N as needed based on token limits and desired context length
      chat_history: chatHistory.value.slice(Math.max(0, chatHistory.value.length - 11), -1).map(msg => ({ 
        role: msg.role,
        content: msg.content,
      })),
      // Send initial OCR data as context, especially for the first interaction
      initial_data_ref: (chatHistory.value.length <= 2 || isInitialPrompt) ? JSON.stringify(aiSessionData) : undefined
    };

    const sseParams = {
      user_message: streamRequestData.user_message,
    };
    if (streamRequestData.chat_history && streamRequestData.chat_history.length > 0) {
      sseParams.chat_history_json = JSON.stringify(streamRequestData.chat_history);
    }
    if (streamRequestData.initial_data_ref) {
      sseParams.initial_data_ref_json = streamRequestData.initial_data_ref;
    }

    // Use the helper from api.js. Note: getAIInteractiveStreamURL returns the full path including /api
    // So if EventSource prepends /api, we need to adjust. Assuming EventSource takes full path.
    const sseUrl = problemAPI.getAIInteractiveStreamURL(sessionId.value, sseParams);
    
    if (eventSource) {
      eventSource.close();
    }
    eventSource = new EventSource(sseUrl); // Assumes backend is GET with params

    eventSource.onmessage = (event) => {
      try {
        const parsedData = JSON.parse(event.data);
        const currentAIMsg = chatHistory.value[aiMessageIndex];

        if (parsedData.type === 'content_chunk') {
          currentAIResponseContent += parsedData.value;
          currentAIMsg.content = currentAIResponseContent;
        } else if (parsedData.type === 'json_suggestion') {
          console.log("AI Suggestion:", parsedData.value);
          currentAIResponseContent += `\n[AI Suggestion: ${JSON.stringify(parsedData.value)}]`;
          currentAIMsg.content = currentAIResponseContent;
           // TODO: Implement logic to apply suggestions to form, e.g.,
           // if (parsedData.value.field && form.hasOwnProperty(parsedData.value.field)) {
           //   form[parsedData.value.field] = parsedData.value.value;
           //   ElMessage.info(`AI建议已更新字段: ${parsedData.value.field}`);
           // }
        } else if (parsedData.type === 'stream_end') {
          console.log('Stream ended by AI.');
          currentAIMsg.isLoading = false;
          isAIChatProcessing.value = false;
          if(eventSource) eventSource.close();
        } else if (parsedData.type === 'error') {
          console.error('AI Stream Error:', parsedData.message);
          currentAIMsg.content = `AI处理错误: ${parsedData.message || '未知流错误'}`;
          currentAIMsg.type = 'error_message';
          currentAIMsg.isLoading = false;
          isAIChatProcessing.value = false;
          if(eventSource) eventSource.close();
        }
      } catch (e) {
        console.error('Error parsing SSE event data:', e, event.data);
        // Append raw data if parsing fails, assuming it's a text chunk
        currentAIResponseContent += event.data + '\n';
        chatHistory.value[aiMessageIndex].content = currentAIResponseContent;
      }
    };

    eventSource.onerror = (error) => {
      console.error('EventSource failed:', error);
      const currentAIMsg = chatHistory.value[aiMessageIndex];
      if (currentAIMsg) {
        currentAIMsg.content = '与AI的连接发生错误。请检查网络或稍后再试。';
        currentAIMsg.type = 'error_message';
        currentAIMsg.isLoading = false;
      } else { // Should not happen if placeholder was added
        chatHistory.value.push({
          role: 'ai',
          content: '与AI的连接发生错误。请检查网络或稍后再试。',
          type: 'error_message',
          isLoading: false,
          timestamp: new Date().toISOString(),
        });
      }
      isAIChatProcessing.value = false;
      if(eventSource) eventSource.close();
    };

  } catch (error) {
    console.error('Error setting up SSE connection:', error);
    const currentAIMsg = chatHistory.value[aiMessageIndex];
     if (currentAIMsg) {
        currentAIMsg.content = '无法建立与AI的流式连接。';
        currentAIMsg.type = 'error_message';
        currentAIMsg.isLoading = false;
     }
    isAIChatProcessing.value = false;
  }
};

onUnmounted(() => {
  if (eventSource) {
    eventSource.close();
    eventSource = null;
  }
}); // Changed from }; to }); and removed the comment below

const resetComponentState = () => {
  // Reset form
  form.title = '';
  form.subject = '';
  form.category = '';
  form.content = '';
  form.user_answer = '';
  form.correct_answer = '';
  form.error_analysis = '';
  form.knowledge_points = [];
  form.tags = [];
  // Reset AI session data
  Object.keys(aiSessionData).forEach(key => {
    if (Array.isArray(aiSessionData[key])) aiSessionData[key] = [];
    else if (typeof aiSessionData[key] === 'object' && aiSessionData[key] !== null) aiSessionData[key] = {};
    else aiSessionData[key] = null;
  });
  sessionId.value = null;
  isAIImported.value = false;
  uploadedImageFile.value = null;
  if (formRef.value) formRef.value.resetFields(); // Reset form validation

  // Reset chat
  chatHistory.value = [];
  currentUserMessage.value = '';
  isAIChatProcessing.value = false;
  autoStartAIChatDone.value = false;
  if (eventSource) {
    eventSource.close();
    eventSource = null;
  }
  loading.value = false; // General loading for submit
  aiProcessing.value = false; // AI import processing
};


const loadSessionForResuming = async (resumeSessionId) => {
  resetComponentState(); // Clear any existing state first
  loading.value = true; // Use general loading indicator for session load
  aiProcessing.value = true; // Also indicate AI processing

  try {
    const detailResponse = await problemAPI.getAISessionDetail(resumeSessionId);
    if (!(detailResponse.data && detailResponse.data.success && detailResponse.data.data)) {
      ElMessage.error('加载会话详情失败: ' + (detailResponse.data?.message || '无效的会话数据'));
      throw new Error('Failed to load session detail');
    }
    
    const sessionData = detailResponse.data.data;
    sessionId.value = sessionData.id; // Set current session ID

    // Populate aiSessionData with current_structured_data from the session
    // current_structured_data is of type ProblemAIStructuredData
    if (sessionData.current_structured_data) {
      Object.assign(aiSessionData, sessionData.current_structured_data);
      // Ensure session_id from the parent is also in aiSessionData if needed by some logic, though redundant
      aiSessionData.session_id = sessionData.id;
    } else {
      // Fallback if current_structured_data is missing, try to use initial hints if any
      aiSessionData.session_id = sessionData.id;
      aiSessionData.suggested_subject = sessionData.initial_subject_hint || '';
      // original_image_ref will be set in aiSessionData by Object.assign if present in sessionData.current_structured_data
      // If not, we might need to get it from sessionData.initial_image_ref if it's part of the main session schema
      // Assuming ProblemAISessionSchema also contains initial_image_ref at top level for display
      aiSessionData.original_image_ref = sessionData.initial_image_ref || null;
    }
    
    // Populate form from aiSessionData (which now contains current_structured_data)
    form.title = aiSessionData.title || '';
    form.content = aiSessionData.extracted_content || ''; // OCR content primarily
    form.subject = aiSessionData.suggested_subject || sessionData.initial_subject_hint || '';
    form.category = aiSessionData.preliminary_category || '';
    form.tags = Array.isArray(aiSessionData.preliminary_tags) ? [...aiSessionData.preliminary_tags] : [];
    form.knowledge_points = Array.isArray(aiSessionData.knowledge_points) ? [...aiSessionData.knowledge_points] : [];
    
    // Populate other form fields if they exist in aiSessionData (e.g., from previous interactions)
    // These fields might not be in the initial ProblemAIStructuredData but could have been added by AI chat.
    // The ProblemAISessionSchema.current_structured_data should ideally hold the most complete version.
    form.user_answer = aiSessionData.user_answer || ''; // Assuming these could be in current_structured_data
    form.correct_answer = aiSessionData.correct_answer || '';
    form.error_analysis = aiSessionData.error_analysis || '';
    form.solution = aiSessionData.solution || '';
    form.difficulty_level = aiSessionData.difficulty;


    isAIImported.value = true; // Mark as AI imported to enable chat UI etc.

    // Fetch and populate chat history
    const chatHistoryResponse = await problemAPI.getAISessionChatHistory(resumeSessionId);
    if (chatHistoryResponse.data && chatHistoryResponse.data.success) {
      chatHistory.value = chatHistoryResponse.data.data.map(log => ({
        role: log.role,
        content: log.content,
        timestamp: log.created_at, // Assuming log entry has created_at
        // type might be needed if we distinguish error messages in chat history display
      }));
    } else {
      ElMessage.warning('加载聊天记录失败: ' + (chatHistoryResponse.data?.message || '未知错误'));
    }

    ElMessage.success(`AI会话 ${resumeSessionId} 已加载。`);
    // Note: uploadedImageFile will be null. The UI should reflect that we are resuming,
    // and maybe show the initial_image_ref from aiSessionData if available.
    // The user cannot re-upload an image for a resumed session via the standard upload component.

  } catch (error) {
    console.error('Error resuming session:', error);
    ElMessage.error('恢复会话失败: ' + (error.response?.data?.detail || error.message || '未知错误'));
    router.push({ name: 'problem-ai-sessions' }); // Redirect if loading fails
  } finally {
    loading.value = false;
    aiProcessing.value = false;
  }
};

onMounted(() => {
  const resumeSessionId = route.query.resume_session_id;
  if (resumeSessionId) {
    loadSessionForResuming(resumeSessionId);
  } else {
    resetComponentState(); // Ensure clean state for new problem creation
  }
});

// Watch for route changes if users navigate away and back with a different session ID (optional)
// watch(() => route.query.resume_session_id, (newId, oldId) => {
//   if (newId && newId !== oldId) {
//     loadSessionForResuming(newId);
//   } else if (!newId && oldId) { // Navigated away from a resume session to create new
//     resetComponentState();
//   }
// });


// 应用AI分析结果 (This can be re-purposed or simplified if form is populated directly)
const applyAIResult = () => {
  // This function might be used if the dialog allows editing aiSessionData,
  // and then user clicks "Apply" to push those changes to the main 'form'.
  // For now, 'handleStartAIImageImport' populates 'form' directly.
  // So, this function could be used to "re-apply" or confirm.
  
  // Example: re-populate form from aiSessionData (which might have been viewed/confirmed in dialog)
  form.title = aiSessionData.title || form.title; // Keep manual edits if aiSessionData field is empty
  form.content = aiSessionData.extracted_content || form.content;
  form.subject = aiSessionData.suggested_subject || form.subject;
  form.category = aiSessionData.preliminary_category || form.category;
  form.tags = Array.isArray(aiSessionData.preliminary_tags) ? 
              [...new Set([...form.tags, ...aiSessionData.preliminary_tags])] : form.tags;
  form.knowledge_points = Array.isArray(aiSessionData.knowledge_points) ? 
              [...new Set([...form.knowledge_points, ...aiSessionData.knowledge_points])] : form.knowledge_points;

  // Fields that are not typically in initial ProblemAIStructuredData but in aiAnalysisResult for the dialog
  // form.error_analysis = aiAnalysisResult.error_analysis || form.error_analysis;
  // form.difficulty = aiAnalysisResult.difficulty_level || form.difficulty;
  
  ElMessage.success('AI分析结果已同步到表单。');
  aiResultVisible.value = false;
}

// 查看相似题目
// The erroneously duplicated block of old applyAIResult logic that was here has been removed.
const viewSimilarProblem = (problem) => {
  // TODO: 实现查看相似题目的逻辑，例如跳转到详情页或弹窗显示
  console.log('查看相似题目:', problem);
  ElMessageBox.alert(`相似题目内容：${problem.content}`, problem.title, {
    confirmButtonText: '确定'
  });
};

// fileToBase64 function removed as it's no longer needed.

const handleSubmit = async () => {
  const valid = await formRef.value.validate();
  if (!valid) return;

  loading.value = true;
  try {
    if (sessionId.value && isAIImported.value) {
      // AI-driven flow: finalize the problem
      const finalizePayload = {
        session_id: sessionId.value,
        // Core problem fields from the form
        title: form.title,
        content: form.content,
        subject: form.subject,
        category: form.category,
        source: form.source, // Assuming form has source
        year: form.year,     // Assuming form has year
        user_answer: form.user_answer,
        correct_answer: form.correct_answer,
        error_analysis: form.error_analysis,
        solution: form.solution, // Assuming form has solution or AI provided it
        knowledge_points: form.knowledge_points,
        difficulty_level: form.difficulty_level || aiSessionData.difficulty || 3, // Prioritize form, then AI, then default
        image_urls: aiSessionData.original_image_ref ? [aiSessionData.original_image_ref] : [], // Use image ref from session
        mastery_level: 0.0, // Default for new problems
        tags: form.tags,
        notes: form.notes, // Assuming form has notes
        
        // AI specific data for finalization
        // ai_full_analysis_json: Could be a more detailed structure if the chat updated it.
        // For now, using the current aiSessionData (which holds initial structured OCR + potentially updated fields)
        // A more robust implementation might involve collecting all AI interactions and suggestions into one comprehensive JSON.
        ai_full_analysis_json: { ...aiSessionData }, 
        
        // chat_logs: Backend will re-fetch by session_id. If sending from frontend, ensure it matches AIChatLogEntryCreateSchema.
        // For simplicity and consistency with current backend service, let's not send chat_logs array from here,
        // as the backend finalize method re-fetches them based on session_id.
        // If we were to send them, they'd need 'order_in_conversation' and 'content_type'.
        // chat_logs: chatHistory.value.map((msg, index) => ({ 
        //   role: msg.role,
        //   content: msg.content,
        //   content_type: "text", // Default
        //   order_in_conversation: index 
        // }))
      };
      
      try {
        const response = await problemAPI.finalizeAICreation(finalizePayload); // Use updated API method name
        if (response.data && response.data.success && response.data.data.id) {
          ElMessage.success('AI辅助错题创建成功！');
          router.push(`/problems/detail/${response.data.data.id}`);
        } else {
          ElMessage.error('AI辅助错题创建失败: ' + (response.data.message || '未知错误'));
        }
      } catch (e) {
          console.error("Finalize AI Creation Error:", e);
          ElMessage.error('AI辅助错题最终确认失败: ' + (e.response?.data?.detail || e.message || '未知错误'));
      }

    } else {
      // Manual creation flow
      // If an image was uploaded for manual flow, convert it to base64 or handle as needed by `problemAPI.create`
      const createData = { ...form };
      if (uploadedImageFile.value && uploadedImageFile.value.raw) {
        // Assuming problemAPI.create can handle a base64 string or expects a separate upload step
        // For now, let's add image_base64 similar to how ProblemCreate schema in backend might handle it.
        // This part might need adjustment based on `problemAPI.create`'s actual capabilities.
        // A common pattern is to upload image first, get URL, then include URL in create payload.
        // Or, include base64 directly.
        // Let's assume for now `problemAPI.create` can handle image_base64
        // This is a simplification. A robust solution would upload first, then create with URL.
        // Or ensure backend `ProblemCreate` schema can handle `image_base64: Optional[List[str]]`
        
        // To keep it simple for now, we will not pass image data directly with manual create.
        // The user should use the "AI 图片识别导入" or a separate image management feature.
        // If `problemAPI.create` is to handle images, it needs to be explicit.
        // For this pass, we assume images for manual creation are handled separately or not at all through this direct form.
        // If `image_urls` were populated from some other source, they would be in `form.image_urls`.
         if (!createData.image_urls || createData.image_urls.length === 0) {
             // If no image_urls from AI process, and user manually uploaded one, 
             // it implies they didn't use AI import.
             // Manual flow typically means no image or image handled by other means.
             // For simplicity, we will clear image_urls if not from AI flow for manual create.
             // This depends on whether your `ProblemCreate` backend schema expects image_urls
             // for manual creation or if images are linked differently.
             // Let's assume manual create does not automatically link the uploaded image here.
         }
      }


      await problemAPI.create(createData);
      ElMessage.success('错题创建成功！');
      router.push('/problems');
    }
  } catch (error) {
    console.error("Problem creation/finalization error:", error);
    ElMessage.error('操作失败: ' + (error.response?.data?.detail || error.message || '未知错误'));
  } finally {
    loading.value = false;
  }
};
</script>

<style scoped>
.problem-create {
  padding: 20px;
}

.page-header {
  margin-bottom: 20px;
}

.page-header h1 {
  margin: 0;
}

.ai-analysis-section {
  margin-top: 15px;
  display: flex;
  align-items: center;
}

.ai-analysis-btn {
  margin-right: 10px;
}

.ai-tip {
  font-size: 12px;
  color: #909399;
}

.form-tip {
  font-size: 12px;
  color: #909399;
  margin-top: 5px;
}

.similar-problems {
  width: 100%;
}

.similar-problem-item {
  margin-bottom: 15px;
}

.similar-card {
  width: 100%;
}

.similar-content {
  margin-bottom: 10px;
}

.similar-preview {
  font-size: 14px;
  color: #606266;
  margin-bottom: 10px;
}

.similar-meta .el-tag {
  margin-right: 5px;
}

.similar-actions {
  text-align: right;
}

.analysis-section h4 {
  margin-top: 0;
}

.knowledge-points .el-tag {
  margin-right: 5px;
  margin-bottom: 5px;
}

.error-analysis-content,
.suggestions-list {
  white-space: pre-wrap; /* 保留换行符 */
}

.suggestions-list {
  padding-left: 20px;
}

.dialog-footer {
  text-align: right;
}

/* 图片上传组件样式 */
.problem-image-upload ::v-deep .el-upload--picture-card {
  width: 100px;
  height: 100px;
  line-height: 100px;
}

.problem-image-upload ::v-deep .el-upload-list--picture-card .el-upload-list__item {
  width: 100px;
  height: 100px;
}

/* Chat UI Styles */
.ai-chat-container {
  border: 1px solid #dcdfe6;
  border-radius: 4px;
  padding: 15px;
  background-color: #f9fafb;
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 10px; /* Add some space above the chat */
}

.chat-history {
  max-height: 300px; /* Or any desired height */
  overflow-y: auto;
  padding: 10px;
  border: 1px solid #e4e7ed;
  background-color: #fff;
  border-radius: 4px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.chat-message {
  display: flex;
  flex-direction: column;
}

.chat-message.user .message-bubble {
  background-color: #ecf5ff; /* Element Plus primary light blue */
  align-self: flex-end;
  border-radius: 10px 10px 0 10px;
  max-width: 70%;
}

.chat-message.ai .message-bubble {
  background-color: #f0f9eb; /* Element Plus success light green */
  align-self: flex-start;
  border-radius: 10px 10px 10px 0;
   max-width: 85%;
}

.message-bubble {
  padding: 8px 12px;
  border-radius: 4px;
  word-wrap: break-word; /* Ensure long words don't overflow */
  line-height: 1.5;
}

.message-bubble strong {
  display: block;
  font-size: 0.9em;
  margin-bottom: 4px;
  color: #303133;
}

.message-content-text {
  white-space: pre-wrap; /* Respects newlines and spaces from text */
}
.message-content-text .is-loading {
  margin-left: 5px;
  vertical-align: middle;
}


.message-meta {
  font-size: 0.75em;
  color: #909399;
  text-align: right;
  margin-top: 5px;
}

.chat-input-area {
  display: flex;
  gap: 10px;
  align-items: flex-end; /* Align button to bottom of textarea */
}

.chat-input-area .el-textarea {
  flex-grow: 1;
}
.send-chat-btn {
  height: auto; /* Adjust if textarea rows change */
  min-height: 32px; /* Default button height */
  align-self: flex-end; /* Stick to bottom if textarea grows */
}


.chat-starter {
  text-align: center;
  padding: 10px 0;
}

.error-text {
  color: #f56c6c; /* Element Plus error color */
  white-space: pre-wrap;
}

.ai-import-section {
  margin-top: 10px;
  display: flex;
  flex-direction: column; /* Stack button and tip */
  align-items: flex-start; /* Align items to the left */
  gap: 8px; /* Space between button and tip */
}

.ai-import-btn {
  /* Button styling if needed, already an ElButton */
}

.ai-import-status {
  margin-top: 10px;
}
</style>
