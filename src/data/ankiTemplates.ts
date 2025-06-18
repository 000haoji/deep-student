import { AnkiCardTemplate, CustomAnkiTemplate, FieldExtractionRule, FieldType } from '../types';

// 将预置模板转换为CustomAnkiTemplate格式，实现统一管理
const createBuiltInTemplate = (template: AnkiCardTemplate): CustomAnkiTemplate => {
  // 生成字段提取规则
  const fieldExtractionRules: Record<string, FieldExtractionRule> = {};
  template.fields.forEach(field => {
    const fieldLower = field.toLowerCase();
    
    // 根据字段名称和模板类型确定字段属性
    let isRequired = false;
    let fieldType: FieldType = 'Text';
    let defaultValue = '';
    
    // 必需字段判断
    if (['front', 'back', 'text'].includes(fieldLower)) {
      isRequired = true;
    }
    
    // 字段类型判断
    if (fieldLower === 'tags') {
      fieldType = 'Array';
      defaultValue = '[]';
    }
    
    // 填空题特殊字段
    if (template.id === 'cloze-card') {
      if (fieldLower === 'text') {
        isRequired = true;
        defaultValue = '';
      }
    }
    
    // 选择题特殊字段
    if (template.id === 'choice-card') {
      if (['front', 'optiona', 'optionb', 'optionc', 'optiond', 'correct', 'explanation'].includes(fieldLower)) {
        isRequired = true;
      }
    }
    
    fieldExtractionRules[field] = {
      field_type: fieldType,
      is_required: isRequired,
      default_value: defaultValue,
      description: `${field} 字段`
    };
  });

  return {
    id: template.id,
    name: template.name,
    description: template.description,
    author: '系统内置',
    version: '1.0.0',
    preview_front: template.preview_front,
    preview_back: template.preview_back,
    note_type: template.note_type,
    fields: template.fields,
    generation_prompt: template.generation_prompt,
    front_template: template.front_template,
    back_template: template.back_template,
    css_style: template.css_style,
    field_extraction_rules: fieldExtractionRules,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    is_active: true,
    is_built_in: true
  };
};

