import React, { useState } from 'react';
import { Language, DataStatus, InstitutionType, DisciplineKey } from '../types';
import { DISCIPLINE_OPTIONS } from '../data/seedData';
import { translations } from '../data/translations';
import {
  Search,
  RotateCcw,
  SlidersHorizontal,
  ChevronDown,
  X,
  Cpu,
  Stethoscope,
  TrendingUp,
  Atom,
  Scale,
  BookOpen,
  Building2,
  GraduationCap,
  Sparkles,
  Check,
} from 'lucide-react';

interface Props {
  lang: Language;
  searchQuery: string;
  onSearchChange: (q: string) => void;
  selectedCity: string;
  onCityChange: (city: string) => void;
  selectedStatus: string;
  onStatusChange: (status: string) => void;
  selectedType: string;
  onTypeChange: (type: string) => void;
  selectedDiscipline: DisciplineKey | 'all';
  onDisciplineChange: (discipline: DisciplineKey | 'all') => void;
  viewFilter: 'all' | 'universities' | 'institutions';
  onViewFilterChange: (view: 'all' | 'universities' | 'institutions') => void;
  totalResultsCount: number;
  universitiesCount: number;
  institutionsCount: number;
  disciplineCounts: Record<string, number>;
  onResetFilters: () => void;
}

