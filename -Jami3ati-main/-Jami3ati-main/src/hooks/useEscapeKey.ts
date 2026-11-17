import { useEffect } from 'react';

/**
 * Custom hook that listens for the 'Escape' key globally to trigger a callback
 * @param onEscape Callback function to execute when 'Escape' is pressed
 * @param isEnabled Whether the listener should be currently active
 */
export function useEscapeKey(onEscape: () => void, isEnabled: boolean = true) {
  useEffect(() => {
    if (!isEnabled) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onEscape();
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  }, [onEscape, isEnabled]);
}