// 原始模板数据（用于兼容性）
export const LEGACY_ANKI_CARD_TEMPLATES: AnkiCardTemplate[] = [
  {
    id: 'minimal-card',
    name: '极简卡片',
    description: '简洁优雅的极简设计，专注内容展示',
    preview_front: '什么是动量守恒定律？',
    preview_back: '在没有外力作用的系统中，系统的总动量保持不变',
    note_type: 'Basic',
    fields: ['Front', 'Back', 'Notes', 'Tags'],
    generation_prompt: `你是一个专业的ANKI卡片制作专家。请根据提供的学习材料生成极简风格的记忆卡片。

请按照以下格式生成卡片，每张卡片用JSON格式表示：

{
  "Front": "简洁明了的问题或概念名称",
  "Back": "准确、清晰的答案或解释",
  "Notes": "可选的补充说明或注释，帮助理解记忆",
  "Tags": ["相关标签"]
}

要求：
1. 问题简洁直接，便于快速理解
2. 答案准确完整，语言简练
3. 如果有重要的补充信息，可以添加到Notes字段
4. 每张卡片专注一个知识点
5. 适合反复记忆和复习

请根据内容的复杂度和知识点数量，生成适量的高质量记忆卡片。如果内容包含多个重要概念，应该为每个概念生成对应的卡片。`,
    front_template: `
<div class="card minimal-card">
  <div class="question">{{Front}}</div>
  <div class="hint">点击显示答案</div>
</div>
    `,
    back_template: `
<div class="card minimal-card">
  <div class="question">{{Front}}</div>
  <div class="hint">点击显示答案</div>
  
  <div class="answer">{{Back}}</div>
  
  {{#Notes}}
  <div class="notes">
    <div class="notes-label">注释：</div>
    <div>{{Notes}}</div>
  </div>
  {{/Notes}}
</div>
    `,
    css_style: `
.minimal-card {
  font-family: 'Segoe UI', system-ui, sans-serif;
  width: 100%;
  max-width: 100%;
  margin: 0;
  padding: 20px;
  border-radius: 16px;
  background: white;
  box-shadow: 0 5px 25px rgba(0,0,0,0.08);
  text-align: center;
  box-sizing: border-box;
  overflow: hidden;
}

.question {
  font-size: 20px;
  font-weight: 600;
  color: #2c3e50;
  line-height: 1.4;
  margin-bottom: 15px;
  word-wrap: break-word;
}

.answer {
  font-size: 16px;
  color: #27ae60;
  padding: 15px;
  background: #f9fbfa;
  border-radius: 12px;
  margin: 20px 0;
  border-left: 4px solid #2ecc71;
  display: block;
  word-wrap: break-word;
}

.hint {
  font-size: 12px;
  color: #95a5a6;
  font-style: italic;
  margin-bottom: 10px;
}

.notes {
  text-align: left;
  margin-top: 15px;
  padding: 12px;
  background: #f8f9fa;
  border-radius: 10px;
  font-size: 14px;
  color: #7f8c8d;
  word-wrap: break-word;
}

.notes-label {
  font-weight: 600;
  color: #3498db;
  margin-bottom: 5px;
}

.card:hover {
  box-shadow: 0 8px 30px rgba(0,0,0,0.12);
}
    `
  },
  {
    id: 'academic-card',
    name: '学术卡片',
    description: '正式的学术风格，适合专业知识和术语学习',
    preview_front: 'DNA复制',
    preview_back: 'DNA复制是一个半保留复制过程，包括解旋、引物合成、延伸等步骤',
    note_type: 'Basic',
    fields: ['Front', 'Back', 'Example', 'Source', 'Tags', 'Deck'],
    generation_prompt: `你是一个专业的学术ANKI卡片制作专家。请根据提供的学习材料生成学术风格的记忆卡片。

请按照以下格式生成卡片，每张卡片用JSON格式表示：

{
  "Front": "学术术语、概念或理论名称",
  "Back": "详细、准确的学术定义和解释",
  "Example": "具体的实例或应用场景（可选）",
  "Source": "来源或参考资料信息（可选）",
  "Tags": ["学科", "章节", "难度等标签"],
  "Deck": "所属学科或课程（可选）"
}

要求：
1. 术语和概念要准确，符合学术标准
2. 定义要完整、严谨，使用规范的学术语言
3. 提供具体实例或应用场景有助于理解
4. 标注来源信息有助于后续查阅
5. 适合专业学习和考试复习
6. 重点关注概念的精确性和完整性

请根据内容的复杂度和知识点数量，生成适量的高质量学术记忆卡片。如果内容包含多个重要概念，应该为每个概念生成对应的卡片。`,
    front_template: `
<div class="card academic-card">
  <div class="header">
    <div class="deck-name">{{Deck}}</div>
    <div class="card-type">知识卡片</div>
  </div>
  
  <div class="question">{{Front}}</div>
</div>
    `,
    back_template: `
<div class="card academic-card">
  <div class="header">
    <div class="deck-name">{{Deck}}</div>
    <div class="card-type">知识卡片</div>
  </div>
  
  <div class="question">{{Front}}</div>
  
  <div class="divider"></div>
  
  <div class="answer">
    <div class="definition">{{Back}}</div>
    {{#Example}}
    <div class="example">
      <div class="example-label">示例：</div>
      <div class="example-content">{{Example}}</div>
    </div>
    {{/Example}}
  </div>
  
  <div class="footer">
    <div class="source">{{Source}}</div>
    <div class="tags">{{Tags}}</div>
  </div>
</div>
    `,
    css_style: `
.academic-card {
  font-family: 'Georgia', serif;
  width: 100%;
  max-width: 100%;
  margin: 0;
  padding: 20px;
  background: #fcfaf7;
  border: 1px solid #e6e2dd;
  box-shadow: 0 3px 10px rgba(0,0,0,0.05);
  box-sizing: border-box;
  overflow: hidden;
}

.header {
  display: flex;
  justify-content: space-between;
  margin-bottom: 15px;
  font-size: 12px;
  color: #95a5a6;
}

.question {
  font-size: 20px;
  font-weight: bold;
  color: #2c3e50;
  text-align: center;
  margin: 15px 0 20px;
  word-wrap: break-word;
}

.divider {
  height: 1px;
  background: linear-gradient(90deg, transparent, #bdc3c7, transparent);
  margin: 10px 0 15px;
}

.definition {
  font-size: 16px;
  line-height: 1.6;
  color: #34495e;
  text-align: justify;
  word-wrap: break-word;
}

.example {
  margin-top: 15px;
  padding: 12px;
  background: #ffffff;
  border-left: 3px solid #3498db;
  border: 1px solid #3498db;
  word-wrap: break-word;
  color: #2c3e50;
  font-size: 14px;
  line-height: 1.5;
}

.example-label {
  font-weight: bold;
  color: #2980b9;
  margin-bottom: 5px;
}

.footer {
  display: flex;
  justify-content: space-between;
  margin-top: 20px;
  font-size: 11px;
  color: #7f8c8d;
  flex-wrap: wrap;
}

.tag {
  display: inline-block;
  background: #ecf0f1;
  padding: 2px 8px;
  border-radius: 12px;
  margin-left: 3px;
  margin-bottom: 2px;
  font-size: 10px;
}
    `
  },
  {
    id: 'code-card',
    name: '编程卡片',
    description: '专为编程学习设计的深色主题卡片',
    preview_front: '如何在Python中创建列表？',
    preview_back: 'my_list = [1, 2, 3, 4, 5]',
    note_type: 'Basic',
    fields: ['Front', 'Back', 'Code', 'Tags'],
    generation_prompt: `你是一个专业的编程ANKI卡片制作专家。请根据提供的编程学习材料生成代码风格的记忆卡片。

请按照以下格式生成卡片，每张卡片用JSON格式表示：

{
  "Front": "编程问题、概念或语法要点",
  "Back": "详细的解释和说明",
  "Code": "相关的代码示例（如果适用）",
  "Tags": ["编程语言", "主题", "难度等标签"]
}

要求：
1. 问题要具体明确，针对实际编程场景
2. 解释要准确完整，包含关键概念和注意事项
3. 代码示例要简洁实用，可以直接运行
4. 代码格式规范，包含必要的注释
5. 适合程序员日常学习和面试复习
6. 重点关注实用性和可操作性

代码示例格式要求：
- 使用标准的编程语言语法
- 包含输入输出示例（如果适用）
- 添加简要注释说明关键步骤

请根据内容的复杂度和知识点数量，生成适量的高质量编程记忆卡片。如果内容包含多个重要概念或代码示例，应该为每个知识点生成对应的卡片。`,
    front_template: `
<div class="card code-card">
  <div class="question">{{Front}}</div>
  
  <div class="hint">// 点击查看解决方案</div>
</div>
    `,
    back_template: `
<div class="card code-card">
  <div class="question">{{Front}}</div>
  
  <div class="hint">// 点击查看解决方案</div>
  
  <div class="answer">
    {{#Code}}
    <pre><code>{{Code}}</code></pre>
    {{/Code}}
    <div class="explanation">{{Back}}</div>
  </div>
</div>
    `,
    css_style: `
.code-card {
  font-family: 'Fira Code', 'Consolas', monospace;
  width: 100%;
  max-width: 100%;
  margin: 0;
  padding: 20px;
  background: #2d3748;
  color: #cbd5e0;
  border-radius: 8px;
  box-shadow: 0 10px 25px rgba(0,0,0,0.3);
  box-sizing: border-box;
  overflow: hidden;
}

.question {
  font-size: 16px;
  line-height: 1.5;
  color: #81e6d9;
  margin-bottom: 15px;
  word-wrap: break-word;
}

.hint {
  text-align: center;
  color: #718096;
  font-style: italic;
  margin-bottom: 15px;
  font-size: 12px;
}

pre {
  background: #1a202c;
  padding: 15px;
  border-radius: 6px;
  overflow-x: auto;
  border-left: 3px solid #63b3ed;
  font-size: 12px;
  line-height: 1.4;
  word-wrap: break-word;
  white-space: pre-wrap;
}

code {
  color: #feb2b2;
  word-wrap: break-word;
}

.explanation {
  margin-top: 15px;
  padding: 12px;
  background: #4a5568;
  border-radius: 6px;
  font-size: 14px;
  line-height: 1.6;
  word-wrap: break-word;
}
    `
  },
  {
    id: 'cloze-card',
    name: '填空题卡片',
    description: '专为填空记忆设计，支持多个空格和提示信息',
    preview_front: '牛顿第二定律的公式是 F = [...]，其中m表示[...]，a表示[...]',
    preview_back: '牛顿第二定律的公式是 F = ma，其中m表示质量，a表示加速度',
    note_type: 'Cloze',
    fields: ['Text', 'Hint', 'Source', 'Tags'],
    generation_prompt: `你是一个专业的ANKI填空题卡片制作专家。请根据提供的学习材料生成填空题风格的记忆卡片。

重要规则：为了在【一张卡片】上实现【多个挖空】，请务必将所有挖空标记的数字都设置为【1】。例如：{{c1::答案一}}...{{c1::答案二}}。

请按照以下格式生成卡片，每张卡片用JSON格式表示：

{
  "Text": "包含填空标记的完整文本。所有挖空都必须使用 {{c1::答案}} 的格式。",
  "Hint": "填空提示信息（可选）",
  "Source": "知识来源或参考资料（可选）",
  "Tags": ["相关标签"]
}

填空标记规则：
1. 【最重要】对于一张卡片上的所有挖空，【必须】全部使用 {{c1::...}} 格式，不要使用c2, c3...。这会确保所有挖空都在同一张卡片上。
2. 可以添加提示：{{c1::答案::提示}}
3. 确保填空内容是关键知识点

要求：
1. 选择文本中的关键概念、公式、定义作为填空内容
2. 填空应该有一定难度，能够测试理解程度
3. 文本要完整连贯，去掉填空后仍然语法正确
4. 适合主动回忆训练

请根据内容的复杂度和知识点数量，生成适量的高质量填空题记忆卡片。如果内容包含多个重要概念，应该为每个概念生成对应的卡片。`,
    front_template: `
<div class="card cloze-card">
  <div class="cloze-text">{{cloze:Text}}</div>
  
  {{#Hint}}
  <div class="hint-section">
    <div class="hint-label">💡 提示：</div>
    <div class="hint-content">{{Hint}}</div>
  </div>
  {{/Hint}}
</div>
    `,
    back_template: `
<div class="card cloze-card">
  <div class="cloze-text">{{cloze:Text}}</div>
  
  {{#Hint}}
  <div class="hint-section">
    <div class="hint-label">💡 提示：</div>
    <div class="hint-content">{{Hint}}</div>
  </div>
  {{/Hint}}
  
  <div class="complete-text">
    <div class="complete-label">完整内容：</div>
    <div class="complete-content">{{text:Text}}</div>
  </div>
  
  {{#Source}}
  <div class="source-section">
    <span class="source-label">📚 来源：</span>
    <span class="source-content">{{Source}}</span>
  </div>
  {{/Source}}
</div>
    `,
    css_style: `
.cloze-card {
  font-family: 'PingFang SC', 'Microsoft YaHei', sans-serif;
  width: 100%;
  max-width: 100%;
  margin: 0;
  padding: 24px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  border-radius: 12px;
  box-shadow: 0 8px 32px rgba(102, 126, 234, 0.3);
  box-sizing: border-box;
  overflow: hidden;
  position: relative;
}

.cloze-card::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: rgba(255, 255, 255, 0.1);
  backdrop-filter: blur(10px);
  border-radius: 12px;
  z-index: -1;
}

.cloze-text {
  font-size: 18px;
  line-height: 1.6;
  margin-bottom: 20px;
  text-align: justify;
  word-wrap: break-word;
}

.cloze {
  background: #FFD700;
  color: #2c3e50;
  padding: 2px 8px;
  border-radius: 6px;
  font-weight: 600;
  box-shadow: 0 2px 4px rgba(0,0,0,0.2);
}

.hint-section {
  background: rgba(255, 255, 255, 0.2);
  padding: 12px;
  border-radius: 8px;
  margin: 15px 0;
  border-left: 4px solid #FFD700;
}

.hint-label {
  font-weight: 600;
  font-size: 14px;
  margin-bottom: 5px;
  color: #FFD700;
}

.hint-content {
  font-size: 14px;
  line-height: 1.4;
  opacity: 0.9;
}

.complete-text {
  background: rgba(255, 255, 255, 0.15);
  padding: 15px;
  border-radius: 8px;
  margin-top: 20px;
  border: 1px solid rgba(255, 255, 255, 0.3);
}

.complete-label {
  font-weight: 600;
  font-size: 14px;
  margin-bottom: 8px;
  color: #E8F4FD;
}

.complete-content {
  font-size: 16px;
  line-height: 1.5;
  color: #F8F9FA;
}

.source-section {
  margin-top: 15px;
  padding-top: 10px;
  border-top: 1px solid rgba(255, 255, 255, 0.3);
  font-size: 12px;
  opacity: 0.8;
}

.source-label {
  font-weight: 600;
}
    `
  },
  {
    id: 'choice-card',
    name: '选择题卡片',
    description: '交互式选择题设计，支持多选项和解析说明',
    preview_front: '下列哪个是牛顿第一定律的内容？',
    preview_back: 'A. F=ma\nB. 作用力与反作用力\nC. 惯性定律 ✓\nD. 万有引力定律',
    note_type: 'Basic',
    fields: ['Front', 'Back', 'optiona', 'optionb', 'optionc', 'optiond', 'correct', 'explanation', 'Tags'],
    generation_prompt: `你是一个专业的ANKI选择题卡片制作专家。请根据提供的学习材料生成选择题风格的记忆卡片。

请按照以下格式生成卡片，每张卡片用JSON格式表示：

{
  "Front": "选择题题目",
  "optiona": "选项A的内容",
  "optionb": "选项B的内容", 
  "optionc": "选项C的内容",
  "optiond": "选项D的内容",
  "correct": "A",
  "explanation": "答案解析和详细说明",
  "tags": ["相关标签"]
}

要求：
1. 题目要清晰明确，针对核心知识点
2. 四个选项要有合理的干扰性，避免过于明显的错误选项
3. 正确答案用A、B、C、D中的一个字母表示
4. 解析要详细说明为什么这个选项正确，其他选项错在哪里
5. 选项长度尽量相近，避免明显的长短差异暴露答案
6. 适合检验理解程度和辨析能力

请根据内容的复杂度和知识点数量，生成适量的高质量选择题记忆卡片。特别注意：如果提供的内容本身就是选择题格式，请确保为每一道原有选择题都生成对应的卡片。`,
    front_template: `
<div class="card choice-card">
  <div class="question-section">
    <div class="question-label">📝 题目</div>
    <div class="question-text">{{Front}}</div>
  </div>
  
  <div class="options-section">
    <div class="option">
      <span class="option-label">A</span>
      <span class="option-text">{{optiona}}</span>
    </div>
    <div class="option">
      <span class="option-label">B</span>
      <span class="option-text">{{optionb}}</span>
    </div>
    <div class="option">
      <span class="option-label">C</span>
      <span class="option-text">{{optionc}}</span>
    </div>
    <div class="option">
      <span class="option-label">D</span>
      <span class="option-text">{{optiond}}</span>
    </div>
  </div>
  
  <div class="instruction">点击查看答案和解析</div>
</div>
    `,
    back_template: `
<div class="card choice-card">
  <div class="question-section">
    <div class="question-label">📝 题目</div>
    <div class="question-text">{{Front}}</div>
  </div>
  
  <div class="options-section answered">
    <div class="option">
      <span class="option-label">A</span>
      <span class="option-text">{{optiona}}</span>
    </div>
    <div class="option">
      <span class="option-label">B</span>
      <span class="option-text">{{optionb}}</span>
    </div>
    <div class="option">
      <span class="option-label">C</span>
      <span class="option-text">{{optionc}}</span>
    </div>
    <div class="option">
      <span class="option-label">D</span>
      <span class="option-text">{{optiond}}</span>
    </div>
  </div>
  
  <div class="answer-section">
    <div class="answer-label">✅ 正确答案：{{correct}}</div>
  </div>
  
  {{#explanation}}
  <div class="explanation-section">
    <div class="explanation-label">💡 解析</div>
    <div class="explanation-text">{{explanation}}</div>
  </div>
  {{/explanation}}
</div>
    `,
    css_style: `
.choice-card {
  font-family: 'PingFang SC', 'Microsoft YaHei', sans-serif;
  width: 100%;
  max-width: 100%;
  margin: 0;
  padding: 20px;
  background: #f8fafc;
  border: 2px solid #e2e8f0;
  border-radius: 12px;
  box-shadow: 0 4px 20px rgba(0,0,0,0.08);
  box-sizing: border-box;
  overflow: hidden;
}

.question-section {
  margin-bottom: 20px;
}

.question-label {
  font-size: 14px;
  font-weight: 600;
  color: #3b82f6;
  margin-bottom: 8px;
}

.question-text {
  font-size: 18px;
  font-weight: 500;
  color: #1e293b;
  line-height: 1.6;
  word-wrap: break-word;
}

.options-section {
  margin-bottom: 20px;
}

.option {
  display: flex;
  align-items: center;
  padding: 12px;
  margin: 8px 0;
  background: white;
  border: 1px solid #e2e8f0;
  border-radius: 8px;
  transition: all 0.2s ease;
  position: relative;
}

.option:hover {
  border-color: #3b82f6;
  box-shadow: 0 2px 8px rgba(59, 130, 246, 0.1);
}

.option-label {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  background: #f1f5f9;
  color: #475569;
  border-radius: 50%;
  font-weight: 600;
  font-size: 14px;
  margin-right: 12px;
  flex-shrink: 0;
}

.option-text {
  flex: 1;
  font-size: 16px;
  color: #334155;
  line-height: 1.5;
  word-wrap: break-word;
}

.option.correct {
  background: #f0f9ff;
  border-color: #22c55e;
}

.option.correct .option-label {
  background: #22c55e;
  color: white;
}

.correct-mark {
  color: #22c55e;
  font-weight: bold;
  font-size: 18px;
  margin-left: 8px;
}

.answer-section {
  background: #dcfce7;
  border: 1px solid #22c55e;
  border-radius: 8px;
  padding: 12px;
  margin: 15px 0;
}

.answer-label {
  font-weight: 600;
  color: #15803d;
  font-size: 16px;
}

.explanation-section {
  background: #fffbeb;
  border: 1px solid #f59e0b;
  border-radius: 8px;
  padding: 15px;
  margin-top: 15px;
}

.explanation-label {
  font-weight: 600;
  color: #d97706;
  margin-bottom: 8px;
  font-size: 14px;
}

.explanation-text {
  color: #92400e;
  line-height: 1.6;
  font-size: 14px;
  word-wrap: break-word;
}

.instruction {
  text-align: center;
  color: #64748b;
  font-style: italic;
  font-size: 14px;
  padding: 10px;
  background: #f1f5f9;
  border-radius: 6px;
}
    `
  }
];

