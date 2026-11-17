import React from 'react';
import { University, Institution, Language } from '../types';
import { translations } from '../data/translations';
import { X, ExternalLink, MapPin, Phone, Mail, Building2, GraduationCap, AlertCircle, BookOpen, Share2 } from 'lucide-react';
import { SourceAttribution } from './SourceAttribution';
import { DataStatusBadge } from './DataStatusBadge';
import { ModalBackdrop } from './ModalBackdrop';
import { useEscapeKey } from '../hooks/useEscapeKey';
import { shareItem } from '../utils/shareUtils';

interface Props {
  item: University | Institution | null;
  itemType: 'university' | 'institution';
  lang: Language;
  onClose: () => void;
  onReport: (item: University | Institution) => void;
  isCompared?: boolean;
  onToggleCompare?: (item: University | Institution) => void;
}

export const DetailModal: React.FC<Props> = ({
  item,
  itemType,
  lang,
  onClose,
  onReport,
  isCompared = false,
  onToggleCompare,
}) => {
  const [imgError, setImgError] = React.useState(false);
  // Listen for Escape key globally to close this modal
  useEscapeKey(onClose, Boolean(item));

  if (!item) return null;
  const t = translations[lang];
  const isAr = lang === 'ar';

  const isUniv = itemType === 'university';
  const univ = isUniv ? (item as University) : null;
  const inst = !isUniv ? (item as Institution) : null;

  const displayName = lang === 'ar' ? item.name_ar : item.name_fr;
  const subName = lang === 'ar' ? item.name_fr : item.name_ar;
  const city = lang === 'ar' ? item.city_ar : item.city_fr;

  return (
    <ModalBackdrop onClose={onClose} ariaLabelledBy="modal-title">
      <div className="relative w-full max-w-3xl max-h-[90vh] my-auto flex flex-col bg-white rounded-2xl shadow-2xl border border-stone-200 overflow-hidden select-text animate-in zoom-in-95 duration-150">
        {/* Sticky/Fixed close button */}
        <button
          onClick={onClose}
          aria-label={t.close}
          tabIndex={0}
          className="absolute top-3.5 end-3.5 z-20 p-2 text-white/90 hover:text-white bg-black/50 hover:bg-black/70 rounded-full transition-colors shadow-sm focus-visible:ring-2 focus-visible:ring-white focus-visible:outline-none"
        >
          <X className="w-5 h-5" />
        </button>

        {/* Header banner */}
        <div className="relative h-44 sm:h-52 w-full overflow-hidden bg-stone-900 shrink-0">
          {!imgError ? (
            <img
              src={item.image_url}
              alt={displayName}
              onError={() => setImgError(true)}
              className="w-full h-full object-cover opacity-85"
              referrerPolicy="no-referrer"
            />
          ) : (
            <div className="w-full h-full bg-gradient-to-br from-[#143D30] to-[#091a14] flex flex-col items-center justify-center p-6 text-white">
              <span className="font-mono text-3xl font-black tracking-widest text-amber-300">
                {item.code}
              </span>
              <span className="text-sm text-stone-300 font-medium mt-1">
                {city}
              </span>
            </div>
          )}
          <div className="absolute inset-0 bg-gradient-to-t from-stone-950/85 via-stone-950/40 to-transparent" />

          <div className="absolute bottom-4 left-6 right-6 flex items-end justify-between gap-4">
            <div>
              <div className="flex items-center gap-2 mb-1.5">
                <span className="font-mono text-xs uppercase px-2 py-0.5 rounded bg-white/20 text-white font-medium">
                  {item.code}
                </span>
                <DataStatusBadge status={item.data_status} lang={lang} size="sm" />
              </div>
              <h2 id="modal-title" className="text-xl sm:text-2xl font-bold text-white tracking-tight">
                {displayName}
              </h2>
              <p className="text-xs sm:text-sm text-stone-300 font-normal mt-0.5">
                {subName}
              </p>
            </div>
          </div>
        </div>

        {/* Scrollable inner content container */}
        <div className="overflow-y-auto overscroll-contain p-6 sm:p-8 space-y-6 flex-1">
          {/* Context Navigation Breadcrumbs */}
          <nav aria-label="Breadcrumb" className="flex items-center gap-1.5 text-xs text-stone-500 bg-[#FAF7F0] px-3 py-2 rounded-xl border border-stone-200 w-full">
            <button
              type="button"
              onClick={onClose}
              className="hover:text-[#143D30] font-medium transition-colors focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-1"
              tabIndex={0}
            >
              {lang === 'ar' ? 'الدليل الرئيسي' : 'Annuaire'}
            </button>
            <span className="text-stone-300" aria-hidden="true">/</span>
            <button
              type="button"
              onClick={onClose}
              className="hover:text-[#143D30] font-medium transition-colors focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-1"
              tabIndex={0}
            >
              {city}
            </button>
            <span className="text-stone-300" aria-hidden="true">/</span>
            <span className="text-stone-900 font-bold truncate max-w-[200px]" aria-current="page">
              {displayName}
            </span>
          </nav>

          {/* Quick info row */}
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 pb-4 border-b border-stone-200">
            <div className="flex items-start gap-2.5 text-sm">
              <MapPin className="w-4 h-4 text-[#143D30] shrink-0 mt-0.5" />
              <div>
                <span className="text-xs text-stone-500 block">{t.filter_by_city}</span>
                <span className="font-medium text-stone-900">{city}</span>
                {inst && <span className="text-xs text-stone-500 block mt-0.5">{lang === 'ar' ? inst.campus_ar : inst.campus_fr}</span>}
              </div>
            </div>

            {inst && (
              <div className="flex items-start gap-2.5 text-sm">
                <Building2 className="w-4 h-4 text-[#143D30]" />
                <div>
                  <span className="text-xs text-stone-500 block">{t.parent_university}</span>
                  <span className="font-medium text-stone-900">
                    {lang === 'ar' ? inst.university_name_ar : inst.university_name_fr}
                  </span>
                </div>
              </div>
            )}

            {univ && (
              <div className="flex items-start gap-2.5 text-sm">
                <Building2 className="w-4 h-4 text-[#143D30]" />
                <div>
                  <span className="text-xs text-stone-500 block">سنة التأسيس / Fondation</span>
                  <span className="font-medium text-stone-900 font-mono">{univ.established_year}</span>
                </div>
              </div>
            )}

            <div className="flex items-start gap-2.5 text-sm">
              <GraduationCap className="w-4 h-4 text-[#143D30]" />
              <div>
                <span className="text-xs text-stone-500 block">{t.filter_by_type}</span>
                <span className="font-medium text-stone-900">
                  {univ ? t.type_university : lang === 'ar' ? inst?.type_label_ar : inst?.type_label_fr}
                </span>
              </div>
            </div>
          </div>

          {/* Description or Academic Programs */}
          {univ && (
            <div>
              <h3 className="text-sm font-semibold text-stone-900 mb-2 flex items-center gap-2">
                <BookOpen className="w-4 h-4 text-[#143D30]" />
                <span>نبذة تعريفية ومكونات القطب الجامعي</span>
              </h3>
              <p className="text-sm text-stone-700 leading-relaxed">
                {lang === 'ar' ? univ.description_ar : univ.description_fr}
              </p>
              {univ.institutions_list && univ.institutions_list.length > 0 && (
                <div className="mt-3">
                  <span className="text-xs text-stone-500 block mb-1.5 font-medium">أبرز المؤسسات التابعة:</span>
                  <div className="flex flex-wrap gap-1.5">
                    {univ.institutions_list.map((sub, idx) => (
                      <span key={idx} className="text-xs bg-stone-100 text-stone-700 px-2.5 py-1 rounded-md border border-stone-200">
                        {sub}
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {inst && (
            <div className="space-y-4">
              <div>
                <h3 className="text-sm font-semibold text-stone-900 mb-2 flex items-center gap-2">
                  <BookOpen className="w-4 h-4 text-[#143D30]" />
                  <span>{t.academic_programs}</span>
                </h3>
                <ul className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-xs sm:text-sm text-stone-700">
                  {(lang === 'ar' ? inst.programs_summary_ar : inst.programs_summary_fr).map((prog, idx) => (
                    <li key={idx} className="flex items-center gap-2 bg-stone-50 p-2.5 rounded-lg border border-stone-200/60">
                      <span className="w-1.5 h-1.5 rounded-full bg-[#143D30] shrink-0" />
                      <span>{prog}</span>
                    </li>
                  ))}
                </ul>
              </div>

              <div>
                <h4 className="text-xs font-semibold text-stone-900 mb-1">{t.access_mode}</h4>
                <p className="text-xs text-stone-600 bg-[#FAF8F5] p-3 rounded-lg border border-[#EDE8DE] leading-relaxed">
                  {lang === 'ar' ? inst.access_mode_ar : inst.access_mode_fr}
                </p>
              </div>
            </div>
          )}

          {/* Verified Contacts Section */}
          <div>
            <h3 className="text-xs font-semibold uppercase tracking-wider text-stone-500 mb-2.5">
              بيانات الاتصال المعتمدة (دون افتراضات)
            </h3>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-sm">
              <div className="p-3 bg-stone-50 rounded-lg border border-stone-200">
                <span className="text-xs text-stone-500 flex items-center gap-1.5 mb-1">
                  <Phone className="w-3.5 h-3.5" />
                  <span>{t.contact_phone}</span>
                </span>
                {item.phone ? (
                  <span className="font-mono text-stone-900 font-medium">{item.phone}</span>
                ) : (
                  <span className="text-xs text-stone-400 italic">{t.unavailable}</span>
                )}
              </div>

              <div className="p-3 bg-stone-50 rounded-lg border border-stone-200">
                <span className="text-xs text-stone-500 flex items-center gap-1.5 mb-1">
                  <Mail className="w-3.5 h-3.5" />
                  <span>{t.contact_email}</span>
                </span>
                {item.email ? (
                  <span className="font-mono text-stone-900 font-medium">{item.email}</span>
                ) : (
                  <span className="text-xs text-stone-400 italic">{t.unavailable}</span>
                )}
              </div>
            </div>
          </div>

          {/* Action buttons: Official Website, Compare & Share */}
          <div className="pt-2 flex flex-wrap items-center gap-3">
            <a
              href={item.website_url}
              target="_blank"
              rel="noopener noreferrer"
              className="flex-1 min-w-[140px] flex items-center justify-center gap-2 px-4 py-3 rounded-xl bg-[#143D30] text-white font-medium hover:bg-[#0e2c22] transition-colors shadow-xs focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none text-xs sm:text-sm"
              aria-label={`${t.official_website} ${displayName}`}
              tabIndex={0}
            >
              <span>{t.official_website}</span>
              <ExternalLink className="w-4 h-4" />
            </a>

            {onToggleCompare && (
              <button
                type="button"
                onClick={() => onToggleCompare(item)}
                className={`px-4 py-3 rounded-xl font-semibold text-xs flex items-center gap-1.5 transition-colors border shadow-xs focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none ${
                  isCompared
                    ? 'bg-amber-100 text-amber-900 border-amber-300'
                    : 'bg-stone-100 hover:bg-stone-200 text-stone-800 border-stone-200'
                }`}
                aria-label={isAr ? 'إضافة أو إزالة من المقارنة' : 'Ajouter au comparateur'}
                tabIndex={0}
              >
                <span>{isCompared ? (isAr ? 'مقارَن' : 'Comparé') : (isAr ? '+ مقارنة' : '+ Comparer')}</span>
              </button>
            )}

            <button
              type="button"
              onClick={() => shareItem(item, lang, (msg) => alert(msg))}
              className="px-4 py-3 bg-stone-100 hover:bg-stone-200 text-stone-800 rounded-xl font-medium text-xs flex items-center gap-1.5 transition-colors border border-stone-200 focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none"
              title={lang === 'ar' ? 'مشاركة رابط المؤسسة المباشر' : 'Partager'}
              aria-label={lang === 'ar' ? `مشاركة رابط ${displayName}` : `Partager ${displayName}`}
              tabIndex={0}
            >
              <Share2 className="w-4 h-4 text-[#143D30]" />
              <span>{lang === 'ar' ? 'مشاركة' : 'Partager'}</span>
            </button>
          </div>

          {/* Provenance and Verification Box */}
          <SourceAttribution
            sourceName={item.source_name}
            sourceUrl={item.source_url}
            verifiedAt={item.verified_at}
            dataStatus={item.data_status}
            lang={lang}
          />

          {/* Report Button */}
          <div className="flex items-center justify-between pt-2 border-t border-stone-200">
            <span className="text-xs text-stone-500">
              هل لاحظت خطأ أو رابطًا معطلاً؟
            </span>
            <button
              onClick={() => {
                onClose();
                onReport(item);
              }}
              className="inline-flex items-center gap-1.5 text-xs text-[#B43425] hover:text-[#8b2316] font-medium p-1.5 transition-colors focus-visible:ring-2 focus-visible:ring-[#B43425] focus-visible:outline-none rounded"
              aria-label={t.report_inaccuracy}
              tabIndex={0}
            >
              <AlertCircle className="w-4 h-4" />
              <span>{t.report_inaccuracy}</span>
            </button>
          </div>
        </div>
      </div>
    </ModalBackdrop>
  );
};
