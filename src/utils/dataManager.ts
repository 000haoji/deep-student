// 数据管理模块 - 模拟本地数据存储
export interface MistakeItem {
  id: string;
  subject: string;
  created_at: string;
  question_images: string[];
  analysis_images: string[];
  user_question: string;
  ocr_text: string;
  tags: string[];
  mistake_type: string;
  status: string;
  chat_history?: ChatMessage[];
}

export interface ChatMessage {
  role: string;
  content: string;
  timestamp: string;
}

export interface ReviewSession {
  id: string;
  subject: string;
  selected_mistake_ids: string[];
  analysis: string;
  created_at: string;
  chat_history: ChatMessage[];
}

export interface AnalysisRequest {
  subject: string;
  question_image_files: File[];
  analysis_image_files: File[];
  user_question: string;
}

export interface AnalysisResponse {
  temp_id: string;
  initial_data: {
    ocr_text: string;
    tags: string[];
    mistake_type: string;
    first_answer: string;
  };
}

// 本地存储键名
const STORAGE_KEYS = {
  MISTAKES: 'ai_mistake_manager_mistakes',
  REVIEWS: 'ai_mistake_manager_reviews',
  SETTINGS: 'ai_mistake_manager_settings',
  TEMP_SESSIONS: 'ai_mistake_manager_temp_sessions'
};

// 模拟数据
const MOCK_MISTAKES: MistakeItem[] = [
  {
    id: '1',
    subject: '数学',
    created_at: '2024-01-15T10:00:00Z',
    question_images: [],
    analysis_images: [],
    user_question: '二次函数的最值问题',
    ocr_text: '求函数f(x) = x² + 2x + 1的最小值',
    tags: ['二次函数', '最值问题', '配方法'],
    mistake_type: '计算题',
    status: 'completed',
    chat_history: [
      {
        role: 'assistant',
        content: '这是一道二次函数求最值的问题。通过配方法可以得到 f(x) = (x+1)²，所以最小值为0。',
        timestamp: '2024-01-15T10:05:00Z'
      }
    ]
  },
  {
    id: '2',
    subject: '数学',
    created_at: '2024-01-16T14:30:00Z',
    question_images: [],
    analysis_images: [],
    user_question: '二次函数图像问题',
    ocr_text: '画出函数y = x² - 4x + 3的图像',
    tags: ['二次函数', '图像', '顶点'],
    mistake_type: '作图题',
    status: 'completed',
    chat_history: [
      {
        role: 'assistant',
        content: '这个函数可以配方为 y = (x-2)² - 1，顶点为(2, -1)，开口向上。',
        timestamp: '2024-01-16T14:35:00Z'
      }
    ]
  },
  {
    id: '3',
    subject: '数学',
    created_at: '2024-01-17T09:15:00Z',
    question_images: [],
    analysis_images: [],
    user_question: '一元二次方程求解',
    ocr_text: '解方程x² - 5x + 6 = 0',
    tags: ['一元二次方程', '因式分解', '求根公式'],
    mistake_type: '计算题',
    status: 'completed',
    chat_history: [
      {
        role: 'assistant',
        content: '这个方程可以因式分解为 (x-2)(x-3) = 0，所以解为 x = 2 或 x = 3。',
        timestamp: '2024-01-17T09:20:00Z'
      }
    ]
  },
  {
    id: '4',
    subject: '物理',
    created_at: '2024-01-18T16:45:00Z',
    question_images: [],
    analysis_images: [],
    user_question: '牛顿第二定律应用',
    ocr_text: '质量为2kg的物体受到10N的力，求加速度',
    tags: ['牛顿定律', '力学', '加速度'],
    mistake_type: '计算题',
    status: 'completed',
    chat_history: [
      {
        role: 'assistant',
        content: '根据牛顿第二定律 F = ma，可得 a = F/m = 10/2 = 5 m/s²。',
        timestamp: '2024-01-18T16:50:00Z'
      }
    ]
  },
  {
    id: '5',
    subject: '数学',
    created_at: '2024-01-19T11:20:00Z',
    question_images: [],
    analysis_images: [],
    user_question: '二次函数与x轴交点',
    ocr_text: '求函数y = x² - 3x + 2与x轴的交点',
    tags: ['二次函数', '交点', '判别式'],
    mistake_type: '计算题',
    status: 'completed',
    chat_history: [
      {
        role: 'assistant',
        content: '令y=0，得到 x² - 3x + 2 = 0，因式分解得 (x-1)(x-2) = 0，所以交点为(1,0)和(2,0)。',
        timestamp: '2024-01-19T11:25:00Z'
      }
    ]
  }
];

