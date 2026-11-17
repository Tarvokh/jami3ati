import React, { useState, useRef, useEffect, useMemo } from 'react';
import { University, Institution, Language } from '../types';
import { translations } from '../data/translations';
import { Search, Building2, GraduationCap, MapPin, X, History, ArrowUpRight } from 'lucide-react';

interface Props {
  searchQuery: string;
  onSearchChange: (query: string) => void;
  universities: University[];
  institutions: Institution[];
  recentSearches: string[];
  onAddRecentSearch: (query: string) => void;
  onRemoveRecentSearch: (query: string) => void;
  onClearRecentSearches: () => void;
  onSelectSuggestion?: (item: University | Institution, type: 'university' | 'institution') => void;
  lang: Language;
}

export const SearchBar: React.FC<Props> = ({
  searchQuery,
  onSearchChange,
  universities,
  institutions,
  recentSearches,
  onAddRecentSearch,
  onRemoveRecentSearch,
  onClearRecentSearches,
  onSelectSuggestion,
  lang,
}) => {
  const [isOpen, setIsOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState<number>(-1);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const isAr = lang === 'ar';
  const t = translations[lang];

  // Generate matching suggestions dynamically from universities and institutions
  const suggestions = useMemo(() => {
    const q = searchQuery.toLowerCase().trim();
    if (!q || q.length < 1) return [];

    const matches: Array<{
      id: string;
      code: string;
      title_ar: string;
      title_fr: string;
      city_ar: string;
      city_fr: string;
      type: 'university' | 'institution';
      rawItem: University | Institution;
    }> = [];

    // Check universities
    universities.forEach((u) => {
      if (
        u.name_ar.toLowerCase().includes(q) ||
        u.name_fr.toLowerCase().includes(q) ||
        u.code.toLowerCase().includes(q) ||
        u.city_ar.toLowerCase().includes(q) ||
        u.city_fr.toLowerCase().includes(q)
      ) {
        matches.push({
          id: u.id,
          code: u.code,
          title_ar: u.name_ar,
          title_fr: u.name_fr,
          city_ar: u.city_ar,
          city_fr: u.city_fr,
          type: 'university',
          rawItem: u,
        });
      }
    });

    // Check institutions
    institutions.forEach((inst) => {
      if (
        inst.name_ar.toLowerCase().includes(q) ||
        inst.name_fr.toLowerCase().includes(q) ||
        inst.code.toLowerCase().includes(q) ||
        inst.university_name_ar.toLowerCase().includes(q) ||
        inst.university_name_fr.toLowerCase().includes(q) ||
        inst.city_ar.toLowerCase().includes(q) ||
        inst.city_fr.toLowerCase().includes(q) ||
        inst.programs_summary_ar.some((p) => p.toLowerCase().includes(q))
      ) {
        matches.push({
          id: inst.id,
          code: inst.code,
          title_ar: inst.name_ar,
          title_fr: inst.name_fr,
          city_ar: inst.city_ar,
          city_fr: inst.city_fr,
          type: 'institution',
          rawItem: inst,
        });
      }
    });

    return matches.slice(0, 7); // Limit to top 7 relevant suggestions
  }, [searchQuery, universities, institutions]);

  // Click outside listener to dismiss suggestions
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const handleSelect = (s: typeof suggestions[0]) => {
    onSearchChange(s.code);
    onAddRecentSearch(s.code);
    setIsOpen(false);
    if (onSelectSuggestion) {
      onSelectSuggestion(s.rawItem, s.type);
    } else {
      document.getElementById('directory')?.scrollIntoView({ behavior: 'smooth' });
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!isOpen || suggestions.length === 0) {
      if (e.key === 'Enter' && searchQuery.trim()) {
        onAddRecentSearch(searchQuery);
        document.getElementById('directory')?.scrollIntoView({ behavior: 'smooth' });
      }
      return;
    }

    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelectedIndex((prev) => (prev < suggestions.length - 1 ? prev + 1 : 0));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelectedIndex((prev) => (prev > 0 ? prev - 1 : suggestions.length - 1));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (selectedIndex >= 0 && selectedIndex < suggestions.length) {
        handleSelect(suggestions[selectedIndex]);
      } else if (searchQuery.trim()) {
        onAddRecentSearch(searchQuery);
        setIsOpen(false);
        document.getElementById('directory')?.scrollIntoView({ behavior: 'smooth' });
      }
    } else if (e.key === 'Escape') {
      setIsOpen(false);
    }
  };

  return (
    <div className="relative w-full max-w-xl" ref={containerRef}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (searchQuery.trim()) {
            onAddRecentSearch(searchQuery);
            setIsOpen(false);
            document.getElementById('directory')?.scrollIntoView({ behavior: 'smooth' });
          }
        }}
        className="relative w-full"
      >
        <div className="absolute inset-y-0 start-0 flex items-center ps-3.5 pointer-events-none text-stone-400">
          <Search className="w-5 h-5 text-[#143D30]" />
        </div>
        <input
          ref={inputRef}
          id="main-search-input"
          type="text"
          value={searchQuery}
          onChange={(e) => {
            onSearchChange(e.target.value);
            setIsOpen(true);
            setSelectedIndex(-1);
          }}
          onFocus={() => {
            if (searchQuery.trim()) setIsOpen(true);
          }}
          onKeyDown={handleKeyDown}
          placeholder={t.search_placeholder}
          className="w-full ps-11 pe-12 py-3.5 bg-white border border-stone-300 rounded-xl shadow-xs text-sm sm:text-base focus:outline-none focus:ring-2 focus:ring-[#143D30] focus:border-transparent text-stone-900"
          autoComplete="off"
        />
        <div className="absolute inset-y-0 end-0 flex items-center pe-3 gap-1.5">
          {searchQuery ? (
            <button
              type="button"
              onClick={() => {
                onSearchChange('');
                setIsOpen(false);
              }}
              className="text-xs text-stone-400 hover:text-stone-700 px-1 py-0.5"
            >
              مسح
            </button>
          ) : (
            <kbd
              className="hidden sm:inline-block px-1.5 py-0.5 text-[10px] font-mono font-bold text-stone-400 bg-stone-100 rounded border border-stone-200"
              title="اضغط على / للبحث مباشرة"
            >
              /
            </kbd>
          )}
        </div>
      </form>

      {/* Autocomplete Dropdown List */}
      {isOpen && suggestions.length > 0 && (
        <div className="absolute top-full start-0 end-0 mt-2 bg-white rounded-xl border border-stone-200 shadow-xl z-50 overflow-hidden animate-in fade-in zoom-in-95 duration-100">
          <div className="p-2 bg-stone-50 border-b border-stone-100 text-[11px] font-semibold text-stone-500 flex items-center justify-between">
            <span>{isAr ? 'مقترحات البحث المباشرة:' : 'Suggestions de recherche :'}</span>
            <span className="font-mono text-[10px]">{suggestions.length} نتائج</span>
          </div>

          <ul className="max-h-72 overflow-y-auto divide-y divide-stone-100">
            {suggestions.map((s, idx) => {
              const isSelected = idx === selectedIndex;
              const title = isAr ? s.title_ar : s.title_fr;
              const city = isAr ? s.city_ar : s.city_fr;

              return (
                <li key={s.id}>
                  <button
                    type="button"
                    onClick={() => handleSelect(s)}
                    onMouseEnter={() => setSelectedIndex(idx)}
                    className={`w-full text-start p-3 transition-colors flex items-center justify-between gap-3 text-xs ${
                      isSelected ? 'bg-[#FAF7F0] text-[#143D30]' : 'hover:bg-stone-50 text-stone-800'
                    }`}
                  >
                    <div className="flex items-center gap-2.5 min-w-0">
                      <div
                        className={`w-7 h-7 rounded-lg flex items-center justify-center shrink-0 ${
                          s.type === 'university'
                            ? 'bg-[#143D30] text-white'
                            : 'bg-emerald-100 text-emerald-800'
                        }`}
                      >
                        {s.type === 'university' ? (
                          <Building2 className="w-3.5 h-3.5" />
                        ) : (
                          <GraduationCap className="w-3.5 h-3.5" />
                        )}
                      </div>

                      <div className="min-w-0">
                        <div className="font-bold truncate text-stone-900 flex items-center gap-1.5">
                          <span>{title}</span>
                          <span className="font-mono text-[10px] bg-stone-200/80 px-1.5 py-0.2 rounded font-bold shrink-0">
                            {s.code}
                          </span>
                        </div>
                        <span className="text-[11px] text-stone-500 flex items-center gap-1 mt-0.5">
                          <MapPin className="w-3 h-3 text-stone-400" />
                          <span>{city}</span>
                        </span>
                      </div>
                    </div>

                    <ArrowUpRight className="w-3.5 h-3.5 text-stone-400 shrink-0" />
                  </button>
                </li>
              );
            })}
          </ul>
        </div>
      )}

      {/* Recent Searches Strip */}
      {recentSearches.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 mt-2.5 text-xs text-stone-500 w-full">
          <span className="font-semibold text-stone-700 flex items-center gap-1 me-1">
            <History className="w-3.5 h-3.5 text-[#143D30]" />
            <span>{isAr ? 'البحث الأخير:' : 'Récents :'}</span>
          </span>
          {recentSearches.map((term) => (
            <span
              key={term}
              className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-white border border-stone-200 text-stone-800 text-[11px] hover:border-[#143D30] transition-colors shadow-2xs group"
            >
              <button
                type="button"
                onClick={() => {
                  onSearchChange(term);
                  onAddRecentSearch(term);
                  document.getElementById('directory')?.scrollIntoView({ behavior: 'smooth' });
                }}
                className="hover:text-[#143D30] font-medium"
              >
                {term}
              </button>
              <button
                type="button"
                onClick={(e) => {
                  e.stopPropagation();
                  onRemoveRecentSearch(term);
                }}
                className="text-stone-400 hover:text-red-600 p-0.5 rounded"
                title="حذف"
              >
                <X className="w-3 h-3" />
              </button>
            </span>
          ))}
          <button
            type="button"
            onClick={onClearRecentSearches}
            className="text-[10px] text-stone-400 hover:text-red-600 underline ms-1"
            title="مسح سجل البحث"
          >
            {isAr ? 'مسح السجل' : 'Effacer'}
          </button>
        </div>
      )}
    </div>
  );
};
