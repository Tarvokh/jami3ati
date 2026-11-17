import React from 'react';
import { Language } from '../types';
import {
  Search,
  Map,
  MessageSquare,
  ShieldCheck,
  ShieldAlert,
} from 'lucide-react';

interface Props {
  lang: Language;
  onOpenSearch: () => void;
  onOpenMap: () => void;
  onOpenFeedback: () => void;
  onOpenModerator: () => void;
  onOpenReport: () => void;
  onOpenHelpCenter?: () => void;
  isModeratorOpen: boolean;
  activeSection?: string;
}

export const BottomNavBar: React.FC<Props> = ({
  lang,
  onOpenSearch,
  onOpenMap,
  onOpenFeedback,
  onOpenModerator,
  onOpenReport,
  onOpenHelpCenter,
  isModeratorOpen,
}) => {
  const isAr = lang === 'ar';

  return (
    <nav
      className="md:hidden fixed bottom-0 inset-x-0 z-40 bg-white/95 backdrop-blur-md border-t border-stone-200/90 px-2 py-1 shadow-[0_-4px_20px_rgba(0,0,0,0.08)] pb-[calc(0.25rem+env(safe-area-inset-bottom))]"
      aria-label="شريط التنقل السريع للهواتف"
    >
      <div className="max-w-md mx-auto grid grid-cols-5 gap-1 items-center">
        {/* 1. Directory & Search */}
        <button
          type="button"
          onClick={onOpenSearch}
          aria-label={isAr ? 'البحث والدليل' : 'Recherche et annuaire'}
          tabIndex={0}
          className="flex flex-col items-center justify-center py-1.5 px-1 rounded-xl text-stone-600 hover:text-[#143D30] active:scale-95 transition-all focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none"
        >
          <Search className="w-5 h-5 text-[#143D30]" />
          <span className="text-[10px] font-medium mt-0.5 leading-tight truncate">
            {isAr ? 'الدليل' : 'Recherche'}
          </span>
        </button>

        {/* 2. Interactive Map */}
        <button
          type="button"
          onClick={onOpenMap}
          aria-label={isAr ? 'الخريطة التفاعلية' : 'Carte interactive'}
          tabIndex={0}
          className="flex flex-col items-center justify-center py-1.5 px-1 rounded-xl text-stone-600 hover:text-[#143D30] active:scale-95 transition-all focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none"
        >
          <Map className="w-5 h-5 text-[#143D30]" />
          <span className="text-[10px] font-medium mt-0.5 leading-tight truncate">
            {isAr ? 'الخريطة' : 'Carte'}
          </span>
        </button>

        {/* 3. Student Feedback */}
        <button
          type="button"
          onClick={onOpenFeedback}
          aria-label={isAr ? 'آراء وتقييمات الطلبة' : 'Avis étudiants'}
          tabIndex={0}
          className="flex flex-col items-center justify-center py-1.5 px-1 rounded-xl text-stone-600 hover:text-[#143D30] active:scale-95 transition-all focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none"
        >
          <MessageSquare className="w-5 h-5 text-[#143D30]" />
          <span className="text-[10px] font-medium mt-0.5 leading-tight truncate">
            {isAr ? 'آراء الطلبة' : 'Avis'}
          </span>
        </button>

        {/* 4. Help Center / Moderator */}
        {onOpenHelpCenter ? (
          <button
            type="button"
            onClick={onOpenHelpCenter}
            aria-label={isAr ? 'مركز المساعدة ودليل الاستخدام' : 'Centre d’aide'}
            tabIndex={0}
            className="flex flex-col items-center justify-center py-1.5 px-1 rounded-xl text-stone-600 hover:text-[#143D30] active:scale-95 transition-all focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none"
          >
            <ShieldCheck className="w-5 h-5 text-[#143D30]" />
            <span className="text-[10px] font-medium mt-0.5 leading-tight truncate">
              {isAr ? 'المساعدة' : 'Aide'}
            </span>
          </button>
        ) : (
          <button
            type="button"
            onClick={onOpenModerator}
            aria-label={isAr ? 'فضاء المكلفين بالموقع' : 'Espace modérateur'}
            tabIndex={0}
            className={`flex flex-col items-center justify-center py-1.5 px-1 rounded-xl active:scale-95 transition-all focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none ${
              isModeratorOpen
                ? 'text-emerald-800 font-bold bg-emerald-50'
                : 'text-stone-600 hover:text-[#143D30]'
            }`}
          >
            <ShieldCheck className="w-5 h-5 text-emerald-700" />
            <span className="text-[10px] font-semibold mt-0.5 leading-tight truncate">
              {isAr ? 'المشرف' : 'Espace'}
            </span>
          </button>
        )}

        {/* 5. Report Inaccuracy */}
        <button
          type="button"
          onClick={onOpenReport}
          aria-label={isAr ? 'الإبلاغ عن معلومة غير صحيحة' : 'Signaler une erreur'}
          tabIndex={0}
          className="flex flex-col items-center justify-center py-1.5 px-1 rounded-xl text-[#B43425] hover:text-[#8a2216] active:scale-95 transition-all focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none"
        >
          <ShieldAlert className="w-5 h-5 text-[#B43425]" />
          <span className="text-[10px] font-medium mt-0.5 leading-tight truncate">
            {isAr ? 'إبلاغ' : 'Signaler'}
          </span>
        </button>
      </div>
    </nav>
  );
};