class DataManager {
  // 初始化数据
  static initializeData() {
    if (!localStorage.getItem(STORAGE_KEYS.MISTAKES)) {
      localStorage.setItem(STORAGE_KEYS.MISTAKES, JSON.stringify(MOCK_MISTAKES));
    }
    if (!localStorage.getItem(STORAGE_KEYS.REVIEWS)) {
      localStorage.setItem(STORAGE_KEYS.REVIEWS, JSON.stringify([]));
    }
    if (!localStorage.getItem(STORAGE_KEYS.TEMP_SESSIONS)) {
      localStorage.setItem(STORAGE_KEYS.TEMP_SESSIONS, JSON.stringify({}));
    }
  }

  // 错题管理
  static getMistakes(filter?: { subject?: string; tags?: string[]; type?: string }): MistakeItem[] {
    this.initializeData();
    const mistakes = JSON.parse(localStorage.getItem(STORAGE_KEYS.MISTAKES) || '[]');
    
    if (!filter) return mistakes;
    
    return mistakes.filter((mistake: MistakeItem) => {
      if (filter.subject && mistake.subject !== filter.subject) return false;
      if (filter.type && mistake.mistake_type !== filter.type) return false;
      if (filter.tags && !filter.tags.some(tag => mistake.tags.includes(tag))) return false;
      return true;
    });
  }

  static getMistakeById(id: string): MistakeItem | null {
    const mistakes = this.getMistakes();
    return mistakes.find(mistake => mistake.id === id) || null;
  }

  static saveMistake(mistake: MistakeItem): boolean {
    try {
      const mistakes = this.getMistakes();
      const existingIndex = mistakes.findIndex(m => m.id === mistake.id);
      
      if (existingIndex >= 0) {
        mistakes[existingIndex] = mistake;
      } else {
        mistakes.push(mistake);
      }
      
      localStorage.setItem(STORAGE_KEYS.MISTAKES, JSON.stringify(mistakes));
      return true;
    } catch (error) {
      console.error('保存错题失败:', error);
      return false;
    }
  }

  static deleteMistake(id: string): boolean {
    try {
      const mistakes = this.getMistakes();
      const filteredMistakes = mistakes.filter(mistake => mistake.id !== id);
      localStorage.setItem(STORAGE_KEYS.MISTAKES, JSON.stringify(filteredMistakes));
      return true;
    } catch (error) {
      console.error('删除错题失败:', error);
      return false;
    }
  }

  // 临时会话管理
  static saveTempSession(tempId: string, data: any): void {
    const sessions = JSON.parse(localStorage.getItem(STORAGE_KEYS.TEMP_SESSIONS) || '{}');
    sessions[tempId] = data;
    localStorage.setItem(STORAGE_KEYS.TEMP_SESSIONS, JSON.stringify(sessions));
  }

  static getTempSession(tempId: string): any {
    const sessions = JSON.parse(localStorage.getItem(STORAGE_KEYS.TEMP_SESSIONS) || '{}');
    return sessions[tempId] || null;
  }

  static deleteTempSession(tempId: string): void {
    const sessions = JSON.parse(localStorage.getItem(STORAGE_KEYS.TEMP_SESSIONS) || '{}');
    delete sessions[tempId];
    localStorage.setItem(STORAGE_KEYS.TEMP_SESSIONS, JSON.stringify(sessions));
  }

  // 回顾分析管理
  static getReviewSessions(): ReviewSession[] {
    this.initializeData();
    return JSON.parse(localStorage.getItem(STORAGE_KEYS.REVIEWS) || '[]');
  }

  static saveReviewSession(session: ReviewSession): boolean {
    try {
      const sessions = this.getReviewSessions();
      const existingIndex = sessions.findIndex(s => s.id === session.id);
      
      if (existingIndex >= 0) {
        sessions[existingIndex] = session;
      } else {
        sessions.push(session);
      }
      
      localStorage.setItem(STORAGE_KEYS.REVIEWS, JSON.stringify(sessions));
      return true;
    } catch (error) {
      console.error('保存回顾分析失败:', error);
      return false;
    }
  }

  // 设置管理
  static getSettings(): any {
    const defaultSettings = {
      apiKey: '',
      model1: 'gpt-4-vision-preview',
      model2: 'gpt-4',
      subjects: ['数学', '物理', '化学', '英语'],
      prompts: {
        数学: {
          model1: '请分析这道数学题目，提取题目文本、分类和标签。',
          model2: '请详细解答这道数学题目，提供清晰的解题步骤。'
        },
        物理: {
          model1: '请分析这道物理题目，提取题目文本、分类和标签。',
          model2: '请详细解答这道物理题目，说明物理原理和计算过程。'
        },
        化学: {
          model1: '请分析这道化学题目，提取题目文本、分类和标签。',
          model2: '请详细解答这道化学题目，解释化学反应和计算过程。'
        },
        英语: {
          model1: '请分析这道英语题目，提取题目文本、分类和标签。',
          model2: '请详细解答这道英语题目，提供语法解释和答案分析。'
        }
      }
    };
    
    const settings = localStorage.getItem(STORAGE_KEYS.SETTINGS);
    return settings ? { ...defaultSettings, ...JSON.parse(settings) } : defaultSettings;
  }