// 统一的模板数据 - 将预置模板转换为CustomAnkiTemplate格式
export const BUILT_IN_TEMPLATES: CustomAnkiTemplate[] = LEGACY_ANKI_CARD_TEMPLATES.map(createBuiltInTemplate);

// 兼容性导出
export const ANKI_CARD_TEMPLATES = LEGACY_ANKI_CARD_TEMPLATES;

// 统一模板管理类
export class TemplateManager {
  private customTemplates: CustomAnkiTemplate[] = [];
  private listeners: Array<(templates: CustomAnkiTemplate[]) => void> = [];
  private userDefaultTemplateId: string | null = null;

  constructor() {
    this.loadTemplates();
  }

  // 加载所有模板（内置 + 自定义）
  async loadTemplates(): Promise<void> {
    try {
      // 动态导入Tauri API（避免SSR问题）
      const { invoke } = await import('@tauri-apps/api/core');
      
      // 并行加载自定义模板和默认模板设置
      const [userTemplates, defaultTemplateId] = await Promise.all([
        invoke<CustomAnkiTemplate[]>('get_all_custom_templates'),
        invoke<string | null>('get_default_template_id').catch(() => null)
      ]);
      
      // 合并内置模板和用户自定义模板
      this.customTemplates = [...BUILT_IN_TEMPLATES, ...userTemplates];
      this.userDefaultTemplateId = defaultTemplateId;
      this.notifyListeners();
    } catch (error) {
      console.warn('Failed to load custom templates, using built-in only:', error);
      this.customTemplates = [...BUILT_IN_TEMPLATES];
      this.userDefaultTemplateId = null;
      this.notifyListeners();
    }
  }