export const FilterPanel: React.FC<Props> = ({
  lang,
  searchQuery,
  onSearchChange,
  selectedCity,
  onCityChange,
  selectedStatus,
  onStatusChange,
  selectedType,
  onTypeChange,
  selectedDiscipline,
  onDisciplineChange,
  viewFilter,
  onViewFilterChange,
  totalResultsCount,
  universitiesCount,
  institutionsCount,
  disciplineCounts,
  onResetFilters,
}) => {
  const [showAdvanced, setShowAdvanced] = useState(false);
  const t = translations[lang];
  const isAr = lang === 'ar';

  const hasActiveFilters =
    Boolean(searchQuery) ||
    selectedCity !== 'all' ||
    selectedStatus !== 'all' ||
    selectedType !== 'all' ||
    selectedDiscipline !== 'all' ||
    viewFilter !== 'all';

  const getDisciplineIcon = (key: DisciplineKey) => {
    switch (key) {
      case 'engineering':
        return <Cpu className="w-3.5 h-3.5" />;
      case 'medicine':
        return <Stethoscope className="w-3.5 h-3.5" />;
      case 'economics':
        return <TrendingUp className="w-3.5 h-3.5" />;
      case 'science_tech':
        return <Atom className="w-3.5 h-3.5" />;
      case 'law':
        return <Scale className="w-3.5 h-3.5" />;
      case 'humanities':
        return <BookOpen className="w-3.5 h-3.5" />;
      default:
        return <Sparkles className="w-3.5 h-3.5" />;
    }
  };

  return (
    <div className="bg-white rounded-2xl border border-stone-200/90 shadow-xs overflow-hidden transition-all duration-200">
      {/* Top Bar: View Segmented Control & Search + Shortcut */}
      <div className="p-4 sm:p-5 border-b border-stone-100 flex flex-col md:flex-row md:items-center justify-between gap-4">
        {/* Segmented View Switcher */}
        <div className="inline-flex items-center gap-1 p-1 bg-stone-100/90 rounded-xl text-xs font-medium self-start md:self-auto w-full sm:w-auto">
          <button
            type="button"
            onClick={() => onViewFilterChange('all')}
            className={`flex-1 sm:flex-initial px-3.5 py-1.5 rounded-lg transition-all focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none ${
              viewFilter === 'all'
                ? 'bg-white text-stone-900 shadow-xs font-bold'
                : 'text-stone-600 hover:text-stone-900'
            }`}
            aria-label={isAr ? 'عرض الكل' : 'Tous'}
            tabIndex={0}
          >
            {isAr ? 'الكل' : 'Tous'} ({universitiesCount + institutionsCount})
          </button>
          <button
            type="button"
            onClick={() => onViewFilterChange('universities')}
            className={`flex-1 sm:flex-initial px-3.5 py-1.5 rounded-lg transition-all flex items-center justify-center gap-1.5 focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none ${
              viewFilter === 'universities'
                ? 'bg-white text-stone-900 shadow-xs font-bold'
                : 'text-stone-600 hover:text-stone-900'
            }`}
            aria-label={isAr ? 'عرض الجامعات فقط' : 'Universités seulement'}
            tabIndex={0}
          >
            <Building2 className="w-3 h-3 text-[#143D30]" />
            <span>{isAr ? 'الجامعات' : 'Universités'} ({universitiesCount})</span>
          </button>
          <button
            type="button"
            onClick={() => onViewFilterChange('institutions')}
            className={`flex-1 sm:flex-initial px-3.5 py-1.5 rounded-lg transition-all flex items-center justify-center gap-1.5 focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none ${
              viewFilter === 'institutions'
                ? 'bg-white text-stone-900 shadow-xs font-bold'
                : 'text-stone-600 hover:text-stone-900'
            }`}
            aria-label={isAr ? 'عرض المؤسسات والكليات فقط' : 'Établissements seulement'}
            tabIndex={0}
          >
            <GraduationCap className="w-3 h-3 text-[#143D30]" />
            <span>{isAr ? 'المؤسسات والكليات' : 'Établissements'} ({institutionsCount})</span>
          </button>
        </div>

        {/* Live Counter & Toggle Advanced Filter on Mobile */}
        <div className="flex items-center justify-between md:justify-end gap-3 text-xs text-stone-500">
          <div className="flex items-center gap-1.5">
            <span>{t.results_count}</span>
            <span className="font-mono font-bold text-sm text-[#143D30] bg-[#FAF7F0] px-2 py-0.5 rounded border border-[#E7E2D6]">
              {totalResultsCount}
            </span>
          </div>

          <button
            type="button"
            onClick={() => setShowAdvanced((prev) => !prev)}
            className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-xs font-medium transition-colors focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none ${
              showAdvanced || selectedCity !== 'all' || selectedStatus !== 'all' || selectedType !== 'all'
                ? 'bg-[#143D30] text-white border-[#143D30]'
                : 'bg-stone-50 text-stone-700 hover:bg-stone-100 border-stone-200'
            }`}
            aria-label={isAr ? 'فتح التصفية التفصيلية' : 'Filtres avancés'}
            tabIndex={0}
          >
            <SlidersHorizontal className="w-3.5 h-3.5" />
            <span>{isAr ? 'تصفية تفصيلية' : 'Filtres avancés'}</span>
            <ChevronDown className={`w-3.5 h-3.5 transition-transform ${showAdvanced ? 'rotate-180' : ''}`} />
          </button>
        </div>
      </div>

      {/* Primary Feature: Study Disciplines Filter Strip (التخصصات الدراسية المتاحة) */}
      <div className="px-4 sm:px-5 py-3.5 bg-[#FAF8F5] border-b border-stone-200/80">
        <div className="flex items-center justify-between mb-2">
          <span className="text-xs font-bold text-stone-900 flex items-center gap-1.5">
            <Sparkles className="w-3.5 h-3.5 text-[#143D30]" />
            <span>{isAr ? 'التخصصات والشعب الدراسية بالمؤسسات:' : 'Filières & disciplines académiques :'}</span>
          </span>
          {selectedDiscipline !== 'all' && (
            <button
              onClick={() => onDisciplineChange('all')}
              className="text-[11px] text-[#B43425] hover:underline font-medium focus-visible:ring-2 focus-visible:ring-[#B43425] focus-visible:outline-none rounded"
              tabIndex={0}
            >
              {isAr ? 'إلغاء تصفية التخصص' : 'Tous les domaines'}
            </button>
          )}
        </div>

        {/* Scrollable / Responsive Chips */}
        <div className="flex items-center gap-2 overflow-x-auto pb-1.5 pt-0.5 no-scrollbar">
          {/* All disciplines button */}
          <button
            type="button"
            onClick={() => onDisciplineChange('all')}
            className={`shrink-0 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold transition-all shadow-xs focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none ${
              selectedDiscipline === 'all'
                ? 'bg-[#143D30] text-white ring-2 ring-[#143D30]/20'
                : 'bg-white text-stone-700 hover:bg-stone-100 border border-stone-200 hover:border-stone-300'
            }`}
            tabIndex={0}
          >
            <span>{isAr ? 'جميع التخصصات' : 'Toutes les disciplines'}</span>
            <span
              className={`font-mono text-[10px] px-1.5 py-0.2 rounded ${
                selectedDiscipline === 'all' ? 'bg-white/20 text-white' : 'bg-stone-100 text-stone-600'
              }`}
            >
              {institutionsCount}
            </span>
          </button>

          {/* Discipline buttons */}
          {DISCIPLINE_OPTIONS.map((disc) => {
            const isSelected = selectedDiscipline === disc.key;
            const count = disciplineCounts[disc.key] || 0;

            return (
              <button
                key={disc.key}
                type="button"
                onClick={() => onDisciplineChange(isSelected ? 'all' : disc.key)}
                className={`shrink-0 inline-flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold transition-all shadow-xs focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none ${
                  isSelected
                    ? 'bg-[#143D30] text-white ring-2 ring-[#143D30]/20'
                    : 'bg-white text-stone-700 hover:bg-stone-100 border border-stone-200 hover:border-stone-300'
                }`}
                title={isAr ? disc.description_ar : disc.description_fr}
                aria-label={isAr ? disc.label_ar : disc.label_fr}
                tabIndex={0}
              >
                <span className={isSelected ? 'text-amber-300' : 'text-[#143D30]'}>
                  {getDisciplineIcon(disc.key)}
                </span>
                <span>{isAr ? disc.label_ar : disc.label_fr}</span>
                <span
                  className={`font-mono text-[10px] px-1.5 py-0.2 rounded ${
                    isSelected ? 'bg-white/20 text-white' : 'bg-stone-100 text-stone-600'
                  }`}
                >
                  {count}
                </span>
              </button>
            );
          })}
        </div>
      </div>

      {/* Advanced Secondary Filters (Collapsible on mobile, clean row on desktop if opened) */}
      {showAdvanced && (
        <div className="p-4 sm:p-5 bg-white border-b border-stone-100 grid grid-cols-1 sm:grid-cols-3 gap-3 animate-in slide-in-from-top-1 duration-150 text-xs">
          {/* City filter */}
          <div>
            <label className="block text-xs font-semibold text-stone-700 mb-1">
              {t.filter_by_city}
            </label>
            <select
              value={selectedCity}
              onChange={(e) => onCityChange(e.target.value)}
              className="w-full px-3 py-2 bg-stone-50 border border-stone-300 rounded-lg text-stone-900 focus:outline-none focus:ring-1 focus:ring-[#143D30]"
            >
              <option value="all">{t.all_cities}</option>
              <option value="الرباط">الرباط / Rabat</option>
              <option value="مراكش">مراكش / Marrakech</option>
              <option value="الدار البيضاء">الدار البيضاء / Casablanca</option>
              <option value="تارودانت">تارودانت / Taroudant</option>
              <option value="فاس">فاس / Fès</option>
              <option value="طنجة">طنجة / Tanger</option>
              <option value="أكادير">أكادير / Agadir</option>
            </select>
          </div>

          {/* Type filter */}
          <div>
            <label className="block text-xs font-semibold text-stone-700 mb-1">
              {t.filter_by_type}
            </label>
            <select
              value={selectedType}
              onChange={(e) => onTypeChange(e.target.value)}
              className="w-full px-3 py-2 bg-stone-50 border border-stone-300 rounded-lg text-stone-900 focus:outline-none focus:ring-1 focus:ring-[#143D30]"
            >
              <option value="all">{t.all_types}</option>
              <option value="engineering_school">مدارس المهندسين / Grandes écoles</option>
              <option value="faculty">الكليات الجامعية / Facultés</option>
              <option value="business_school">مدارس التجارة والتسيير / Commerce & Gestion</option>
            </select>
          </div>

          {/* Verification status filter */}
          <div>
            <label className="block text-xs font-semibold text-stone-700 mb-1">
              {t.filter_by_status}
            </label>
            <select
              value={selectedStatus}
              onChange={(e) => onStatusChange(e.target.value)}
              className="w-full px-3 py-2 bg-stone-50 border border-stone-300 rounded-lg text-stone-900 focus:outline-none focus:ring-1 focus:ring-[#143D30]"
            >
              <option value="all">{t.all_statuses}</option>
              <option value="verified">{t.status_verified}</option>
              <option value="needs_review">{t.status_needs_review}</option>
              <option value="demo">{t.status_demo}</option>
            </select>
          </div>
        </div>
      )}

      {/* Active Filter Badges Strip & Quick Reset */}
      {hasActiveFilters && (
        <div className="px-4 sm:px-5 py-2.5 bg-stone-50/70 border-t border-stone-100 flex flex-wrap items-center justify-between gap-2 text-xs">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="text-stone-500 font-medium me-1">{isAr ? 'الفلاتر النشطة:' : 'Filtres actifs :'}</span>

            {searchQuery && (
              <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-white border border-stone-200 text-stone-800 text-[11px]">
                <span>بحث: «{searchQuery}»</span>
                <button onClick={() => onSearchChange('')} className="hover:text-red-600">
                  <X className="w-3 h-3" />
                </button>
              </span>
            )}

            {selectedDiscipline !== 'all' && (
              <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-emerald-50 border border-emerald-200 text-emerald-800 text-[11px] font-semibold">
                <span>
                  {isAr
                    ? DISCIPLINE_OPTIONS.find((d) => d.key === selectedDiscipline)?.label_ar
                    : DISCIPLINE_OPTIONS.find((d) => d.key === selectedDiscipline)?.label_fr}
                </span>
                <button onClick={() => onDisciplineChange('all')} className="hover:text-red-600">
                  <X className="w-3 h-3" />
                </button>
              </span>
            )}

            {selectedCity !== 'all' && (
              <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-white border border-stone-200 text-stone-800 text-[11px]">
                <span>مدينة: {selectedCity}</span>
                <button onClick={() => onCityChange('all')} className="hover:text-red-600">
                  <X className="w-3 h-3" />
                </button>
              </span>
            )}

            {selectedType !== 'all' && (
              <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-white border border-stone-200 text-stone-800 text-[11px]">
                <span>نوع: {selectedType}</span>
                <button onClick={() => onTypeChange('all')} className="hover:text-red-600">
                  <X className="w-3 h-3" />
                </button>
              </span>
            )}

            {selectedStatus !== 'all' && (
              <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-white border border-stone-200 text-stone-800 text-[11px]">
                <span>حالة: {selectedStatus}</span>
                <button onClick={() => onStatusChange('all')} className="hover:text-red-600">
                  <X className="w-3 h-3" />
                </button>
              </span>
            )}

            {viewFilter !== 'all' && (
              <span className="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full bg-white border border-stone-200 text-stone-800 text-[11px]">
                <span>عرض: {viewFilter === 'universities' ? 'الجامعات' : 'المؤسسات'}</span>
                <button onClick={() => onViewFilterChange('all')} className="hover:text-red-600">
                  <X className="w-3 h-3" />
                </button>
              </span>
            )}
          </div>

          <button
            type="button"
            onClick={onResetFilters}
            className="inline-flex items-center gap-1 text-[11px] font-semibold text-[#B43425] hover:underline"
          >
            <RotateCcw className="w-3 h-3" />
            <span>{t.clear_filters}</span>
          </button>
        </div>
      )}
    </div>
  );
};
