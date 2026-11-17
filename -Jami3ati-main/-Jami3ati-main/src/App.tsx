import React, { useState, useEffect, useMemo } from 'react';
import {
  initialUniversities,
  initialInstitutions,
  initialReports,
  initialStudentFeedbacks,
  DISCIPLINE_OPTIONS,
} from './data/seedData';
import { University, Institution, InaccuracyReport, StudentFeedback, Language, DataStatus, DisciplineKey } from './types';
import { submitReport, type ReportRecord } from './lib/api';
import { translations } from './data/translations';
import { UniversityCard } from './components/UniversityCard';
import { InstitutionCard } from './components/InstitutionCard';
import { DetailModal } from './components/DetailModal';
import { ReportModal } from './components/ReportModal';
import { AnnouncementsSection } from './components/AnnouncementsSection';
import { AcademicAlertsSubscription } from './components/AcademicAlertsSubscription';
import { shareItem } from './utils/shareUtils';
import { AboutDataSection } from './components/AboutDataSection';
import { AdminPanel } from './components/AdminPanel';
import { InteractiveMap } from './components/InteractiveMap';
import { ModeratorPortal } from './components/ModeratorPortal';
import { StudentFeedbackSection } from './components/StudentFeedbackSection';
import { FilterPanel } from './components/FilterPanel';
import { SearchBar } from './components/SearchBar';
import { BottomNavBar } from './components/BottomNavBar';
import { HelpCenterModal } from './components/HelpCenterModal';
import { useEscapeKey } from './hooks/useEscapeKey';
import { CompareModal, ComparableItem } from './components/CompareModal';
import { CompareTray } from './components/CompareTray';
import { useRecentSearches } from './hooks/useRecentSearches';
import {
  Search,
  Building2,
  GraduationCap,
  ShieldAlert,
  Globe,
  RotateCcw,
  CheckCircle2,
  HelpCircle,
  ExternalLink,
  Map,
  ShieldCheck,
  Menu,
  X,
  Compass,
  History,
  Trash2,
  Scale,
} from 'lucide-react';

const REPORT_STATUSES: readonly InaccuracyReport['status'][] = [
  'pending',
  'reviewed',
  'approved',
  'rejected',
];

/** Mirrors a stored row into the UI type without inventing missing fields. */
function mapStoredReport(stored: ReportRecord): InaccuracyReport {
  const status = REPORT_STATUSES.includes(stored.status as InaccuracyReport['status'])
    ? (stored.status as InaccuracyReport['status'])
    : 'pending';
  return {
    id: stored.id,
    target_id: stored.target_id,
    target_name: stored.target_name,
    field_name: stored.field_name,
    current_value: stored.current_value ?? '',
    proposed_correction: stored.proposed_correction,
    source_proof_url: stored.source_proof_url,
    user_notes: stored.user_notes ?? undefined,
    reporter_email: stored.reporter_email ?? undefined,
    created_at: stored.created_at,
    status,
  };
}