  // 获取所有模板
  getAllTemplates(): CustomAnkiTemplate[] {
    return this.customTemplates;
  }

  // 获取活跃模板
  getActiveTemplates(): CustomAnkiTemplate[] {
    return this.customTemplates.filter(t => t.is_active);
  }

  // 根据ID获取模板
  getTemplateById(id: string): CustomAnkiTemplate | undefined {
    return this.customTemplates.find(template => template.id === id);
  }

  // 获取默认模板
  getDefaultTemplate(): CustomAnkiTemplate {
    // 如果用户设置了默认模板，优先使用用户设置的
    if (this.userDefaultTemplateId) {
      const userDefault = this.customTemplates.find(t => t.id === this.userDefaultTemplateId);
      if (userDefault) {
        return userDefault;
      }
    }
    // 否则返回列表中的第一个模板
    return this.customTemplates[0] || BUILT_IN_TEMPLATES[0];
  }

  // 将CustomAnkiTemplate转换为AnkiCardTemplate（向后兼容）
  toAnkiCardTemplate(template: CustomAnkiTemplate): AnkiCardTemplate {
    return {
      id: template.id,
      name: template.name,
      description: template.description,
      preview_front: template.preview_front,
      preview_back: template.preview_back,
      front_template: template.front_template,
      back_template: template.back_template,
      css_style: template.css_style,
      note_type: template.note_type,
      generation_prompt: template.generation_prompt,
      fields: template.fields
    };
  }

