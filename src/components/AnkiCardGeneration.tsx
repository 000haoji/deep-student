import React, { useState, useRef, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { 
  AnkiCard, 
  AnkiGenerationOptions,
  AnkiCardTemplate,
  CustomAnkiTemplate,
  CreateTemplateRequest,
  FieldExtractionRule,
  FieldType
} from '../types';
import { ANKI_CARD_TEMPLATES, getDefaultTemplate, getTemplatePrompt, getTemplateFields, templateManager } from '../data/ankiTemplates';
import TemplateManager from './TemplateManager';
import { useSystemSettings } from '../hooks/useSystemSettings';
import './UnifiedTemplateSelector.css';
import { IframePreview, renderCardPreview as renderTemplatePreview } from './SharedPreview';
import { 
  BookOpen, AlertTriangle, CheckCircle, RefreshCcw, X, 
  Upload, Lightbulb, FileText, Clock, Settings
} from 'lucide-react';

interface AnkiCardGenerationProps {
  onTemplateSelectionRequest?: (callback: (template: any) => void) => void;
}

const AnkiCardGeneration: React.FC<AnkiCardGenerationProps> = ({ onTemplateSelectionRequest }) => {
  const [documentContent, setDocumentContent] = useState('');
  const [generatedCards, setGeneratedCards] = useState<AnkiCard[]>([]);
  const [selectedCards, setSelectedCards] = useState<Set<number>>(new Set());
  const [isGenerating, setIsGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [options, setOptions] = useState<AnkiGenerationOptions>({
    deck_name: 'AI学习卡片',
    note_type: 'Basic',
    enable_images: false,
    max_cards_per_mistake: 30,
    max_tokens: 8192,
    temperature: 0.3,
    template_id: getDefaultTemplate().id,
    segment_overlap_size: 200
  });

  const [selectedTemplate, setSelectedTemplate] = useState<AnkiCardTemplate>(getDefaultTemplate());
  const [showTemplatePreview, setShowTemplatePreview] = useState(false);
  const [currentDocumentId, setCurrentDocumentId] = useState<string | null>(null);
  const [documentTasks, setDocumentTasks] = useState<any[]>([]);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [isAnkiConnectAvailable, setIsAnkiConnectAvailable] = useState<boolean | null>(null);
  const [ankiDeckNames, setAnkiDeckNames] = useState<string[]>([]);
  const [ankiModelNames, setAnkiModelNames] = useState<string[]>([]);
  const [isAddingToAnki, setIsAddingToAnki] = useState(false);
  const [ankiConnectionError, setAnkiConnectionError] = useState<string | null>(null);
  const [isExportingApkg, setIsExportingApkg] = useState(false);
  const [selectedFiles, setSelectedFiles] = useState<File[]>([]);
  const [isDragOver, setIsDragOver] = useState(false);
  const [isProcessingFiles, setIsProcessingFiles] = useState(false);
  const [allTemplates, setAllTemplates] = useState<CustomAnkiTemplate[]>([]);
  const [showTemplateManager, setShowTemplateManager] = useState(false);
  const [isLoadingTemplates, setIsLoadingTemplates] = useState(false);
  const [customRequirements, setCustomRequirements] = useState('');
  const [isAnkiConnectEnabled, setIsAnkiConnectEnabled] = useState(false);
  const [systemPrompt, setSystemPrompt] = useState('你是一个专业的ANKI学习卡片制作助手。请根据提供的学习内容，生成高质量的ANKI学习卡片。\n\n要求：\n1. 卡片应该有助于记忆和理解\n2. 问题要简洁明确\n3. 答案要准确完整\n4. 适当添加相关标签\n5. 确保卡片的逻辑性和实用性');
  const [showPromptConfig, setShowPromptConfig] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const loadAllTemplates = async () => {
    setIsLoadingTemplates(true);
    try {
      await templateManager.refresh();
      setAllTemplates(templateManager.getAllTemplates());
    } catch (error) {
      console.error('加载模板失败:', error);
      setError(`加载模板失败: ${error}`);
    } finally {
      setIsLoadingTemplates(false);
    }
  };

  const updateToDefaultTemplate = () => {
    try {
      const defaultTemplate = templateManager.getDefaultTemplate();
      if (defaultTemplate && defaultTemplate.id !== selectedTemplate.id) {
        const ankiTemplate = templateManager.toAnkiCardTemplate(defaultTemplate);
        setSelectedTemplate(ankiTemplate);
        setOptions(prev => ({
          ...prev,
          template_id: defaultTemplate.id,
          note_type: defaultTemplate.note_type
        }));
      }
    } catch (error) {
      console.error('更新默认模板失败:', error);
    }
  };

  const handleTemplateSelect = (template: CustomAnkiTemplate) => {
    const ankiTemplate = templateManager.toAnkiCardTemplate(template);
    setSelectedTemplate(ankiTemplate);
    setOptions({
      ...options,
      template_id: template.id,
      note_type: template.note_type
    });
  };

  const handleTemplateChange = (templateId: string) => {
    const template = ANKI_CARD_TEMPLATES.find(t => t.id === templateId);
    if (template) {
      setSelectedTemplate(template);
      setOptions({
        ...options,
        template_id: templateId,
        note_type: template.note_type
      });
    }
  };

  const renderClozeText = (text: string) => {
    if (!text) return '';
    return text.replace(/\{\{c(\d+)::([^}]+?)\}\}/g, 
      '<span class="cloze-highlight">$2</span>'
    );
  };

  const getTaskCards = () => {
    if (!selectedTaskId) return generatedCards;
    const task = documentTasks.find(t => t.task_id === selectedTaskId);
    return task?.cards || [];
  };

  const getCurrentDisplayCards = () => {
    if (selectedTaskId) {
      return getTaskCards();
    }
    return generatedCards;
  };

  const getCardGlobalIndex = (localIndex: number) => {
    if (!selectedTaskId) return localIndex;
    const taskCards = getTaskCards();
    const targetCard = taskCards[localIndex];
    if (!targetCard) return -1;
    return generatedCards.findIndex(card => 
      card.front === targetCard.front && 
      card.back === targetCard.back
    );
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(true);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    const files = Array.from(e.dataTransfer.files);
    const supportedFiles = files.filter(file => 
      file.type === 'application/pdf' || 
      file.name.endsWith('.pdf') || 
      file.name.endsWith('.docx') ||
      file.name.endsWith('.txt') ||
      file.name.endsWith('.md')
    );
    if (supportedFiles.length > 0) {
      setSelectedFiles(prev => [...prev, ...supportedFiles]);
    } else {
      setError('请选择支持的文件格式（PDF、DOCX、TXT、MD）');
    }
  };

  const processSelectedFile = async (file: File) => {
    setIsProcessingFiles(true);
    setError(null);
    try {
      const fileName = file.name.toLowerCase();
      if (fileName.endsWith('.docx') || fileName.endsWith('.pdf')) {
        const reader = new FileReader();
        reader.onload = async (e) => {
          try {
            const dataUrl = e.target?.result as string;
            // 从data URL中提取base64内容（移除 "data:application/pdf;base64," 等前缀）
            const base64Content = dataUrl.split(',')[1];
            const { invoke } = await import('@tauri-apps/api/core');
            const extractedText = await invoke('parse_document_from_base64', {
              fileName: file.name,
              base64Content: base64Content
            });
            setDocumentContent(extractedText as string);
          } catch (error) {
            setError(`文档解析失败: ${error}`);
          } finally {
            setIsProcessingFiles(false);
          }
        };
        reader.readAsDataURL(file);
      } else {
        const reader = new FileReader();
        reader.onload = (e) => {
          const content = e.target?.result as string;
          setDocumentContent(content);
          setIsProcessingFiles(false);
        };
        reader.readAsText(file, 'UTF-8');
      }
    } catch (error) {
      setError(`文件处理失败: ${error}`);
      setIsProcessingFiles(false);
    }
  };

  const generateCards = async () => {
    if (!documentContent) {
      setError('请输入文档内容');
      return;
    }
    setIsGenerating(true);
    setError(null);
    setCurrentDocumentId(null);
    setDocumentTasks([]);
    setGeneratedCards([]);
    setSelectedCards(new Set());
    try {
      const templatePrompt = getTemplatePrompt(options.template_id || getDefaultTemplate().id);
      const templateFields = getTemplateFields(options.template_id || getDefaultTemplate().id);
      const currentTemplate = allTemplates.find(t => t.id === (options.template_id || getDefaultTemplate().id));
      const fieldExtractionRules = currentTemplate?.field_extraction_rules || {};
      await invoke('start_enhanced_document_processing', {
        documentContent: documentContent,
        originalDocumentName: `AI学习文档_${new Date().toISOString().slice(0, 10)}`,
        options: {
          ...options,
          max_output_tokens_override: options.max_tokens,
          temperature_override: options.temperature,
          template_id: selectedTemplate?.id || null,
          custom_anki_prompt: templatePrompt,
          template_fields: templateFields,
          field_extraction_rules: fieldExtractionRules,
          custom_requirements: customRequirements.trim() || null,
          system_prompt: systemPrompt.trim() || null
        }
      });
    } catch (err) {
      setError(`生成失败: ${err}`);
      setIsGenerating(false);
    }
  };

  const handleAnkiStreamEvent = (received_payload: any) => {
    let actual_payload = received_payload;
    if (received_payload && typeof received_payload === 'object' && Object.prototype.hasOwnProperty.call(received_payload, 'payload')) {
      actual_payload = received_payload.payload;
    }
    if (!actual_payload || !actual_payload.type) {
      return;
    }
    switch (actual_payload.type) {
      case 'DocumentProcessingStarted':
        if (actual_payload.data && actual_payload.data.document_id) {
          setCurrentDocumentId(actual_payload.data.document_id);
          const initialTasks = Array.from({ length: actual_payload.data.total_segments || 0 }, (_, i) => ({
            task_id: `temp_task_${actual_payload.data.document_id}_${i}`,
            segment_index: i,
            status: 'pending',
            progress: 0,
            cards: [],
            content_preview: `片段 ${i + 1} (等待处理中...)`,
            error_message: null,
          }));
          setDocumentTasks(initialTasks);
          setGeneratedCards([]);
          setSelectedCards(new Set());
        }
        break;
      case 'TaskStatusUpdate':
        if (actual_payload.data && actual_payload.data.task_id) {
          setDocumentTasks(prevTasks => {
            let taskUpdated = false;
            const updatedTasks = prevTasks.map(t => {
              if (t.task_id === actual_payload.data.task_id) {
                taskUpdated = true;
                return { ...t, status: actual_payload.data.status?.toLowerCase() || 'pending', error_message: actual_payload.data.message || t.error_message };
              }
              return t;
            });
            if (!taskUpdated && typeof actual_payload.data.segment_index === 'number') {
                return prevTasks.map(t => {
                    if (t.segment_index === actual_payload.data.segment_index && t.task_id.startsWith('temp_task_')) {
                        return { ...t, task_id: actual_payload.data.task_id, status: actual_payload.data.status?.toLowerCase() || 'pending', error_message: actual_payload.data.message || t.error_message };
                    }
                    return t;
                });
            }
            return updatedTasks;
          });
        }
        break;
      case 'NewCard':
      case 'NewErrorCard':
        if (actual_payload.data && actual_payload.data.task_id) {
          const isError = actual_payload.type === 'NewErrorCard';
          let newCardEntry = { ...actual_payload.data, is_error_card: isError };
          const isDuplicateCard = (existingCard: AnkiCard, newCard: AnkiCard) => {
            if (selectedTemplate.note_type === 'Cloze') {
              const existingText = existingCard.text || existingCard.front || '';
              const newText = newCard.text || newCard.front || '';
              return existingText === newText && existingText.length > 0;
            }
            return existingCard.front === newCard.front && existingCard.back === newCard.back;
          };
          if (selectedTemplate.note_type === 'Cloze' && !newCardEntry.text && newCardEntry.front) {
            newCardEntry = { ...newCardEntry, text: newCardEntry.front, front: '' };
          }
          setDocumentTasks(prev => {
            let cardAddedToTask = false;
            const newTasks = prev.map(task => {
              if (task.task_id === newCardEntry.task_id) {
                cardAddedToTask = true;
                const isDuplicate = task.cards.some((existingCard: AnkiCard) => isDuplicateCard(existingCard, newCardEntry));
                if (isDuplicate) return task;
                return { ...task, cards: [...task.cards, newCardEntry] };
              }
              return task;
            });
            return newTasks;
          });
          setGeneratedCards(prev => {
            const isDuplicate = prev.some(existingCard => isDuplicateCard(existingCard, newCardEntry));
            if (isDuplicate) return prev;
            return [...prev, newCardEntry];
          });
        }
        break;
      case 'TaskCompleted':
        if (actual_payload.data && actual_payload.data.task_id) {
          setDocumentTasks(prev =>
            prev.map(task => {
              if (task.task_id === actual_payload.data.task_id) {
                return { ...task, status: (actual_payload.data.final_status || 'Completed').toLowerCase(), progress: 100 };
              }
              return task;
            })
          );
        }
        break;
      case 'DocumentProcessingCompleted':
        if (actual_payload.data && actual_payload.data.document_id) {
          setIsGenerating(false);
          if (selectedTemplate.note_type === 'Cloze') {
            setGeneratedCards(prev => prev.map(card => (!card.text && card.front) ? { ...card, text: card.front, front: '' } : card));
            setDocumentTasks(prev => prev.map(task => ({
              ...task,
              cards: task.cards.map((card: any) => (!card.text && card.front) ? { ...card, text: card.front, front: '' } : card)
            })));
          }
        }
        break;
      case 'TaskProcessingError':
        if (actual_payload.data) {
          const errorMsg = actual_payload.data.error_message || '处理过程中发生未知错误';
          const taskId = actual_payload.data.task_id;
          const detailedError = taskId ? `任务 ${taskId.substring(0, 8)}... 发生错误: ${errorMsg}` : `处理错误: ${errorMsg}`;
          setError(prevError => prevError ? `${prevError}\n${detailedError}` : detailedError);
          if (taskId) {
            setDocumentTasks(prev => prev.map(task => task.task_id === taskId ? { ...task, status: 'failed', error_message: errorMsg } : task));
          }
        }
        break;
      default:
        break;
    }
  };

  const toggleCardSelection = (index: number) => {
    const newSelected = new Set(selectedCards);
    if (newSelected.has(index)) {
      newSelected.delete(index);
    } else {
      newSelected.add(index);
    }
    setSelectedCards(newSelected);
  };

  const toggleSelectAll = () => {
    const currentCards = getCurrentDisplayCards();
    const currentGlobalIndices = currentCards.map((_: any, localIndex: number) => getCardGlobalIndex(localIndex));
    const allCurrentSelected = currentGlobalIndices.every((globalIndex: number) => selectedCards.has(globalIndex));
    const newSelected = new Set(selectedCards);
    if (allCurrentSelected) {
      currentGlobalIndices.forEach((globalIndex: number) => newSelected.delete(globalIndex));
    } else {
      currentGlobalIndices.forEach((globalIndex: number) => newSelected.add(globalIndex));
    }
    setSelectedCards(newSelected);
  };

  const editCard = (index: number, field: 'front' | 'back' | 'text', value: string) => {
    const newCards = [...generatedCards];
    newCards[index] = { ...newCards[index], [field]: value };
    setGeneratedCards(newCards);
    setDocumentTasks(prev => 
      prev.map(task => ({
        ...task,
        cards: task.cards.map((card: any) => 
          card.front === generatedCards[index].front && card.back === generatedCards[index].back
            ? { ...card, [field]: value }
            : card
        )
      }))
    );
  };

  const deleteCard = (index: number) => {
    const cardToDelete = generatedCards[index];
    if (!cardToDelete) return;
    const newCards = generatedCards.filter((_, i) => i !== index);
    setGeneratedCards(newCards);
    setDocumentTasks(prev => 
      prev.map(task => ({
        ...task,
        cards: task.cards.filter((card: any) => 
          !(card.front === cardToDelete.front && card.back === cardToDelete.back)
        )
      }))
    );
    const newSelected = new Set<number>();
    selectedCards.forEach(selectedIndex => {
      if (selectedIndex < index) {
        newSelected.add(selectedIndex);
      } else if (selectedIndex > index) {
        newSelected.add(selectedIndex - 1);
      }
    });
    setSelectedCards(newSelected);
  };

  const checkAnkiConnectStatus = async () => {
    try {
      console.log('获取Anki牌组和模型列表...');
      
      try {
        const deckNames = await invoke('anki_get_deck_names') as string[];
        const modelNames = await invoke('anki_get_model_names') as string[];
        
        console.log('获取到牌组:', deckNames);
        console.log('获取到模型:', modelNames);
        
        setAnkiDeckNames(deckNames);
        setAnkiModelNames(modelNames);
        setIsAnkiConnectAvailable(true);
        setAnkiConnectionError(null);
        
        return true;
      } catch (err) {
        console.error('AnkiConnect连接失败:', err);
        setIsAnkiConnectAvailable(false);
        setAnkiConnectionError(err as string);
        return false;
      }
    } catch (err) {
      console.error('AnkiConnect连接失败:', err);
      setIsAnkiConnectAvailable(false);
      const errorMessage = err instanceof Error ? err.message : 
                          (typeof err === 'object' ? JSON.stringify(err) : String(err));
      setAnkiConnectionError(errorMessage);
      setAnkiDeckNames([]);
      setAnkiModelNames([]);
    }
  };

  const handleAddToAnkiConnect = async () => {
    if (selectedCards.size === 0) {
      setError('请选择要添加的卡片');
      return;
    }
    if (!options.deck_name.trim() || !options.note_type.trim()) {
      setError('请输入牌组名称和笔记类型');
      return;
    }
    setIsAddingToAnki(true);
    setError(null);
    setAnkiConnectionError(null);
    try {
      const selectedCardsList = Array.from(selectedCards).sort((a, b) => a - b).map(index => generatedCards[index]);
      const noteIds = await invoke<(number | null)[]>('add_cards_to_anki_connect', {
        selectedCards: selectedCardsList,
        deckName: options.deck_name,
        noteType: options.note_type
      });
      const successCount = noteIds.filter(id => id !== null).length;
      const failCount = noteIds.length - successCount;
      if (successCount > 0) {
        alert(failCount > 0 ? `成功添加 ${successCount} 张卡片，${failCount} 张失败` : `成功添加 ${successCount} 张卡片到Anki！`);
        setError(null);
      } else {
        setError('所有卡片添加失败，请检查卡片内容或Anki设置');
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 
                          (typeof err === 'object' ? JSON.stringify(err) : String(err));
      setAnkiConnectionError(errorMessage);
      setError(`添加失败: ${errorMessage}`);
    } finally {
      setIsAddingToAnki(false);
    }
  };

  const handleExportByLevel = async (level: 'document' | 'task' | 'selection') => {
    let cardsToExport: AnkiCard[] = [];
    let exportDescription = '';
    switch (level) {
      case 'document':
        cardsToExport = generatedCards;
        exportDescription = `文档 ${currentDocumentId?.substring(0, 8)}`;
        break;
      case 'task':
        cardsToExport = getTaskCards();
        exportDescription = `任务 ${selectedTaskId?.substring(0, 8)}`;
        break;
      case 'selection':
        cardsToExport = Array.from(selectedCards).sort((a, b) => a - b).map(index => generatedCards[index]);
        exportDescription = '选中卡片';
        break;
    }
    if (cardsToExport.length === 0 || !options.deck_name.trim() || !options.note_type.trim()) {
      setError('没有可导出的卡片或未指定牌组/笔记类型');
      return;
    }
    setIsExportingApkg(true);
    setError(null);
    try {
      const result = await invoke<string>('export_cards_as_apkg_with_template', {
        selectedCards: cardsToExport,
        deckName: options.deck_name,
        noteType: options.note_type,
        templateId: selectedTemplate?.id || null
      });
      alert(`导出成功！\n${exportDescription}: ${cardsToExport.length} 张卡片\n${result}`);
      setError(null);
    } catch (err) {
      setError(`导出失败: ${err}`);
    } finally {
      setIsExportingApkg(false);
    }
  };

  const loadAnkiConnectSetting = async () => {
    try {
      const enabled = await invoke<string>('get_setting', { key: 'anki_connect_enabled' });
      setIsAnkiConnectEnabled(enabled === 'true');
    } catch (error) {
      setIsAnkiConnectEnabled(false);
    }
  };

  useEffect(() => {
    loadAnkiConnectSetting();
    templateManager.loadTemplates().then(() => {
      setAllTemplates(templateManager.getAllTemplates());
      updateToDefaultTemplate();
    });
    const unsubscribe = templateManager.subscribe(() => {
      setAllTemplates(templateManager.getAllTemplates());
      updateToDefaultTemplate();
    });
    let unlistenFunction: (() => void) | null = null;
    listen('anki_generation_event', (event) => {
      handleAnkiStreamEvent(event.payload);
    }).then(unlisten => {
      unlistenFunction = unlisten;
    });
    return () => {
      unsubscribe();
      if (unlistenFunction) {
        unlistenFunction();
      }
    };
  }, []);

  useEffect(() => {
    if (isAnkiConnectEnabled) {
      const timer = setTimeout(() => checkAnkiConnectStatus(), 1000);
      return () => clearTimeout(timer);
    } else {
      setIsAnkiConnectAvailable(null);
      setAnkiConnectionError(null);
    }
  }, [isAnkiConnectEnabled]);

  useEffect(() => {
    const handleWindowFocus = () => loadAnkiConnectSetting();
    const handleSettingsChange = () => loadAnkiConnectSetting();
    
    window.addEventListener('focus', handleWindowFocus);
    window.addEventListener('systemSettingsChanged', handleSettingsChange);
    
    // 添加定期检查设置变化（作为备用机制）
    const settingsCheckInterval = setInterval(() => {
      loadAnkiConnectSetting();
    }, 5000); // 每5秒检查一次设置（频率降低）
    
    return () => {
      window.removeEventListener('focus', handleWindowFocus);
      window.removeEventListener('systemSettingsChanged', handleSettingsChange);
      clearInterval(settingsCheckInterval);
    };
  }, []);

  return (
    <div style={{ width: '100%', height: '100%', overflow: 'auto', background: '#f8fafc' }}>
      <div style={{ background: 'white', borderBottom: '1px solid #e5e7eb', padding: '24px 32px', position: 'relative' }}>
        <div style={{ position: 'absolute', top: 0, left: 0, right: 0, height: '4px', background: 'linear-gradient(90deg, #667eea, #764ba2)' }}></div>
        <div style={{ position: 'relative', zIndex: 1 }}>
          <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: '8px' }}>
            <div style={{ display: 'flex', alignItems: 'center' }}>
              <svg style={{ width: '32px', height: '32px', marginRight: '12px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
                <rect width="20" height="14" x="2" y="5" rx="2" />
                <line x1="2" x2="22" y1="10" y2="10" />
              </svg>
              <h1 style={{ fontSize: '28px', fontWeight: '700', margin: 0, color: '#1f2937' }}>ANKI制卡助手</h1>
            </div>
            <button 
              onClick={() => setShowPromptConfig(!showPromptConfig)}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
                padding: '8px 16px',
                backgroundColor: showPromptConfig ? '#667eea' : 'transparent',
                color: showPromptConfig ? 'white' : '#667eea',
                border: '2px solid #667eea',
                borderRadius: '8px',
                cursor: 'pointer',
                fontSize: '14px',
                fontWeight: '500',
                transition: 'all 0.2s ease'
              }}
            >
              <Settings size={16} />
              Prompt配置
            </button>
          </div>
          <p style={{ fontSize: '16px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
            将学习资料智能转换为ANKI记忆卡片，提升记忆效率
          </p>
        </div>
      </div>

      {showPromptConfig && (
        <div style={{ 
          background: 'white', 
          borderBottom: '1px solid #e5e7eb', 
          padding: '24px 32px',
          borderLeft: '4px solid #667eea'
        }}>
          <h3 style={{ 
            margin: '0 0 16px 0', 
            fontSize: '18px', 
            fontWeight: '600', 
            color: '#1f2937',
            display: 'flex',
            alignItems: 'center',
            gap: '8px'
          }}>
            <Settings size={20} />
            系统Prompt配置
          </h3>
          <div style={{ marginBottom: '16px' }}>
            <label style={{ 
              display: 'block', 
              marginBottom: '8px', 
              fontSize: '14px', 
              fontWeight: '500', 
              color: '#374151' 
            }}>
              自定义系统Prompt（留空使用默认设置）：
            </label>
            <textarea
              value={systemPrompt}
              onChange={(e) => setSystemPrompt(e.target.value)}
              placeholder="在此输入自定义的系统prompt，这将覆盖默认的制卡指令..."
              style={{
                width: '100%',
                minHeight: '120px',
                padding: '12px',
                border: '2px solid #e5e7eb',
                borderRadius: '8px',
                fontSize: '14px',
                lineHeight: '1.5',
                fontFamily: 'Monaco, Consolas, "Courier New", monospace',
                resize: 'vertical',
                outline: 'none',
                transition: 'border-color 0.2s ease'
              }}
              onFocus={(e) => e.target.style.borderColor = '#667eea'}
              onBlur={(e) => e.target.style.borderColor = '#e5e7eb'}
            />
            <div style={{ 
              marginTop: '8px', 
              fontSize: '12px', 
              color: '#6b7280',
              display: 'flex',
              alignItems: 'center',
              gap: '4px'
            }}>
              <svg style={{ width: '14px', height: '14px', color: '#f59e0b' }} fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
              </svg>
              修改Prompt会影响所有生成的卡片质量和格式，请谨慎设置
            </div>
          </div>
          <div style={{ display: 'flex', gap: '12px', alignItems: 'center' }}>
            <button
              onClick={() => {
                setSystemPrompt('你是一个专业的ANKI学习卡片制作助手。请根据提供的学习内容，生成高质量的ANKI学习卡片。\n\n要求：\n1. 卡片应该有助于记忆和理解\n2. 问题要简洁明确\n3. 答案要准确完整\n4. 适当添加相关标签\n5. 确保卡片的逻辑性和实用性');
              }}
              style={{
                padding: '8px 16px',
                backgroundColor: '#f3f4f6',
                color: '#374151',
                border: '1px solid #d1d5db',
                borderRadius: '6px',
                cursor: 'pointer',
                fontSize: '14px',
                fontWeight: '500'
              }}
            >
              恢复默认
            </button>
            <span style={{ fontSize: '12px', color: '#6b7280' }}>
              当前字符数: {systemPrompt.length}
            </span>
          </div>
        </div>
      )}

      <div className="anki-card-generation" style={{ padding: '24px', background: 'transparent' }}>
        <div className="input-section">
          <div className="form-group">
            <label>文档上传：</label>
            <div className={`upload-zone ${isDragOver ? 'drag-over' : ''} ${isProcessingFiles ? 'uploading' : ''}`} onDragOver={handleDragOver} onDragLeave={handleDragLeave} onDrop={handleDrop}>
              <div className="upload-content">
                <div className="upload-icon">
                  <svg style={{ width: '24px', height: '24px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                  </svg>
                </div>
                <div className="upload-text">{isProcessingFiles ? '处理中...' : '拖拽文件到此处或点击选择文件'}</div>
                <div className="upload-hint">支持格式: PDF, DOCX, TXT, MD</div>
                <input type="file" ref={fileInputRef} accept=".pdf,.docx,.txt,.md" onChange={(e) => setSelectedFiles(prev => [...prev, ...Array.from(e.target.files || [])])} style={{ display: 'none' }} id="anki-file-input" />
                <label htmlFor="anki-file-input" className="btn btn-primary">选择文件</label>
              </div>
              {selectedFiles.length > 0 && (
                <div className="selected-files">
                  <h4>选择的文件 ({selectedFiles.length})</h4>
                  <div className="file-list">
                    {selectedFiles.map((file, index) => (
                      <div key={index} className="file-item">
                        <span className="file-name">{file.name}</span>
                        <span className="file-size">{(file.size / 1024 / 1024).toFixed(2)} MB</span>
                        <button onClick={() => setSelectedFiles(prev => prev.filter((_, i) => i !== index))} className="btn-remove">
                          <svg style={{ width: '16px', height: '16px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                          </svg>
                        </button>
                      </div>
                    ))}
                  </div>
                  <div className="upload-actions">
                    <button onClick={() => { if (selectedFiles.length > 0) { processSelectedFile(selectedFiles[0]); setSelectedFiles([]); } }} disabled={isProcessingFiles || selectedFiles.length === 0} className="btn btn-success">{isProcessingFiles ? '处理中...' : '处理文件'}</button>
                    <button onClick={() => setSelectedFiles([])} disabled={isProcessingFiles} className="btn btn-secondary">清空</button>
                  </div>
                </div>
              )}
            </div>
          </div>
          <div className="form-group">
            <label>文档内容：</label>
            <textarea value={documentContent} onChange={(e) => setDocumentContent(e.target.value)} placeholder="请输入或上传学习文档内容..." rows={8} className="form-control" />
          </div>
          <div className="form-group">
            <label>制卡要求（可选）：</label>
            <textarea value={customRequirements} onChange={(e) => setCustomRequirements(e.target.value)} placeholder="请描述您的特殊制卡要求，例如：重点关注概念定义、增加实例说明、突出重点公式、按难度分层等..." rows={4} className="form-control" style={{ fontSize: '14px', lineHeight: '1.5', border: '2px dashed #e2e8f0', borderRadius: '8px', padding: '12px', backgroundColor: '#f8fafc' }} />
            <small className="form-help" style={{ color: '#64748b', fontSize: '12px', marginTop: '5px', display: 'flex', alignItems: 'center', gap: '4px' }}>
              <svg style={{ width: '16px', height: '16px', color: '#fbbf24' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
              </svg>
              提示：详细的要求有助于AI生成更符合您需求的卡片。例如："请重点制作概念定义类卡片，每个卡片包含实际应用场景"
            </small>
          </div>
          <div className="options-section">
            <h4>生成选项</h4>
            <div className="template-section">
              <h5>
                <svg style={{ width: '20px', height: '20px', marginRight: '8px', color: '#667eea', display: 'inline-block' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zM21 5a2 2 0 00-2-2h-4a2 2 0 00-2 2v12a4 4 0 004 4h4a2 2 0 002-2V5z" />
                </svg>
                卡片模板
              </h5>
              <div className="unified-template-selector">
                <div className="current-template-display">
                  <div className="current-template-card">
                    <div className="template-header">
                      <div className="template-info">
                        <h6 className="template-name">{selectedTemplate.name}</h6>
                        <p className="template-description">{selectedTemplate.description}</p>
                      </div>
                      <div className="template-actions">
                        <button type="button" onClick={() => setShowTemplatePreview(!showTemplatePreview)} className="btn btn-outline btn-sm">
                          {showTemplatePreview ? '隐藏预览' : (
                            <>
                              <svg style={{ width: '16px', height: '16px', marginRight: '4px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                              </svg>
                              预览
                            </>
                          )}
                        </button>
                        <button type="button" onClick={() => { if (onTemplateSelectionRequest) { onTemplateSelectionRequest((template: CustomAnkiTemplate) => handleTemplateSelect(template)); } else { setShowTemplateManager(true); loadAllTemplates(); } }} className="btn btn-primary btn-sm">
                          <svg style={{ width: '16px', height: '16px', marginRight: '4px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zM21 5a2 2 0 00-2-2h-4a2 2 0 00-2 2v12a4 4 0 004 4h4a2 2 0 002-2V5z" />
                          </svg>
                          选择模板
                        </button>
                      </div>
                    </div>
                    <div className="template-quick-info">
                      <span className="field-count">
                        <svg style={{ width: '16px', height: '16px', marginRight: '4px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                        </svg>
                        {selectedTemplate.fields.length} 个字段
                      </span>
                      <span className="note-type">
                        <svg style={{ width: '16px', height: '16px', marginRight: '4px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                        </svg>
                        {selectedTemplate.note_type}
                      </span>
                    </div>
                  </div>
                </div>
                {showTemplatePreview && (
                  <div className="template-preview-detail">
                    <h6>模板预览：{selectedTemplate.name}</h6>
                    <div className="template-info-detail">
                      <div className="template-description"><strong>描述：</strong>{selectedTemplate.description}</div>
                      <div className="template-fields-detail"><strong>包含字段：</strong><span className="field-list">{selectedTemplate.fields.map((field, index) => (<span key={field} className="field-tag">{field}{index < selectedTemplate.fields.length - 1 && ', '}</span>))}</span></div>
                      <div className="template-usage"><strong>适用场景：</strong>{selectedTemplate.id === 'minimal-card' && '适合快速记忆、概念复习'}{selectedTemplate.id === 'academic-card' && '适合学术学习、专业术语'}{selectedTemplate.id === 'code-card' && '适合编程学习、代码练习'}{selectedTemplate.id === 'cloze-card' && '适合主动回忆、关键词记忆'}{selectedTemplate.id === 'choice-card' && '适合理解检验、选择判断'}</div>
                    </div>
                    <div className="preview-container">
                      <div className="preview-card">
                        <div className="preview-label">正面</div>
                        <div className="preview-content"><IframePreview htmlContent={renderTemplatePreview(selectedTemplate.front_template, selectedTemplate)} cssContent={selectedTemplate.css_style} /></div>
                      </div>
                      <div className="preview-card">
                        <div className="preview-label">背面</div>
                        <div className="preview-content"><IframePreview htmlContent={renderTemplatePreview(selectedTemplate.back_template, selectedTemplate)} cssContent={selectedTemplate.css_style} /></div>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            </div>
            <div className="form-row">
              <div className="form-group">
                <label>目标牌组：</label>
                {ankiDeckNames.length > 0 ? (<select value={options.deck_name} onChange={(e) => setOptions({...options, deck_name: e.target.value})} className="form-control"><option value="">选择已有牌组或输入新牌组名</option>{ankiDeckNames.map(deckName => (<option key={deckName} value={deckName}>{deckName}</option>))}</select>) : (<input type="text" value={options.deck_name} onChange={(e) => setOptions({...options, deck_name: e.target.value})} className="form-control" placeholder="输入牌组名称（如果不存在将自动创建）" />)}
              </div>
              <div className="form-group">
                <label>笔记类型：</label>
                {ankiModelNames.length > 0 ? (<select value={options.note_type} onChange={(e) => setOptions({...options, note_type: e.target.value})} className="form-control"><option value="">选择笔记类型</option>{ankiModelNames.map(modelName => (<option key={modelName} value={modelName}>{modelName}</option>))}</select>) : (<input type="text" value={options.note_type} onChange={(e) => setOptions({...options, note_type: e.target.value})} className="form-control" placeholder="输入笔记类型（如：Basic, Cloze）" />)}
              </div>
            </div>
            <div className="advanced-options">
              <h5>AI生成参数</h5>
              <div className="form-row">
                <div className="form-group">
                  <label>最大令牌数：</label>
                  <input type="number" value={options.max_tokens || 4096} onChange={(e) => setOptions({...options, max_tokens: parseInt(e.target.value) || 4096})} className="form-control" min="512" max="8192" step="256" placeholder="4096" />
                  <small className="form-help">控制AI生成文本的长度（512-8192）</small>
                </div>
                <div className="form-group">
                  <label>创造性温度：</label>
                  <input type="number" value={options.temperature || 0.3} onChange={(e) => setOptions({...options, temperature: parseFloat(e.target.value) || 0.3})} className="form-control" min="0" max="1" step="0.1" placeholder="0.3" />
                  <small className="form-help">控制AI回答的创造性（0-1，数值越低越保守）</small>
                </div>
                <div className="form-group">
                  <label>每个主题最大卡片数：</label>
                  <input type="number" value={options.max_cards_per_mistake} onChange={(e) => setOptions({...options, max_cards_per_mistake: parseInt(e.target.value) || 5})} className="form-control" min="1" max="99" placeholder="5" />
                  <small className="form-help">每个知识点生成的卡片数量</small>
                </div>
                <div className="form-group">
                  <label>任务重叠区域大小：</label>
                  <input 
                    type="number" 
                    value={options.segment_overlap_size} 
                    onChange={(e) => setOptions({...options, segment_overlap_size: parseInt(e.target.value) || 200})} 
                    className="form-control" 
                    min="0" 
                    max="1000" 
                    placeholder="200"
                    title="分段间重叠字符数，防止知识点在分段边界被切断，推荐设置200-400字符"
                  />
                  <small className="form-help">重叠字符数（推荐200-400）</small>
                </div>
              </div>
            </div>
          </div>
          <button onClick={generateCards} disabled={isGenerating || !documentContent} className="btn btn-primary">{isGenerating ? '正在生成...' : '生成ANKI卡片'}</button>
          {isAnkiConnectEnabled && (
            <div className="anki-status" style={{ marginTop: '20px', padding: '15px', backgroundColor: '#f8f9fa', borderRadius: '8px', border: '1px solid #e9ecef' }}>
              <div className="status-indicator">
                <span className="status-label">AnkiConnect状态：</span>
                {isAnkiConnectAvailable === null ? (
                  <span className="status-checking" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <svg style={{ width: '16px', height: '16px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                    </svg>
                    检查中...
                  </span>
                ) : isAnkiConnectAvailable ? (
                  <span className="status-available" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <svg style={{ width: '16px', height: '16px', color: '#10b981' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                    </svg>
                    已连接
                  </span>
                ) : (
                  <span className="status-unavailable" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <svg style={{ width: '16px', height: '16px', color: '#f59e0b' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
                    </svg>
                    未连接
                  </span>
                )}
                <button onClick={checkAnkiConnectStatus} className="btn btn-sm btn-secondary" style={{ marginLeft: '10px' }} disabled={isAnkiConnectAvailable === null}>{isAnkiConnectAvailable === null ? '检查中...' : '重新检查'}</button>
              </div>
              {ankiConnectionError && !isAnkiConnectAvailable && (
                <div className="connection-error" style={{ backgroundColor: '#fff3cd', border: '1px solid #ffeaa7', padding: '12px', borderRadius: '8px', marginTop: '12px', fontSize: '0.9em' }}>
                  <div style={{ color: '#856404', fontWeight: 'bold', marginBottom: '8px' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <svg style={{ width: '16px', height: '16px', color: '#f59e0b' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
                      </svg>
                      AnkiConnect 连接失败: {ankiConnectionError}
                    </div>
                  </div>
                  
                  <div style={{ marginTop: '0.5rem', fontSize: '0.8rem', color: '#6c757d' }}>
                    如果只想导出 .apkg 文件手动导入，可以忽略此错误。
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
        {error && (
          <div className="error-message">
            <span className="error-icon">
              <svg style={{ width: '20px', height: '20px', color: '#f59e0b' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L3.34 16.5c-.77.833.192 2.5 1.732 2.5z" />
              </svg>
            </span>
            <span className="error-text">{error}</span>
          </div>
        )}
        {(isGenerating || isAddingToAnki || isExportingApkg) && (<div className="loading-indicator"><div className="loading-spinner"></div><span className="loading-text">{isGenerating && (<div className="generation-status"><div className="status-main">正在生成ANKI卡片...</div>{currentDocumentId && (<div className="status-detail">文档ID: {currentDocumentId.substring(0, 8)}...{documentTasks.length > 0 && ` | ${documentTasks.length} 个任务`}{generatedCards.length > 0 && ` | ${generatedCards.length} 张卡片已生成`}</div>)}</div>)}{isAddingToAnki && '正在添加到Anki...'}{isExportingApkg && '正在导出.apkg文件...'}</span></div>)}
        {currentDocumentId && (
          <div className="document-processing-status">
            <h3 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <svg style={{ width: '24px', height: '24px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
              </svg>
              文档处理状态
            </h3>
            <div className="document-info">
              <span>文档ID: {currentDocumentId.substring(0, 8)}...</span>
              <span>任务数量: {documentTasks.length}</span>
              {generatedCards.length > 0 && (<><span>总卡片: {generatedCards.length}</span>{generatedCards.filter((card: any) => card.is_error_card).length > 0 && (<span className="error-summary">错误: {generatedCards.filter((card: any) => card.is_error_card).length}</span>)}</>)}
              {documentTasks.filter(t => t.status === 'completed').length === documentTasks.length && documentTasks.length > 0 && !isGenerating && (
                <span className="completion-indicator" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                  <CheckCircle size={16} color="green" />
                  全部完成
                </span>
              )}
            </div>
            {documentTasks.length > 0 && (
              <div className="tasks-list">
                <h4 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <BookOpen size={20} />
                  任务列表
                </h4>
                {documentTasks.map(task => (
                  <div key={task.task_id} className={`task-item ${selectedTaskId === task.task_id ? 'selected' : ''}`} onClick={() => setSelectedTaskId(task.task_id)}>
                    <div className="task-header">
                      <span className="task-id">任务 {task.task_id.substring(0, 8)}</span>
                      <div className="task-status-info">
                        <span className={`task-status status-${task.status}`}>
                          {task.status === 'pending' && (
                            <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                              <Clock size={14} />
                              等待中
                            </span>
                          )}
                          {task.status === 'processing' && (
                            <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                              <RefreshCcw size={14} />
                              处理中
                            </span>
                          )}
                          {task.status === 'completed' && (
                            <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                              <CheckCircle size={14} color="green" />
                              已完成
                            </span>
                          )}
                          {task.status === 'failed' && (
                            <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                              <X size={14} color="red" />
                              失败
                            </span>
                          )}
                          {task.status === 'truncated' && (
                            <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                              <AlertTriangle size={14} color="orange" />
                              已截断
                            </span>
                          )}
                        </span>
                        {task.cards.some((card: any) => card.is_error_card) && (
                          <span className="error-indicator" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                            <AlertTriangle size={14} color="orange" />
                            {task.cards.filter((card: any) => card.is_error_card).length} 个错误
                          </span>
                        )}
                      </div>
                    </div>
                    {task.content_preview && (<div className="task-preview">{task.content_preview}</div>)}
                    {task.status === 'failed' && task.error_message && (<div className="task-error-message" style={{ color: 'red', fontSize: '0.9em', marginTop: '5px', whiteSpace: 'pre-wrap' }}>错误详情: {task.error_message}</div>)}
                    <div className="task-progress">
                      <div className="progress-bar"><div className="progress-fill" style={{ width: `${task.progress}%` }} /></div>
                      <span className="progress-text">{task.progress}% - {task.cards.length} 张卡片{task.cards.filter((card: any) => card.is_error_card).length > 0 && (<span className="error-count">({task.cards.filter((card: any) => card.is_error_card).length} 错误)</span>)}</span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
        {generatedCards.length > 0 && (
          <div className="cards-preview">
            <div className="cards-header">
              <h3>{selectedTaskId ? `任务 ${selectedTaskId.substring(0, 8)} 的卡片 (${getTaskCards().length}张)` : `全部卡片 (${generatedCards.length}张)`}</h3>
              <div className="card-controls">
                {selectedTaskId && (<button onClick={() => setSelectedTaskId(null)} className="btn btn-outline btn-sm">显示全部卡片</button>)}
                <button onClick={toggleSelectAll} className="btn btn-secondary">{selectedCards.size === getCurrentDisplayCards().length ? '取消全选' : '全选'}</button>
                <span className="selected-count">已选择: {selectedCards.size}/{getCurrentDisplayCards().length}</span>
              </div>
            </div>
            <div className="cards-list">
              {getCurrentDisplayCards().map((card: any, localIndex: number) => {
                const globalIndex = getCardGlobalIndex(localIndex);
                return (
                  <div key={`${selectedTaskId || 'all'}-${localIndex}`} className={`card-item ${selectedCards.has(globalIndex) ? 'selected' : ''} ${(card as any).is_error_card ? 'error-card' : ''}`}>
                    <div className="card-header">
                      <input type="checkbox" checked={selectedCards.has(globalIndex)} onChange={() => toggleCardSelection(globalIndex)} />
                      <span className="card-number">
                        {(card as any).is_error_card ? (
                          <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                            <AlertTriangle size={16} color="orange" />
                            错误卡片
                          </span>
                        ) : (
                          `卡片 ${selectedTaskId ? localIndex + 1 : globalIndex + 1}`
                        )}
                      </span>
                      <button onClick={() => deleteCard(globalIndex)} className="btn btn-danger btn-sm">删除</button>
                    </div>
                    {(card as any).is_error_card && (card as any).error_content && (<div className="error-content"><label>错误原因：</label><div className="error-text">{(card as any).error_content}</div></div>)}
                    <div className="card-content">
                      {selectedTemplate.note_type === 'Cloze' ? (
                        <>
                          <div className="card-field">
                            <label>填空题文本：</label>
                            <textarea value={card.text || card.front || ''} onChange={(e) => editCard(globalIndex, 'text', e.target.value)} rows={4} className="form-control" placeholder="包含填空标记的文本，如：定律公式是 F = {{c1::ma}}" />
                            <small className="form-help">使用{`{{c1::答案}}`}、{`{{c2::答案}}`}等格式标记填空部分</small>
                          </div>
                          <div className="card-field">
                            <label>预览效果：</label>
                            <div className="cloze-preview" dangerouslySetInnerHTML={{ __html: renderClozeText(card.text || card.front || '') }} />
                          </div>
                        </>
                      ) : (
                        <>
                          <div className="card-field">
                            <label>正面（问题）：</label>
                            <textarea value={card.front} onChange={(e) => editCard(globalIndex, 'front', e.target.value)} rows={2} className="form-control" />
                          </div>
                          <div className="card-field">
                            <label>背面（答案）：</label>
                            <textarea value={card.back} onChange={(e) => editCard(globalIndex, 'back', e.target.value)} rows={3} className="form-control" />
                          </div>
                        </>
                      )}
                      <div className="card-field">
                        <label>标签：</label>
                        <div className="tags">{card.tags.map((tag: string, tagIndex: number) => (<span key={tagIndex} className="tag">{tag}</span>))}</div>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
            <div className="output-actions">
              <div className="export-options">
                <h5 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <Upload size={20} />
                  导出选项
                </h5>
                <div className="export-buttons">
                  {currentDocumentId && (<button className="btn btn-success btn-sm" disabled={generatedCards.length === 0 || isExportingApkg} onClick={() => handleExportByLevel('document')}>导出整个文档 ({generatedCards.length}张)</button>)}
                  {selectedTaskId && (<button className="btn btn-success btn-sm" disabled={getTaskCards().length === 0 || isExportingApkg} onClick={() => handleExportByLevel('task')}>导出当前任务 ({getTaskCards().length}张)</button>)}
                  <button className="btn btn-success btn-sm" disabled={selectedCards.size === 0 || isExportingApkg} onClick={() => handleExportByLevel('selection')}>导出选中卡片 ({selectedCards.size}张)</button>
                </div>
              </div>
              {isAnkiConnectEnabled && (
                <div className="anki-connect-actions">
                  <h5 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <Settings size={20} />
                    AnkiConnect
                  </h5>
                  <button className="btn btn-info" disabled={selectedCards.size === 0 || isAddingToAnki || !isAnkiConnectAvailable} onClick={handleAddToAnkiConnect} title={!isAnkiConnectAvailable ? "请确保Anki桌面程序正在运行并启用了AnkiConnect插件" : ""}>{isAddingToAnki ? '添加中...' : `添加到Anki (${selectedCards.size}张)`}</button>
                </div>
              )}
            </div>
          </div>
        )}
      </div>
      {showTemplateManager && (<TemplateManager onClose={() => setShowTemplateManager(false)} onSelectTemplate={(template) => { handleTemplateSelect(template); setShowTemplateManager(false); }} />)}
      <style dangerouslySetInnerHTML={{ __html: `.cloze-highlight { background: #FFD700; color: #2c3e50; padding: 2px 8px; border-radius: 6px; font-weight: 600; box-shadow: 0 2px 4px rgba(0,0,0,0.2); } .cloze-preview { padding: 12px; background: #f8fafc; border-radius: 8px; border: 1px solid #e2e8f0; line-height: 1.6; font-size: 14px; }` }} />
    </div>
  );
};

export default AnkiCardGeneration;

