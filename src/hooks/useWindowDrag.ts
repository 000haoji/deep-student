import { getCurrentWindow } from '@tauri-apps/api/window';

export const useWindowDrag = () => {
  const startDragging = async (e: React.MouseEvent) => {
    e.preventDefault();
    try {
      const window = getCurrentWindow();
      await window.startDragging();
    } catch (error) {
      console.error('Failed to start dragging:', error);
    }
  };

  return { startDragging };
};