  static saveSettings(settings: any): boolean {
    try {
      localStorage.setItem(STORAGE_KEYS.SETTINGS, JSON.stringify(settings));
      return true;
    } catch (error) {
      console.error('保存设置失败:', error);
      return false;
    }
  }

  // 统计信息
  static getStatistics() {
    const mistakes = this.getMistakes();
    const reviews = this.getReviewSessions();
    
    const subjectStats = mistakes.reduce((acc, mistake) => {
      acc[mistake.subject] = (acc[mistake.subject] || 0) + 1;
      return acc;
    }, {} as Record<string, number>);

    const typeStats = mistakes.reduce((acc, mistake) => {
      acc[mistake.mistake_type] = (acc[mistake.mistake_type] || 0) + 1;
      return acc;
    }, {} as Record<string, number>);

    const tagStats = mistakes.reduce((acc, mistake) => {
      mistake.tags.forEach(tag => {
        acc[tag] = (acc[tag] || 0) + 1;
      });
      return acc;
    }, {} as Record<string, number>);

    return {
      totalMistakes: mistakes.length,
      totalReviews: reviews.length,
      subjectStats,
      typeStats,
      tagStats,
      recentMistakes: mistakes
        .sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())
        .slice(0, 5)
    };
  }
}

// 模拟API调用
export class MockAPI {
  static async analyzeNewMistake(request: AnalysisRequest): Promise<AnalysisResponse> {
    // 模拟网络延迟
    await new Promise(resolve => setTimeout(resolve, 2000 + Math.random() * 1000));
    
    const tempId = 'temp-' + Date.now() + '-' + Math.random().toString(36).substr(2, 9);
    
    // 根据科目生成不同的分析结果
    const subjectAnalysis = {
      '数学': {
        tags: ['二次函数', '最值问题', '配方法'],
        type: '计算题',
        answer: '这是一道二次函数求最值的问题。通过配方法可以将函数转换为顶点式，从而求出最值。'
      },
      '物理': {
        tags: ['牛顿定律', '力学', '运动学'],
        type: '计算题',
        answer: '这是一道力学问题。需要运用牛顿定律分析物体的运动状态和受力情况。'
      },
      '化学': {
        tags: ['化学反应', '计算', '平衡'],
        type: '计算题',
        answer: '这是一道化学计算题。需要根据化学方程式和反应原理进行计算。'
      },
      '英语': {
        tags: ['语法', '阅读理解', '词汇'],
        type: '选择题',
        answer: '这是一道英语题目。需要理解语法规则和词汇含义来选择正确答案。'
      }
    };

    const analysis = subjectAnalysis[request.subject as keyof typeof subjectAnalysis] || subjectAnalysis['数学'];
    
    const response: AnalysisResponse = {
      temp_id: tempId,
      initial_data: {
        ocr_text: `模拟OCR识别结果：这是一道关于${request.subject}的题目。用户问题：${request.user_question}`,
        tags: analysis.tags,
        mistake_type: analysis.type,
        first_answer: `${analysis.answer}\n\n**解题思路：**\n1. 分析题目条件\n2. 选择合适的解题方法\n3. 按步骤计算\n4. 验证答案\n\n您对这个解答有什么疑问吗？`
      }
    };

    // 保存临时会话
    DataManager.saveTempSession(tempId, {
      request,
      response,
      created_at: new Date().toISOString()
    });

    return response;
  }