export default function App() {
  const [lang, setLang] = useState<Language>('ar');
  const [universities, setUniversities] = useState<University[]>(initialUniversities);
  const [institutions, setInstitutions] = useState<Institution[]>(initialInstitutions);
  const [reports, setReports] = useState<InaccuracyReport[]>(initialReports);
  const [feedbacks, setFeedbacks] = useState<StudentFeedback[]>(initialStudentFeedbacks);

  // View state: 'all' | 'universities' | 'institutions'
  const [viewFilter, setViewFilter] = useState<'all' | 'universities' | 'institutions'>('all');

  // Interactive map visibility toggle
  const [showMap, setShowMap] = useState(true);

  // Moderator portal visibility toggle
  const [showModeratorPortal, setShowModeratorPortal] = useState(false);

  // Demo Admin Panel modal toggle
  const [showAdminPanel, setShowAdminPanel] = useState(false);

  // Help Center modal toggle
  const [showHelpCenter, setShowHelpCenter] = useState(false);

  // Mobile menu open state
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  // Search and filter parameters
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCity, setSelectedCity] = useState('all');
  const [selectedStatus, setSelectedStatus] = useState<string>('all');
  const [selectedType, setSelectedType] = useState<string>('all');
  const [selectedDiscipline, setSelectedDiscipline] = useState<DisciplineKey | 'all'>('all');

  // Recent searches hook (persisted in localStorage)
  const { recentSearches, addRecentSearch, removeRecentSearch, clearRecentSearches } = useRecentSearches();

  // Compare feature state
  const [comparedItems, setComparedItems] = useState<ComparableItem[]>([]);
  const [compareModalOpen, setCompareModalOpen] = useState(false);

  // Modal states
  const [selectedItem, setSelectedItem] = useState<{
    item: University | Institution;
    type: 'university' | 'institution';
  } | null>(null);

  const [reportModalOpen, setReportModalOpen] = useState(false);
  const [reportTargetItem, setReportTargetItem] = useState<University | Institution | null>(null);

  const t = translations[lang];
  const isAr = lang === 'ar';

  // Toggle item in comparison list (max 3 items)
  const handleToggleCompare = (item: University | Institution, type: 'university' | 'institution') => {
    setComparedItems((prev) => {
      const exists = prev.some((c) => c.item.id === item.id);
      if (exists) {
        return prev.filter((c) => c.item.id !== item.id);
      }
      if (prev.length >= 3) {
        alert(
          isAr
            ? 'يمكنك مقارنة حتى 3 مؤسسات في نفس الوقت فقط.'
            : 'Vous pouvez comparer jusqu’à 3 établissements au maximum.'
        );
        return prev;
      }
      return [...prev, { item, type }];
    });
  };

  const handleRemoveCompareItem = (id: string) => {
    setComparedItems((prev) => prev.filter((c) => c.item.id !== id));
  };

  const handleClearCompare = () => {
    setComparedItems([]);
    setCompareModalOpen(false);
  };

  // Check if any modal dialog is currently active
  const isAnyModalActive = Boolean(
    selectedItem || reportModalOpen || showModeratorPortal || compareModalOpen || showAdminPanel || showHelpCenter
  );

  // Prevent background scrolling on document.body whenever any modal is active
  useEffect(() => {
    if (isAnyModalActive) {
      const originalOverflow = document.body.style.overflow;
      const originalPaddingRight = document.body.style.paddingRight;
      const scrollbarWidth = window.innerWidth - document.documentElement.clientWidth;

      document.body.style.overflow = 'hidden';
      if (scrollbarWidth > 0) {
        document.body.style.paddingRight = `${scrollbarWidth}px`;
      }

      return () => {
        document.body.style.overflow = originalOverflow;
        document.body.style.paddingRight = originalPaddingRight;
      };
    } else {
      document.body.style.overflow = 'unset';
    }
  }, [isAnyModalActive]);

  // Global Escape key listener that closes whichever modal is open
  useEscapeKey(() => {
    if (selectedItem) {
      setSelectedItem(null);
    } else if (reportModalOpen) {
      setReportModalOpen(false);
    } else if (showModeratorPortal) {
      setShowModeratorPortal(false);
    } else if (compareModalOpen) {
      setCompareModalOpen(false);
    } else if (showAdminPanel) {
      setShowAdminPanel(false);
    } else if (showHelpCenter) {
      setShowHelpCenter(false);
    }
  }, isAnyModalActive);

  // Toast notification state
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  const showToast = (msg: string) => {
    setToastMessage(msg);
    setTimeout(() => setToastMessage(null), 3500);
  };

  // Deep linking: check URL search params for ?item=ID on load and open detail modal automatically
  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const itemId = params.get('item');
    if (itemId) {
      const foundUniv = universities.find((u) => u.id === itemId || u.code.toLowerCase() === itemId.toLowerCase());
      if (foundUniv) {
        setSelectedItem({ item: foundUniv, type: 'university' });
        return;
      }
      const foundInst = institutions.find((i) => i.id === itemId || i.code.toLowerCase() === itemId.toLowerCase());
      if (foundInst) {
        setSelectedItem({ item: foundInst, type: 'institution' });
      }
    }
  }, [universities, institutions]);

  // Synchronize document title and direction when language changes
  useEffect(() => {
    document.documentElement.lang = lang;
    document.documentElement.dir = isAr ? 'rtl' : 'ltr';
    document.title = isAr
      ? 'جامعتي | Jami3ati — دليل الجامعات والمؤسسات بالمغرب'
      : 'Jami3ati | Guide des universités et établissements au Maroc';
  }, [lang, isAr]);

  // Language toggle handler
  const toggleLanguage = () => {
    setLang((prev) => (prev === 'ar' ? 'fr' : 'ar'));
  };

  // Open report modal for a specific item
  const handleOpenReport = (item?: University | Institution) => {
    setReportTargetItem(item || null);
    setReportModalOpen(true);
    setMobileMenuOpen(false);
  };

  // Submit report handler — the row is stored by the API; the local list only
  // mirrors what the server returned, and the modal shows the server id.
  const handleAddReport = async (
    newReportData: Omit<InaccuracyReport, 'id' | 'created_at' | 'status'>
  ): Promise<InaccuracyReport> => {
    const stored = await submitReport({
      target_id: newReportData.target_id,
      target_name: newReportData.target_name,
      field_name: newReportData.field_name,
      current_value: newReportData.current_value ?? null,
      proposed_correction: newReportData.proposed_correction,
      source_proof_url: newReportData.source_proof_url,
      user_notes: newReportData.user_notes ?? null,
      reporter_email: newReportData.reporter_email ?? null,
    });

    const mapped = mapStoredReport(stored);
    setReports((prev) => [mapped, ...prev.filter((r) => r.id !== mapped.id)]);
    return mapped;
  };

  // Update report status in demo admin / moderator portal
  const handleUpdateReportStatus = (reportId: string, newStatus: InaccuracyReport['status']) => {
    setReports((prev) =>
      prev.map((r) => (r.id === reportId ? { ...r, status: newStatus } : r))
    );
  };

  // Submit student feedback handler (pre-moderation)
  const handleAddFeedback = (newFb: Omit<StudentFeedback, 'id' | 'created_at' | 'status' | 'moderated_at'>) => {
    const newEntry: StudentFeedback = {
      ...newFb,
      id: `fb-${Date.now().toString().slice(-4)}`,
      created_at: new Date().toISOString().split('T')[0],
      status: 'pending', // enters pre-moderation queue
    };
    setFeedbacks((prev) => [newEntry, ...prev]);
  };

  // Update student feedback status from moderator portal
  const handleUpdateFeedbackStatus = (feedbackId: string, newStatus: 'approved' | 'rejected') => {
    setFeedbacks((prev) =>
      prev.map((f) =>
        f.id === feedbackId
          ? { ...f, status: newStatus, moderated_at: new Date().toISOString().split('T')[0] }
          : f
      )
    );
  };

  // Add institution handler
  const handleAddInstitution = (newInst: Institution) => {
    setInstitutions((prev) => [newInst, ...prev]);
  };

  // Update institution from moderator portal
  const handleUpdateInstitution = (updated: Institution) => {
    setInstitutions((prev) =>
      prev.map((i) => (i.id === updated.id ? updated : i))
    );
  };

  // Update university from moderator portal
  const handleUpdateUniversity = (updated: University) => {
    setUniversities((prev) =>
      prev.map((u) => (u.id === updated.id ? updated : u))
    );
  };

  // Listen for '/' key to quickly focus the main search input (Desktop shortcut)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (
        e.key === '/' &&
        !['INPUT', 'TEXTAREA', 'SELECT'].includes((e.target as HTMLElement)?.tagName)
      ) {
        e.preventDefault();
        const input = document.getElementById('main-search-input') as HTMLInputElement | null;
        if (input) {
          input.focus();
          input.scrollIntoView({ behavior: 'smooth', block: 'center' });
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // Filtered universities
  const filteredUniversities = useMemo(() => {
    return universities.filter((u) => {
      const q = searchQuery.toLowerCase().trim();
      const matchQuery =
        !q ||
        u.name_ar.toLowerCase().includes(q) ||
        u.name_fr.toLowerCase().includes(q) ||
        u.code.toLowerCase().includes(q) ||
        u.city_ar.toLowerCase().includes(q) ||
        u.city_fr.toLowerCase().includes(q);

      const matchCity =
        selectedCity === 'all' ||
        u.city_ar === selectedCity ||
        u.city_fr === selectedCity;

      const matchStatus =
        selectedStatus === 'all' || u.data_status === selectedStatus;

      // If a specific discipline is active, show universities that host institutions with this discipline
      const matchDiscipline =
        selectedDiscipline === 'all' ||
        institutions.some(
          (inst) => inst.university_id === u.id && inst.disciplines?.includes(selectedDiscipline)
        );

      const matchType = selectedType === 'all';

      return matchQuery && matchCity && matchStatus && matchType && matchDiscipline;
    });
  }, [universities, institutions, searchQuery, selectedCity, selectedStatus, selectedType, selectedDiscipline]);

  // Filtered institutions
  const filteredInstitutions = useMemo(() => {
    return institutions.filter((inst) => {
      const q = searchQuery.toLowerCase().trim();
      const matchQuery =
        !q ||
        inst.name_ar.toLowerCase().includes(q) ||
        inst.name_fr.toLowerCase().includes(q) ||
        inst.code.toLowerCase().includes(q) ||
        inst.university_name_ar.toLowerCase().includes(q) ||
        inst.university_name_fr.toLowerCase().includes(q) ||
        inst.city_ar.toLowerCase().includes(q) ||
        inst.city_fr.toLowerCase().includes(q) ||
        inst.programs_summary_ar.some((p) => p.toLowerCase().includes(q)) ||
        inst.programs_summary_fr.some((p) => p.toLowerCase().includes(q));

      const matchCity =
        selectedCity === 'all' ||
        inst.city_ar === selectedCity ||
        inst.city_fr === selectedCity;

      const matchStatus =
        selectedStatus === 'all' || inst.data_status === selectedStatus;

      const matchType =
        selectedType === 'all' || inst.type === selectedType;

      const matchDiscipline =
        selectedDiscipline === 'all' ||
        Boolean(inst.disciplines?.includes(selectedDiscipline));

      return matchQuery && matchCity && matchStatus && matchType && matchDiscipline;
    });
  }, [institutions, searchQuery, selectedCity, selectedStatus, selectedType, selectedDiscipline]);

  // Discipline counts for active dataset
  const disciplineCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    DISCIPLINE_OPTIONS.forEach((disc) => {
      counts[disc.key] = institutions.filter((inst) =>
        inst.disciplines?.includes(disc.key)
      ).length;
    });
    return counts;
  }, [institutions]);

  const totalResultsCount =
    (viewFilter === 'all' || viewFilter === 'universities' ? filteredUniversities.length : 0) +
    (viewFilter === 'all' || viewFilter === 'institutions' ? filteredInstitutions.length : 0);

  const resetFilters = () => {
    setSearchQuery('');
    setSelectedCity('all');
    setSelectedStatus('all');
    setSelectedType('all');
    setSelectedDiscipline('all');
    setViewFilter('all');
  };

  return (
    <div className="min-h-screen flex flex-col gap-y-12 bg-[#FBF9F5] text-[#1C1917] pb-16 md:pb-0 w-full max-w-full overflow-x-hidden">
      {/* Disclaimer Top Banner */}
      <div className="bg-[#143D30] text-emerald-100 text-xs py-2 px-4 text-center border-b border-[#0f2e24]">
        <div className="max-w-6xl mx-auto flex items-center justify-center gap-2">
          <HelpCircle className="w-3.5 h-3.5 shrink-0 text-emerald-300" />
          <span className="font-normal">{t.disclaimer_banner}</span>
        </div>
      </div>

      {/* Top Bar Contract: 3 zones, 1 row */}
      <header className="sticky top-0 z-40 bg-white/95 backdrop-blur-md border-b border-stone-200">
        <div className="max-w-6xl mx-auto px-4 sm:px-6 h-16 flex items-center justify-between gap-4 sm:gap-8">
          {/* Zone 1: Single text element wordmark */}
          <a
            href="/"
            className="text-base sm:text-lg font-bold tracking-tight text-[#143D30] whitespace-nowrap shrink-0 flex items-center gap-2 focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded-lg p-1"
            aria-label={isAr ? 'الصفحة الرئيسية جامعتي' : 'Accueil Jami3ati'}
            tabIndex={0}
          >
            <div className="w-8 h-8 rounded-lg bg-[#143D30] text-white flex items-center justify-center font-bold text-sm">
              ج
            </div>
            <span>{isAr ? 'جامعتي | Jami3ati' : 'Jami3ati | جامعتي'}</span>
          </a>

          {/* Zone 2: Streamlined core nav links (desktop) */}
          <nav className="hidden lg:flex items-center gap-3 xl:gap-5 text-sm font-medium text-stone-600" aria-label="القائمة الرئيسية">
            <a
              href="#directory"
              className="hover:text-[#143D30] transition-colors whitespace-nowrap shrink-0 focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-1 py-0.5"
              aria-label={t.nav_directory}
              tabIndex={0}
            >
              {t.nav_directory}
            </a>
            <a
              href="#interactive-map"
              className="hover:text-[#143D30] transition-colors whitespace-nowrap shrink-0 flex items-center gap-1 focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-1 py-0.5"
              aria-label={t.nav_map}
              tabIndex={0}
            >
              <Compass className="w-3.5 h-3.5 text-[#143D30]" />
              <span>{t.nav_map}</span>
            </a>
            <a
              href="#student-feedback"
              className="hover:text-[#143D30] transition-colors whitespace-nowrap shrink-0 focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-1 py-0.5"
              aria-label={t.nav_feedback}
              tabIndex={0}
            >
              {t.nav_feedback}
            </a>
            <a
              href="#announcements"
              className="hover:text-[#143D30] transition-colors whitespace-nowrap shrink-0 focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-1 py-0.5"
              aria-label={t.nav_announcements}
              tabIndex={0}
            >
              {t.nav_announcements}
            </a>
            <a
              href="#about-data"
              className="hover:text-[#143D30] transition-colors whitespace-nowrap shrink-0 focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-1 py-0.5"
              aria-label={t.nav_about_data}
              tabIndex={0}
            >
              {t.nav_about_data}
            </a>

            {/* More / Portals Dropdown */}
            <div className="relative group">
              <button
                type="button"
                className="hover:text-[#143D30] transition-colors whitespace-nowrap shrink-0 flex items-center gap-1 text-stone-700 font-semibold focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-1.5 py-0.5"
                aria-label="المزيد من الخيارات والأقسام"
                tabIndex={0}
              >
                <span>{isAr ? 'المزيد والفضاءات' : 'Plus'}</span>
              </button>

              <div className="absolute top-full end-0 mt-1 w-56 bg-white rounded-xl shadow-xl border border-stone-200 py-2 hidden group-hover:block group-focus-within:block z-50 text-xs">
                <button
                  type="button"
                  onClick={() => setShowHelpCenter(true)}
                  className="w-full text-start px-4 py-2 hover:bg-stone-50 text-stone-800 flex items-center gap-2 font-medium"
                >
                  <HelpCircle className="w-4 h-4 text-amber-600" />
                  <span>{isAr ? 'مركز المساعدة ودليل الاستخدام' : 'Centre d’aide'}</span>
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setShowModeratorPortal((prev) => !prev);
                    setTimeout(() => {
                      const el = document.getElementById('moderator-portal');
                      if (el) el.scrollIntoView({ behavior: 'smooth' });
                    }, 50);
                  }}
                  className="w-full text-start px-4 py-2 hover:bg-stone-50 text-emerald-800 flex items-center gap-2 font-medium"
                >
                  <ShieldCheck className="w-4 h-4 text-emerald-700" />
                  <span>{t.nav_moderator}</span>
                </button>
                <button
                  type="button"
                  onClick={() => setShowAdminPanel(true)}
                  className="w-full text-start px-4 py-2 hover:bg-stone-50 text-stone-800 flex items-center gap-2 font-medium"
                >
                  <ShieldCheck className="w-4 h-4 text-[#143D30]" />
                  <span>{t.admin_title}</span>
                </button>
                <div className="border-t border-stone-100 my-1" />
                <button
                  type="button"
                  onClick={() => handleOpenReport()}
                  className="w-full text-start px-4 py-2 hover:bg-red-50 text-[#B43425] flex items-center gap-2 font-medium"
                >
                  <ShieldAlert className="w-4 h-4 text-[#B43425]" />
                  <span>{t.report_inaccuracy}</span>
                </button>
              </div>
            </div>
          </nav>

          {/* Zone 3: Language Switcher and quick desktop CTA */}
          <div className="flex items-center gap-2 sm:gap-3 shrink-0">
            <button
              onClick={toggleLanguage}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-semibold rounded-lg bg-stone-100 hover:bg-stone-200 text-stone-800 transition-colors whitespace-nowrap shrink-0 border border-stone-200/80 focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none"
              title="Changer de langue / تبديل اللغة"
              aria-label={isAr ? 'تغيير اللغة إلى الفرنسية' : 'Changer la langue en arabe'}
              tabIndex={0}
            >
              <Globe className="w-3.5 h-3.5 text-[#143D30]" />
              <span>{t.switch_lang}</span>
            </button>
          </div>
        </div>
      </header>

      {/* Hero Marquee Section */}
      <section className="relative overflow-hidden bg-[#FAF7F0] border-b border-stone-200 py-12 sm:py-16 w-full max-w-full overflow-x-hidden">
        <div className="max-w-6xl mx-auto px-4 sm:px-6 w-full">
          <div className="grid grid-cols-1 lg:grid-cols-12 gap-8 items-center w-full">
            {/* Left/Right Text Column */}
            <div className="lg:col-span-7 space-y-4 w-full">
              <div className="inline-flex items-center gap-2 px-3 py-1 rounded-md bg-[#143D30]/10 text-[#143D30] text-xs font-semibold">
                <CheckCircle2 className="w-4 h-4 text-[#143D30]" />
                <span>دليل وطني استكشافي بالبيانات المفتوحة</span>
              </div>

              <h1 className="text-3xl sm:text-4xl lg:text-5xl font-extrabold text-stone-900 tracking-tight leading-tight">
                {t.hero_title}
              </h1>

              <p className="text-sm sm:text-base text-stone-600 leading-relaxed max-w-xl">
                {t.hero_subtitle}
              </p>

              {/* Search Box in Hero with Live Autocomplete */}
              <div className="pt-2 w-full">
                <SearchBar
                  searchQuery={searchQuery}
                  onSearchChange={setSearchQuery}
                  universities={universities}
                  institutions={institutions}
                  recentSearches={recentSearches}
                  onAddRecentSearch={addRecentSearch}
                  onRemoveRecentSearch={removeRecentSearch}
                  onClearRecentSearches={clearRecentSearches}
                  onSelectSuggestion={(item, type) => {
                    setSelectedItem({ item, type });
                  }}
                  lang={lang}
                />

                {/* Quick discipline & institution tags */}
                <div className="flex flex-wrap items-center gap-2 mt-3 text-xs text-stone-500 w-full">
                  <span className="font-semibold text-stone-700">تخصصات سريعة:</span>
                  <button
                    type="button"
                    onClick={() => {
                      setSelectedDiscipline('engineering');
                      document.getElementById('directory')?.scrollIntoView({ behavior: 'smooth' });
                    }}
                    className="hover:text-[#143D30] underline underline-offset-2 font-medium focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-0.5"
                    aria-label="تصفية تخصص هندسة وتكنولوجيا"
                    tabIndex={0}
                  >
                    هندسة وتكنولوجيا
                  </button>
                  <span aria-hidden="true">·</span>
                  <button
                    type="button"
                    onClick={() => {
                      setSelectedDiscipline('medicine');
                      document.getElementById('directory')?.scrollIntoView({ behavior: 'smooth' });
                    }}
                    className="hover:text-[#143D30] underline underline-offset-2 font-medium focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-0.5"
                    aria-label="تصفية تخصص طب وصحة"
                    tabIndex={0}
                  >
                    طب وصحة
                  </button>
                  <span aria-hidden="true">·</span>
                  <button
                    type="button"
                    onClick={() => {
                      setSelectedDiscipline('economics');
                      document.getElementById('directory')?.scrollIntoView({ behavior: 'smooth' });
                    }}
                    className="hover:text-[#143D30] underline underline-offset-2 font-medium focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-0.5"
                    aria-label="تصفية تخصص اقتصاد وتدبير"
                    tabIndex={0}
                  >
                    اقتصاد وتدبير
                  </button>
                  <span aria-hidden="true">·</span>
                  <button
                    type="button"
                    onClick={() => {
                      setSelectedDiscipline('science_tech');
                      document.getElementById('directory')?.scrollIntoView({ behavior: 'smooth' });
                    }}
                    className="hover:text-[#143D30] underline underline-offset-2 font-medium focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-0.5"
                    aria-label="تصفية تخصص علوم وتقنيات"
                    tabIndex={0}
                  >
                    علوم وتقنيات
                  </button>
                </div>
              </div>

              {/* Rigor Statistics Strip */}
              <div className="pt-4 grid grid-cols-3 gap-3 border-t border-stone-200/80 max-w-lg w-full">
                <div>
                  <span className="font-mono text-xl sm:text-2xl font-bold text-[#143D30] block">
                    {universities.length + institutions.length}
                  </span>
                  <span className="text-xs text-stone-500">{t.stats_verified_institutions}</span>
                </div>
                <div>
                  <span className="font-mono text-xl sm:text-2xl font-bold text-[#143D30] block">
                    100%
                  </span>
                  <span className="text-xs text-stone-500">{t.stats_sources_verified}</span>
                </div>
                <div>
                  <span className="font-mono text-xl sm:text-2xl font-bold text-[#B43425] block">
                    0
                  </span>
                  <span className="text-xs text-stone-500">{t.stats_zero_fake_data}</span>
                </div>
              </div>
            </div>

            {/* Right/Left Image Banner Column */}
            <div className="lg:col-span-5 relative w-full">
              <div className="relative rounded-2xl overflow-hidden border border-stone-200 shadow-md w-full">
                <img
                  src="/src/assets/images/hero_academic_campus_1793985375880.jpg"
                  alt="Moroccan Academic Campus"
                  className="w-full h-72 sm:h-80 object-cover"
                  referrerPolicy="no-referrer"
                />
                <div className="absolute inset-0 bg-gradient-to-t from-stone-950/70 via-stone-950/20 to-transparent" />
                <div className="absolute bottom-4 left-4 right-4 text-white">
                  <span className="text-[11px] font-mono uppercase tracking-wider text-emerald-300 block">
                    Data Integrity Protocol
                  </span>
                  <p className="text-xs text-stone-200 mt-1 leading-snug">
                    لكل مؤسسة رابط تحقق رسمي وتاريخ مراجعة صريح ومصدر موثوق من data.gov.ma أو البوابات الجامعية الرسمية.
                  </p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      {/* Interactive Map Section (D3.js) */}
      <section id="interactive-map" className="py-10 sm:py-14 max-w-6xl mx-auto px-4 sm:px-6 w-full overflow-x-hidden">
        <div className="flex items-center justify-between mb-4 w-full">
          <div className="flex items-center gap-2">
            <Map className="w-5 h-5 text-[#143D30]" />
            <h2 className="text-xl font-bold text-stone-900">
              {isAr ? 'الخريطة التفاعلية للأقطاب الجامعية بالمغرب' : 'Carte interactive des pôles universitaires'}
            </h2>
          </div>
          <button
            onClick={() => setShowMap((prev) => !prev)}
            className="text-xs font-semibold text-[#143D30] hover:underline"
          >
            {showMap ? (isAr ? 'إخفاء الخريطة' : 'Masquer la carte') : (isAr ? 'إظهار الخريطة' : 'Afficher la carte')}
          </button>
        </div>

        {showMap && (
          <InteractiveMap
            universities={universities}
            institutions={institutions}
            selectedCity={selectedCity}
            onSelectCity={(city) => {
              setSelectedCity(city);
              // scroll gently to directory
              const el = document.getElementById('directory');
              if (el) el.scrollIntoView({ behavior: 'smooth' });
            }}
            onSelectInstitution={(inst) => setSelectedItem({ item: inst, type: 'institution' })}
            onSelectUniversity={(univ) => setSelectedItem({ item: univ, type: 'university' })}
            lang={lang}
          />
        )}
      </section>

      {/* Directory Explorer Section */}
      <section id="directory" className="py-10 sm:py-14 flex-1 w-full max-w-full overflow-x-hidden">
        <div className="max-w-6xl mx-auto px-4 sm:px-6 space-y-6 w-full">
          {/* Advanced Filter Panel with Study Disciplines & View Switching */}
          <FilterPanel
            lang={lang}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            selectedCity={selectedCity}
            onCityChange={setSelectedCity}
            selectedStatus={selectedStatus}
            onStatusChange={setSelectedStatus}
            selectedType={selectedType}
            onTypeChange={setSelectedType}
            selectedDiscipline={selectedDiscipline}
            onDisciplineChange={setSelectedDiscipline}
            viewFilter={viewFilter}
            onViewFilterChange={setViewFilter}
            totalResultsCount={totalResultsCount}
            universitiesCount={filteredUniversities.length}
            institutionsCount={filteredInstitutions.length}
            disciplineCounts={disciplineCounts}
            onResetFilters={resetFilters}
          />

          {/* Cards Display Grid */}
          {totalResultsCount === 0 ? (
            <div className="bg-white rounded-xl border border-stone-200 p-12 text-center space-y-3 w-full">
              <div className="w-12 h-12 bg-stone-100 text-stone-400 rounded-full flex items-center justify-center mx-auto">
                <Search className="w-6 h-6" />
              </div>
              <h3 className="text-base font-bold text-stone-900">{t.no_results_title}</h3>
              <p className="text-xs sm:text-sm text-stone-500 max-w-md mx-auto">
                {t.no_results_desc}
              </p>
              <button
                onClick={resetFilters}
                className="inline-flex items-center gap-1 px-4 py-2 bg-[#143D30] text-white text-xs font-medium rounded-lg hover:bg-[#0f2e24]"
              >
                <RotateCcw className="w-3.5 h-3.5" />
                <span>{t.clear_filters}</span>
              </button>
            </div>
          ) : (
            <div className="space-y-8 w-full">
              {/* Universities Section */}
              {(viewFilter === 'all' || viewFilter === 'universities') && filteredUniversities.length > 0 && (
                <div className="w-full">
                  <div className="flex items-center gap-2 mb-4 pb-2 border-b border-stone-200">
                    <Building2 className="w-4 h-4 text-[#143D30]" />
                    <h2 className="text-lg font-bold text-stone-900">
                      الجامعات العمومية بالمملكة ({filteredUniversities.length})
                    </h2>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 w-full">
                    {filteredUniversities.map((univ) => (
                      <UniversityCard
                        key={univ.id}
                        university={univ}
                        lang={lang}
                        onSelect={(u) => setSelectedItem({ item: u, type: 'university' })}
                        onReport={(u) => handleOpenReport(u)}
                        isCompared={comparedItems.some((c) => c.item.id === univ.id)}
                        onToggleCompare={(u) => handleToggleCompare(u, 'university')}
                        onShare={(u) => shareItem(u, lang, showToast)}
                      />
                    ))}
                  </div>
                </div>
              )}

              {/* Institutions Section */}
              {(viewFilter === 'all' || viewFilter === 'institutions') && filteredInstitutions.length > 0 && (
                <div className="w-full">
                  <div className="flex items-center gap-2 mb-4 pb-2 border-b border-stone-200">
                    <GraduationCap className="w-4 h-4 text-[#143D30]" />
                    <h2 className="text-lg font-bold text-stone-900">
                      المدارس العليا والكليات والمعاهد ({filteredInstitutions.length})
                    </h2>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6 w-full">
                    {filteredInstitutions.map((inst) => (
                      <InstitutionCard
                        key={inst.id}
                        institution={inst}
                        lang={lang}
                        onSelect={(i) => setSelectedItem({ item: i, type: 'institution' })}
                        onReport={(i) => handleOpenReport(i)}
                        isCompared={comparedItems.some((c) => c.item.id === inst.id)}
                        onToggleCompare={(i) => handleToggleCompare(i, 'institution')}
                        onShare={(i) => shareItem(i, lang, showToast)}
                      />
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </section>

      {/* Dedicated Moderator Portal Section (لوحة المكلفين بالموقع) */}
      {showModeratorPortal && (
        <ModeratorPortal
          universities={universities}
          institutions={institutions}
          reports={reports}
          feedbacks={feedbacks}
          lang={lang}
          onUpdateInstitution={handleUpdateInstitution}
          onUpdateUniversity={handleUpdateUniversity}
          onUpdateReportStatus={handleUpdateReportStatus}
          onUpdateFeedbackStatus={handleUpdateFeedbackStatus}
          onClosePortal={() => setShowModeratorPortal(false)}
        />
      )}

      {/* Student Feedback & Service Quality Section */}
      <StudentFeedbackSection
        feedbacks={feedbacks}
        universities={universities}
        institutions={institutions}
        lang={lang}
        onAddFeedback={handleAddFeedback}
      />

      {/* Verified Announcements & Dates Section — live public archive */}
      <AnnouncementsSection lang={lang} />

      {/* Mock Academic Deadline Alerts Subscription Section */}
      <section id="alerts-subscription" className="py-10 max-w-6xl mx-auto px-4 sm:px-6 w-full overflow-x-hidden">
        <AcademicAlertsSubscription lang={lang} />
      </section>

      {/* About the Data & Governance Section */}
      <AboutDataSection lang={lang} />

      {/* Footer */}
      <footer className="bg-stone-900 text-stone-300 py-12 border-t border-stone-800 text-xs sm:text-sm">
        <div className="max-w-6xl mx-auto px-4 sm:px-6">
          <div className="grid grid-cols-1 md:grid-cols-4 gap-8 mb-8 pb-8 border-b border-stone-800">
            <div className="md:col-span-2 space-y-2">
              <span className="text-base font-bold text-white block">جامعتي | Jami3ati</span>
              <p className="text-xs text-stone-400 leading-relaxed max-w-sm">
                دليل موحد للخدمات والمعلومات الجامعية بالمغرب. يعتمد على مصادر البيانات المفتوحة الرسمية المنشورة على data.gov.ma ومواقع الجامعات المعتمدة.
              </p>
              <div className="text-[11px] text-stone-400 pt-2">
                مشروع هاكاثون الابتكار الرقمي والبيانات المفتوحة بالمغرب 2026.
              </div>
            </div>

            <div>
              <span className="text-xs font-semibold text-white uppercase tracking-wider block mb-3">
                روابط التحقق الرسمية
              </span>
              <ul className="space-y-1.5 text-xs text-stone-400">
                <li>
                  <a href="https://data.gov.ma" target="_blank" rel="noopener noreferrer" className="hover:text-white flex items-center gap-1">
                    <span>بوابة البيانات المفتوحة (data.gov.ma)</span>
                    <ExternalLink className="w-3 h-3" />
                  </a>
                </li>
                <li>
                  <a href="https://www.enssup.gov.ma" target="_blank" rel="noopener noreferrer" className="hover:text-white flex items-center gap-1">
                    <span>قطاع التعليم العالي (enssup.gov.ma)</span>
                    <ExternalLink className="w-3 h-3" />
                  </a>
                </li>
                <li>
                  <a href="https://www.um5.ac.ma" target="_blank" rel="noopener noreferrer" className="hover:text-white flex items-center gap-1">
                    <span>جامعة محمد الخامس بالرباط</span>
                    <ExternalLink className="w-3 h-3" />
                  </a>
                </li>
                <li>
                  <a href="https://www.uca.ma" target="_blank" rel="noopener noreferrer" className="hover:text-white flex items-center gap-1">
                    <span>جامعة القاضي عياض بمراكش</span>
                    <ExternalLink className="w-3 h-3" />
                  </a>
                </li>
              </ul>
            </div>

            <div>
              <span className="text-xs font-semibold text-white uppercase tracking-wider block mb-3">
                المساهمة والشفافية
              </span>
              <ul className="space-y-1.5 text-xs text-stone-400">
                <li>
                  <button
                    onClick={() => setShowHelpCenter(true)}
                    className="hover:text-amber-300 text-start font-semibold text-amber-200/90 focus-visible:ring-2 focus-visible:ring-amber-400 focus-visible:outline-none rounded"
                    aria-label={isAr ? 'مركز المساعدة ودليل الاستخدام' : 'Centre d’aide'}
                    tabIndex={0}
                  >
                    {isAr ? 'مركز المساعدة ودليل الاستخدام' : 'Centre d’aide & Guide'}
                  </button>
                </li>
                <li>
                  <button
                    onClick={() => handleOpenReport()}
                    className="hover:text-amber-400 text-start focus-visible:ring-2 focus-visible:ring-amber-400 focus-visible:outline-none rounded"
                    aria-label={isAr ? 'الإبلاغ عن معلومة غير صحيحة' : 'Signaler une erreur'}
                    tabIndex={0}
                  >
                    الإبلاغ عن معلومة غير صحيحة
                  </button>
                </li>
                <li>
                  <button
                    onClick={() => {
                      setShowModeratorPortal(true);
                      setTimeout(() => {
                        const el = document.getElementById('moderator-portal');
                        if (el) el.scrollIntoView({ behavior: 'smooth' });
                      }, 50);
                    }}
                    className="hover:text-emerald-400 text-start focus-visible:ring-2 focus-visible:ring-emerald-400 focus-visible:outline-none rounded"
                    aria-label={isAr ? 'فضاء المكلفين بالموقع' : 'Espace modérateurs'}
                    tabIndex={0}
                  >
                    فضاء المكلفين بالموقع (Modérateurs)
                  </button>
                </li>
                <li>
                  <a
                    href="#about-data"
                    className="hover:text-white focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded"
                    aria-label={isAr ? 'ميثاق البيانات المفتوحة' : 'Charte des données ouvertes'}
                    tabIndex={0}
                  >
                    ميثاق البيانات المفتوحة
                  </a>
                </li>
                <li>
                  <button
                    onClick={() => setShowAdminPanel(true)}
                    className="hover:text-white text-start cursor-pointer focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded"
                    aria-label={isAr ? 'لوحة الإدارة التجريبية' : 'Panneau d’administration démo'}
                    tabIndex={0}
                  >
                    لوحة الإدارة التجريبية
                  </button>
                </li>
              </ul>
            </div>
          </div>

          <div className="flex flex-col sm:flex-row items-center justify-between gap-4 text-xs text-stone-400">
            <span>© 2026 جامعتي | Jami3ati — نموذج أولي مستقل غير رسمي (Hackathon Prototype).</span>
            <span className="text-[11px] text-stone-400">
              مبني وفق مبدأ الصفر تخمين والتحقق من الروابط الرسمية.
            </span>
          </div>
        </div>
      </footer>

      {/* Independent Help Center Modal */}
      <HelpCenterModal
        isOpen={showHelpCenter}
        onClose={() => setShowHelpCenter(false)}
        lang={lang}
      />

      {/* Independent Demo Admin Modal */}
      <AdminPanel
        isOpen={showAdminPanel}
        onClose={() => setShowAdminPanel(false)}
        universities={universities}
        institutions={institutions}
        reports={reports}
        lang={lang}
        onAddInstitution={handleAddInstitution}
        onUpdateReportStatus={handleUpdateReportStatus}
      />

      {/* Detail Modal */}
      {selectedItem && (
        <DetailModal
          item={selectedItem.item}
          itemType={selectedItem.type}
          lang={lang}
          onClose={() => setSelectedItem(null)}
          onReport={(item) => handleOpenReport(item)}
          isCompared={comparedItems.some((c) => c.item.id === selectedItem.item.id)}
          onToggleCompare={(item) => handleToggleCompare(item, selectedItem.type)}
        />
      )}

      {/* Report Inaccuracy Modal */}
      <ReportModal
        isOpen={reportModalOpen}
        targetItem={reportTargetItem}
        lang={lang}
        onClose={() => setReportModalOpen(false)}
        onSubmitReport={handleAddReport}
      />

      {/* Floating Comparison Tray */}
      <CompareTray
        items={comparedItems}
        lang={lang}
        onOpenCompare={() => setCompareModalOpen(true)}
        onRemoveItem={handleRemoveCompareItem}
        onClearAll={handleClearCompare}
      />

      {/* Comparison Modal */}
      {compareModalOpen && (
        <CompareModal
          items={comparedItems}
          lang={lang}
          onClose={() => setCompareModalOpen(false)}
          onRemoveItem={handleRemoveCompareItem}
          onClearAll={handleClearCompare}
        />
      )}

      {/* Fixed Bottom Navigation Bar for Mobile */}
      <BottomNavBar
        lang={lang}
        onOpenSearch={() => {
          const el = document.getElementById('directory');
          if (el) el.scrollIntoView({ behavior: 'smooth' });
          const input = document.getElementById('main-search-input') as HTMLInputElement | null;
          input?.focus();
        }}
        onOpenMap={() => {
          setShowMap(true);
          const el = document.getElementById('interactive-map');
          if (el) el.scrollIntoView({ behavior: 'smooth' });
        }}
        onOpenFeedback={() => {
          const el = document.getElementById('student-feedback');
          if (el) el.scrollIntoView({ behavior: 'smooth' });
        }}
        onOpenHelpCenter={() => setShowHelpCenter(true)}
        onOpenModerator={() => setShowModeratorPortal(true)}
        onOpenReport={() => handleOpenReport()}
        isModeratorOpen={showModeratorPortal}
      />

      {/* Global Share Toast Notification Banner */}
      {toastMessage && (
        <div className="fixed top-20 start-1/2 -translate-x-1/2 z-50 bg-stone-900/95 text-white px-5 py-3 rounded-2xl shadow-2xl border border-stone-700 text-xs sm:text-sm font-semibold flex items-center gap-2 animate-in fade-in slide-in-from-top-4 duration-200">
          <CheckCircle2 className="w-4.5 h-4.5 text-emerald-400 shrink-0" />
          <span>{toastMessage}</span>
        </div>
      )}
    </div>
  );
}
