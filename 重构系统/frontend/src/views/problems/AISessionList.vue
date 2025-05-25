<template>
  <div class="ai-session-list-view">
    <div class="page-header">
      <h1>AI错题创建会话列表</h1>
      <el-button type="primary" @click="goToCreateProblem" :icon="Plus">
        创建新错题
      </el-button>
    </div>

    <el-card>
      <el-form :inline="true" :model="filters" @submit.prevent="fetchSessions">
        <el-form-item label="状态">
          <el-select v-model="filters.status" placeholder="所有状态" clearable @change="handleFilterChange">
            <el-option label="进行中 (Active)" value="active" />
            <el-option label="已完成 (Finalized)" value="finalized" />
            <el-option label="已中止 (Aborted)" value="aborted" />
          </el-select>
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="fetchSessions" :icon="Search">查询</el-button>
        </el-form-item>
      </el-form>

      <el-table :data="sessions" v-loading="loading" style="width: 100%">
        <el-table-column prop="id" label="会话ID" width="280">
          <template #default="scope">
            <el-link type="primary" @click="viewSessionDetail(scope.row.id)">{{ scope.row.id }}</el-link>
          </template>
        </el-table-column>
        <el-table-column prop="status" label="状态" width="120">
          <template #default="scope">
            <el-tag :type="getSessionStatusTagType(scope.row.status)">
              {{ formatSessionStatus(scope.row.status) }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="initial_subject_hint" label="学科提示" width="120">
           <template #default="scope">
            {{ scope.row.initial_subject_hint || '-' }}
          </template>
        </el-table-column>
        <el-table-column label="初始图片" width="150">
          <template #default="scope">
            <el-image 
              v-if="scope.row.initial_image_ref"
              style="width: 100px; height: 60px"
              :src="scope.row.initial_image_ref" 
              :preview-src-list="[scope.row.initial_image_ref]"
              fit="contain"
              lazy
            />
            <span v-else>-</span>
          </template>
        </el-table-column>
        <el-table-column prop="created_at" label="创建时间" sortable width="180">
          <template #default="scope">
            {{ new Date(scope.row.created_at).toLocaleString() }}
          </template>
        </el-table-column>
        <el-table-column prop="updated_at" label="更新时间" sortable width="180">
          <template #default="scope">
            {{ new Date(scope.row.updated_at).toLocaleString() }}
          </template>
        </el-table-column>
        <el-table-column label="操作" width="180" fixed="right">
          <template #default="scope">
            <el-button size="small" type="primary" @click="viewSessionDetail(scope.row.id)">详情</el-button>
            <el-button 
              v-if="scope.row.status === 'active'" 
              size="small" 
              @click="resumeSession(scope.row)"
              title="继续编辑此会话"
            >
              继续
            </el-button>
            <!-- Add delete/abort actions later if needed -->
          </template>
        </el-table-column>
      </el-table>

      <el-pagination
        v-if="totalSessions > 0"
        background
        layout="total, sizes, prev, pager, next, jumper"
        :total="totalSessions"
        :page-sizes="[10, 20, 50, 100]"
        :page-size="pagination.size"
        :current-page="pagination.page"
        @size-change="handleSizeChange"
        @current-change="handleCurrentChange"
        style="margin-top: 20px; text-align: right;"
      />
    </el-card>

    <!-- Session Detail Dialog (Placeholder - could be a separate route/component too) -->
    <el-dialog v-model="detailDialogVisible" title="会话详情" width="70%">
      <div v-if="selectedSessionDetail">
        <el-descriptions :column="2" border>
          <el-descriptions-item label="会话ID">{{ selectedSessionDetail.id }}</el-descriptions-item>
          <el-descriptions-item label="状态">
            <el-tag :type="getSessionStatusTagType(selectedSessionDetail.status)">
              {{ formatSessionStatus(selectedSessionDetail.status) }}
            </el-tag>
          </el-descriptions-item>
          <el-descriptions-item label="创建时间">{{ new Date(selectedSessionDetail.created_at).toLocaleString() }}</el-descriptions-item>
          <el-descriptions-item label="更新时间">{{ new Date(selectedSessionDetail.updated_at).toLocaleString() }}</el-descriptions-item>
          <el-descriptions-item label="学科提示">{{ selectedSessionDetail.initial_subject_hint || '-' }}</el-descriptions-item>
          <el-descriptions-item label="初始图片">
             <el-image 
              v-if="selectedSessionDetail.initial_image_ref"
              style="width: 150px; height: 100px"
              :src="selectedSessionDetail.initial_image_ref" 
              :preview-src-list="[selectedSessionDetail.initial_image_ref]"
              fit="contain"
            />
            <span v-else>-</span>
          </el-descriptions-item>
           <el-descriptions-item label="最终错题ID" v-if="selectedSessionDetail.final_problem_id">
            <router-link :to="`/problems/detail/${selectedSessionDetail.final_problem_id}`">
              {{ selectedSessionDetail.final_problem_id }}
            </router-link>
          </el-descriptions-item>
        </el-descriptions>
        
        <h4 style="margin-top: 20px;">当前结构化数据:</h4>
        <pre style="background-color: #f5f5f5; padding: 10px; border-radius: 4px; max-height: 200px; overflow-y: auto;">{{ JSON.stringify(selectedSessionDetail.current_structured_data, null, 2) }}</pre>

        <h4 style="margin-top: 20px;">聊天记录:</h4>
        <div class="chat-history-dialog" v-if="selectedSessionChatHistory.length > 0">
          <div v-for="(message, index) in selectedSessionChatHistory" :key="index" :class="['chat-message', message.role]">
            <div class="message-bubble-dialog">
              <strong>{{ message.role === 'user' ? '你' : 'AI助手' }}:</strong>
              <p style="white-space: pre-wrap;">{{ message.content }}</p>
            </div>
          </div>
        </div>
        <p v-else>暂无聊天记录。</p>

      </div>
      <div v-else>加载详情中...</div>
      <template #footer>
        <el-button @click="detailDialogVisible = false">关闭</el-button>
      </template>
    </el-dialog>

  </div>
</template>

<script setup>
import { ref, reactive, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { ElMessage, ElMessageBox } from 'element-plus';
import { problemAPI } from '@/utils/api';
import { Plus, Search } from '@element-plus/icons-vue'; // Added Search

const router = useRouter();
const sessions = ref([]);
const loading = ref(false);
const totalSessions = ref(0);
const pagination = reactive({
  page: 1,
  size: 10,
});
const filters = reactive({
  status: null,
});

const detailDialogVisible = ref(false);
const selectedSessionDetail = ref(null);
const selectedSessionChatHistory = ref([]);

const fetchSessions = async () => {
  loading.value = true;
  try {
    const params = {
      page: pagination.page,
      size: pagination.size,
      status: filters.status || undefined, // Pass undefined if null/empty
      sort_by: 'updated_at', // Default sort
      sort_desc: true,
    };
    const response = await problemAPI.listAISessions(params);
    if (response.data && response.data.success) {
      // Backend returns { success, message, data, total, page, size, pages }
      sessions.value = response.data.data;
      totalSessions.value = response.data.total;
      // pagination.page and pagination.size are already set
    } else {
      ElMessage.error(response.data.message || '获取AI会话列表失败');
    }
  } catch (error) {
    console.error('Error fetching AI sessions:', error);
    ElMessage.error('获取AI会话列表失败: ' + (error.response?.data?.detail || error.message));
  } finally {
    loading.value = false;
  }
};

const handleFilterChange = () => {
  pagination.page = 1; // Reset to first page on filter change
  fetchSessions();
};

const handleSizeChange = (newSize) => {
  pagination.size = newSize;
  fetchSessions();
};

const handleCurrentChange = (newPage) => {
  pagination.page = newPage;
  fetchSessions();
};

const getSessionStatusTagType = (status) => {
  switch (status) {
    case 'active': return 'warning';
    case 'finalized': return 'success';
    case 'aborted': return 'danger';
    default: return 'info';
  }
};

const formatSessionStatus = (status) => {
  switch (status) {
    case 'active': return '进行中';
    case 'finalized': return '已完成';
    case 'aborted': return '已中止';
    default: return status;
  }
};

const viewSessionDetail = async (sessionId) => {
  selectedSessionDetail.value = null; // Reset previous detail
  selectedSessionChatHistory.value = [];
  detailDialogVisible.value = true;
  try {
    const detailResponse = await problemAPI.getAISessionDetail(sessionId);
    if (detailResponse.data && detailResponse.data.success) {
      selectedSessionDetail.value = detailResponse.data.data;
      // Fetch chat history if session detail loaded successfully
      const chatHistoryResponse = await problemAPI.getAISessionChatHistory(sessionId);
      if (chatHistoryResponse.data && chatHistoryResponse.data.success) {
        selectedSessionChatHistory.value = chatHistoryResponse.data.data;
      } else {
        ElMessage.warning('获取聊天记录失败: ' + (chatHistoryResponse.data.message || '未知错误'));
      }
    } else {
      ElMessage.error('获取会话详情失败: ' + (detailResponse.data.message || '未知错误'));
      detailDialogVisible.value = false; // Close dialog on error
    }
  } catch (error) {
    console.error('Error fetching session detail or chat history:', error);
    ElMessage.error('获取会话详情失败: ' + (error.response?.data?.detail || error.message));
    detailDialogVisible.value = false; // Close dialog on error
  }
};

const resumeSession = (session) => {
  if (session.status === 'active') {
    // Navigate to ProblemCreate view, passing session ID as a query parameter.
    router.push({ name: 'problem-create', query: { resume_session_id: session.id } });
  } else {
    ElMessage.warning('只有进行中的会话才能继续。');
  }
};

const goToCreateProblem = () => {
  router.push({ name: 'ProblemCreate' }); // Assuming 'ProblemCreate' is the route name
};


onMounted(() => {
  fetchSessions();
});

</script>

<style scoped>
.ai-session-list-view {
  padding: 20px;
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.page-header h1 {
  margin: 0;
}

.el-form {
  margin-bottom: 20px;
}

.chat-history-dialog {
  max-height: 300px;
  overflow-y: auto;
  border: 1px solid #eee;
  padding: 10px;
  margin-top: 10px;
  background-color: #f9f9f9;
}

.chat-message {
  margin-bottom: 8px;
}

.message-bubble-dialog {
  padding: 6px 10px;
  border-radius: 6px;
  display: inline-block; /* Make bubbles only as wide as content */
  max-width: 90%;
}

.chat-message.user .message-bubble-dialog {
  background-color: #d9ecff;
  text-align: left; /* Align user messages to the right conceptually, but text left */
}

.chat-message.ai .message-bubble-dialog {
  background-color: #e1f3d8;
  text-align: left;
}

.chat-message.user {
 display: flex;
 justify-content: flex-end; /* This will push the bubble to the right */
}
.chat-message.ai {
 display: flex;
 justify-content: flex-start; /* This will push the bubble to the left */
}
</style>
