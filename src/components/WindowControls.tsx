import { useState, useEffect } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';

export const WindowControls: React.FC = () => {
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    const initWindow = async () => {
      const window = getCurrentWindow();
      
      // 检查初始最大化状态
      try {
        const maximized = await window.isMaximized();
        setIsMaximized(maximized);
      } catch (error) {
        console.error('Failed to get maximized state:', error);
      }

      // 监听窗口状态变化
      try {
        const unlisten = await window.listen('tauri://resize', async () => {
          try {
            const maximized = await window.isMaximized();
            setIsMaximized(maximized);
          } catch (error) {
            console.error('Failed to update maximized state:', error);
          }
        });

                return () => {
        };
      } catch (error) {
        console.error('Failed to listen to window events:', error);
      }
    };

    initWindow();
  }, []);

  const handleMinimize = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      const window = getCurrentWindow();
      await window.minimize();
    } catch (error) {
      console.error('Failed to minimize window:', error);
    }
  };

  const handleMaximize = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      const window = getCurrentWindow();
      if (isMaximized) {
        await window.unmaximize();
        setIsMaximized(false);
      } else {
        await window.maximize();
        setIsMaximized(true);
      }
    } catch (error) {
      console.error('Failed to toggle maximize:', error);
    }
  };

  const handleClose = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      const window = getCurrentWindow();
      await window.close();
    } catch (error) {
      console.error('Failed to close window:', error);
    }
  };

  return (
    <div className="window-controls">
      <button 
        className="window-button minimize"
        onClick={handleMinimize}
        onMouseDown={(e) => e.stopPropagation()}
        title="最小化"
      >
        <svg width="12" height="12" viewBox="0 0 12 12">
          <path d="M2 6h8" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
      
      <button 
        className="window-button maximize"
        onClick={handleMaximize}
        onMouseDown={(e) => e.stopPropagation()}
        title={isMaximized ? "还原" : "最大化"}
      >
        {isMaximized ? (
          <svg width="12" height="12" viewBox="0 0 12 12">
            <path d="M3 3h6v6H3V3z M1 1h6v2H3v4H1V1z" stroke="currentColor" strokeWidth="1" fill="none" />
          </svg>
        ) : (
          <svg width="12" height="12" viewBox="0 0 12 12">
            <path d="M2 2h8v8H2V2z" stroke="currentColor" strokeWidth="1" fill="none" />
          </svg>
        )}
      </button>
      
      <button 
        className="window-button close"
        onClick={handleClose}
        onMouseDown={(e) => e.stopPropagation()}
        title="关闭"
      >
        <svg width="12" height="12" viewBox="0 0 12 12">
          <path d="M3 3l6 6M9 3l-6 6" stroke="currentColor" strokeWidth="1" />
        </svg>
      </button>
    </div>
  );
};