  static async continueChat(_tempId: string, chatHistory: ChatMessage[]): Promise<{ new_assistant_message: string }> {
    // 模拟网络延迟
    await new Promise(resolve => setTimeout(resolve, 1000 + Math.random() * 500));
    
    const lastUserMessage = chatHistory[chatHistory.length - 1]?.content || '';
    
    // 智能回复逻辑
    let response = '';
    if (lastUserMessage.includes('步骤') || lastUserMessage.includes('详细')) {
      response = '我来为您详细解释每个步骤：\n\n1. **第一步**：仔细阅读题目，理解题意\n2. **第二步**：确定已知条件和求解目标\n3. **第三步**：选择合适的公式或方法\n4. **第四步**：代入数值进行计算\n5. **第五步**：检查答案的合理性\n\n您希望我详细解释哪个步骤？';
    } else if (lastUserMessage.includes('为什么') || lastUserMessage.includes('原理')) {
      response = '关于您提到的原理问题：\n\n这个方法的理论基础是...\n\n**核心思想**：\n- 利用数学/物理的基本定律\n- 通过逻辑推理得出结论\n- 验证结果的正确性\n\n您还想了解哪方面的原理？';
    } else if (lastUserMessage.includes('例题') || lastUserMessage.includes('练习')) {
      response = '我为您推荐一些类似的练习题：\n\n**基础练习**：\n1. 类似题型的简化版本\n2. 相同方法的不同应用\n\n**进阶练习**：\n1. 综合性更强的题目\n2. 需要多种方法结合的问题\n\n您希望我提供具体的练习题吗？';
    } else {
      response = `我理解您的问题。让我从另一个角度来解释：\n\n${lastUserMessage.includes('不懂') ? '看起来您对某个概念还不太清楚，' : ''}基于您的问题，我建议：\n\n1. 回顾相关的基础知识\n2. 多做类似的练习题\n3. 总结解题规律\n\n您还有什么具体的疑问吗？`;
    }

    return { new_assistant_message: response };
  }

  static async saveToLibrary(tempId: string, chatHistory: ChatMessage[]): Promise<{ success: boolean; mistake_item: MistakeItem }> {
    const tempSession = DataManager.getTempSession(tempId);
    if (!tempSession) {
      throw new Error('临时会话不存在');
    }

    const mistakeItem: MistakeItem = {
      id: 'mistake-' + Date.now() + '-' + Math.random().toString(36).substr(2, 9),
      subject: tempSession.request.subject,
      created_at: new Date().toISOString(),
      question_images: [], // 在实际应用中这里会是图片路径
      analysis_images: [],
      user_question: tempSession.request.user_question,
      ocr_text: tempSession.response.initial_data.ocr_text,
      tags: tempSession.response.initial_data.tags,
      mistake_type: tempSession.response.initial_data.mistake_type,
      status: 'completed',
      chat_history: chatHistory
    };

    const success = DataManager.saveMistake(mistakeItem);
    if (success) {
      DataManager.deleteTempSession(tempId);
    }

    return { success, mistake_item: mistakeItem };
  }

  static async reviewAnalysis(mistakeIds: string[]): Promise<string> {
    // 模拟网络延迟
    await new Promise(resolve => setTimeout(resolve, 3000));
    
    const mistakes = mistakeIds.map(id => DataManager.getMistakeById(id)).filter(Boolean) as MistakeItem[];
    
    if (mistakes.length === 0) {
      throw new Error('没有找到有效的错题');
    }

    const subjects = [...new Set(mistakes.map(m => m.subject))];
    const tags = [...new Set(mistakes.flatMap(m => m.tags))];
    const types = [...new Set(mistakes.map(m => m.mistake_type))];
    
    return `## 📊 回顾分析报告

### 📈 整体概况
- **分析题目数量**: ${mistakes.length} 道
- **涉及科目**: ${subjects.join('、')}
- **题目类型**: ${types.join('、')}
- **主要知识点**: ${tags.slice(0, 5).join('、')}

### 🔍 关联分析
通过对您选择的 ${mistakes.length} 道错题进行深入分析，我发现了以下关联模式：

**1. 知识点关联**
${subjects.includes('数学') ? `
- 二次函数相关问题占比较高，涉及最值、图像、交点等多个方面
- 这些问题都围绕二次函数的基本性质展开，说明您需要加强对二次函数整体概念的理解
` : ''}

**2. 解题方法关联**
- 多道题目都涉及到基础运算和概念理解
- 建议加强基础知识的掌握和解题方法的练习

### ⚠️ 易错点分析
根据您的错题模式，识别出以下易错点：

**1. 概念理解不够深入**
- 对基本概念的理解还需要加强
- 建议：系统复习相关概念，理解其本质含义

**2. 计算准确性有待提高**
- 在计算过程中容易出错
- 建议：加强基础运算练习，注意计算步骤的规范性

**3. 方法选择不够灵活**
- 对不同题型的解题方法掌握不够熟练
- 建议：多做不同类型的题目，总结解题规律

### 📚 复习建议
1. **系统复习基础知识**，确保概念理解准确
2. **加强计算训练**，提高计算准确性和速度
3. **多做综合性题目**，培养知识点之间的联系思维
4. **重视错题回顾**，定期复习已做错的题目

### 🎯 下一步学习重点
- 重点复习出现频率高的知识点
- 加强薄弱环节的专项训练
- 培养良好的解题习惯
- 建立完整的知识体系

希望这个分析对您的学习有所帮助！如有疑问，请随时提问。`;
  }
}

export default DataManager; 