  // 订阅模板变化
  subscribe(listener: (templates: CustomAnkiTemplate[]) => void): () => void {
    this.listeners.push(listener);
    return () => {
      const index = this.listeners.indexOf(listener);
      if (index > -1) {
        this.listeners.splice(index, 1);
      }
    };
  }

  private notifyListeners(): void {
    this.listeners.forEach(listener => listener(this.customTemplates));
  }

  // 刷新模板列表
  async refresh(): Promise<void> {
    await this.loadTemplates();
  }

  // 创建新模板
  async createTemplate(templateData: any): Promise<string> {
    const { invoke } = await import('@tauri-apps/api/core');
    const templateId = await invoke<string>('create_custom_template', { request: templateData });
    await this.loadTemplates(); // 重新加载
    return templateId;
  }

  // 删除模板
  async deleteTemplate(templateId: string): Promise<void> {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('delete_custom_template', { templateId });
    await this.loadTemplates(); // 重新加载
  }

  // 加载用户默认模板设置
  async loadUserDefaultTemplate(): Promise<void> {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      this.userDefaultTemplateId = await invoke<string | null>('get_default_template_id');
    } catch (error) {
      console.warn('Failed to load user default template:', error);
      this.userDefaultTemplateId = null;
    }
  }

  // 设置默认模板
  async setDefaultTemplate(templateId: string): Promise<void> {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('set_default_template', { templateId });
      this.userDefaultTemplateId = templateId;
      this.notifyListeners(); // 通知UI更新
    } catch (error) {
      console.error('Failed to set default template:', error);
      throw error;
    }
  }

  // 获取当前默认模板ID
  getDefaultTemplateId(): string | null {
    return this.userDefaultTemplateId;
  }

  // 检查模板是否为默认模板
  isDefaultTemplate(templateId: string): boolean {
    return this.userDefaultTemplateId === templateId;
  }
}

// 全局模板管理器实例
export const templateManager = new TemplateManager();

// 兼容性函数
export const getTemplateById = (id: string): AnkiCardTemplate | undefined => {
  const template = templateManager.getTemplateById(id);
  return template ? templateManager.toAnkiCardTemplate(template) : undefined;
};

export const getDefaultTemplate = (): AnkiCardTemplate => {
  return templateManager.toAnkiCardTemplate(templateManager.getDefaultTemplate());
};

export const getTemplatePrompt = (templateId: string): string => {
  const template = templateManager.getTemplateById(templateId);
  return template?.generation_prompt || templateManager.getDefaultTemplate().generation_prompt;
};

export const getTemplateFields = (templateId: string): string[] => {
  const template = templateManager.getTemplateById(templateId);
  return template?.fields || ['Front', 'Back'];
};
