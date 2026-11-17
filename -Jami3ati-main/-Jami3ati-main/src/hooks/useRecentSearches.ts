import { useState, useEffect } from 'react';

const STORAGE_KEY = 'jami3ati_recent_searches_v1';
const MAX_RECENT_SEARCHES = 5;

export function useRecentSearches() {
  const [recentSearches, setRecentSearches] = useState<string[]>(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) {
        const parsed = JSON.parse(stored);
        if (Array.isArray(parsed)) {
          return parsed.slice(0, MAX_RECENT_SEARCHES);
        }
      }
    } catch (e) {
      console.warn('Failed to parse recent searches from localStorage', e);
    }
    return ['ENSIAS', 'الرباط', 'مراكش', 'كلية العلوم'];
  });

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(recentSearches));
    } catch (e) {
      console.warn('Failed to save recent searches to localStorage', e);
    }
  }, [recentSearches]);

  const addRecentSearch = (term: string) => {
    const trimmed = term.trim();
    if (!trimmed || trimmed.length < 2) return;

    setRecentSearches((prev) => {
      const filtered = prev.filter((item) => item.toLowerCase() !== trimmed.toLowerCase());
      return [trimmed, ...filtered].slice(0, MAX_RECENT_SEARCHES);
    });
  };

  const removeRecentSearch = (term: string) => {
    setRecentSearches((prev) =>
      prev.filter((item) => item.toLowerCase() !== term.toLowerCase())
    );
  };

  const clearRecentSearches = () => {
    setRecentSearches([]);
  };

  return {
    recentSearches,
    addRecentSearch,
    removeRecentSearch,
    clearRecentSearches,
  };
}
