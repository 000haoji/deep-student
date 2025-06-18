import React from 'react';
import '../styles/modern-sidebar.css';
import {
  FileText,
  Layers,
  Target,
  BookOpen,
  Brain,
  Search,
  CreditCard,
  Palette,
  Image,
  BarChart3,
  Package,
  Settings,
  FlaskConical,
  ChevronLeft,
  ChevronRight,
  Network
} from 'lucide-react';

type CurrentView = 'analysis' | 'library' | 'settings' | 'mistake-detail' | 'batch' | 'review' | 'dashboard' | 'data-management' | 'unified-review' | 'create-review' | 'review-session' | 'anki-generation' | 'knowledge-base' | 'rag-query' | 'image-occlusion' | 'template-management' | 'gemini-adapter-test' | 'cogni-graph';

interface ModernSidebarProps {
  currentView: CurrentView;
  onViewChange: (view: CurrentView) => void;
  sidebarCollapsed: boolean;
  onToggleSidebar: () => void;
  batchAnalysisEnabled: boolean;
  geminiAdapterTestEnabled: boolean;
  imageOcclusionEnabled: boolean;
  startDragging: (e: React.MouseEvent) => void;
}

interface NavItem {
  id: CurrentView;
  label: string;
  icon: React.ReactNode;
  section: string;
  visible?: boolean;
}

export const ModernSidebar: React.FC<ModernSidebarProps> = ({
  currentView,
  onViewChange,
  sidebarCollapsed,
  onToggleSidebar,
  batchAnalysisEnabled,
  geminiAdapterTestEnabled,
  imageOcclusionEnabled,
  startDragging
}) => {
  const navItems: NavItem[] = [
    // 分析工具
    { id: 'analysis', label: '分析', icon: <FileText size={20} />, section: '分析工具' },
    { id: 'batch', label: '批量分析', icon: <Layers size={20} />, section: '分析工具', visible: batchAnalysisEnabled },
    { id: 'unified-review', label: '统一回顾', icon: <Target size={20} />, section: '分析工具' },
    
    // 知识管理
    { id: 'library', label: '错题库', icon: <BookOpen size={20} />, section: '知识管理' },
    { id: 'knowledge-base', label: '知识库', icon: <Brain size={20} />, section: '知识管理' },
    { id: 'rag-query', label: 'RAG查询', icon: <Search size={20} />, section: '知识管理' },
    { id: 'cogni-graph', label: '知识图谱', icon: <Network size={20} />, section: '知识管理' },
    
    // ANKI工具
    { id: 'anki-generation', label: 'ANKI制卡', icon: <CreditCard size={20} />, section: 'ANKI工具' },
    { id: 'template-management', label: '模板管理', icon: <Palette size={20} />, section: 'ANKI工具' },
    { id: 'image-occlusion', label: '图片遮罩卡', icon: <Image size={20} />, section: 'ANKI工具', visible: imageOcclusionEnabled },
    
    // 系统工具
    { id: 'dashboard', label: '数据统计', icon: <BarChart3 size={20} />, section: '系统工具' },
    { id: 'data-management', label: '数据管理', icon: <Package size={20} />, section: '系统工具' },
    { id: 'gemini-adapter-test', label: 'Gemini适配器测试', icon: <FlaskConical size={20} />, section: '系统工具', visible: geminiAdapterTestEnabled },
  ];

  const sections = [...new Set(navItems.map(item => item.section))];

  return (
    <div className={`app-sidebar ${sidebarCollapsed ? 'collapsed' : ''}`}>
      <div className="sidebar-header" onMouseDown={startDragging}>
        <div className="app-logo">
          {sidebarCollapsed ? (
            <img src="/dslogo2.png" alt="Deep Student" className="logo-icon" />
          ) : (
            <img src="/dslogo1.png" alt="Deep Student" className="logo-full" />
          )}
        </div>
      </div>
      
      <nav className="sidebar-nav">
        {sections.map(section => {
          const sectionItems = navItems.filter(item => 
            item.section === section && (item.visible === undefined || item.visible)
          );
          
          if (sectionItems.length === 0) return null;
          
          return (
            <div key={section} className="nav-section">
              <div className="nav-label">{!sidebarCollapsed && section}</div>
              {sectionItems.map(item => (
                <button
                  key={item.id}
                  className={`nav-item tooltip-test ${currentView === item.id ? 'active' : ''}`}
                  onClick={() => onViewChange(item.id)}
                  data-tooltip={sidebarCollapsed ? item.label : ''}
                >
                  <span className="nav-icon">{item.icon}</span>
                  {!sidebarCollapsed && <span className="nav-text">{item.label}</span>}
                </button>
              ))}
            </div>
          );
        })}
      </nav>
      
      <div className="sidebar-footer">
        <button 
          className={`nav-item tooltip-test ${currentView === 'settings' ? 'active' : ''}`}
          onClick={() => onViewChange('settings')}
          data-tooltip={sidebarCollapsed ? '设置' : ''}
        >
          <span className="nav-icon"><Settings size={20} /></span>
          {!sidebarCollapsed && <span className="nav-text">设置</span>}
        </button>
      </div>
    </div>
  );
}; 