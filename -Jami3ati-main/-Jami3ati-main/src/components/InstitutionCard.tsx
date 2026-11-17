import React from 'react';
import { Institution, Language } from '../types';
import { translations } from '../data/translations';
import { DataStatusBadge } from './DataStatusBadge';
import { MapPin, Globe, ExternalLink, ArrowRight, ArrowLeft, Building2, Scale, Share2 } from 'lucide-react';
import { shareItem } from '../utils/shareUtils';

interface Props {
  institution: Institution;
  lang: Language;
  onSelect: (inst: Institution) => void;
  onReport: (inst: Institution) => void;
  isCompared?: boolean;
  onToggleCompare?: (inst: Institution) => void;
  onShare?: (inst: Institution) => void;
}

export const InstitutionCard: React.FC<Props> = ({
  institution,
  lang,
  onSelect,
  onReport,
  isCompared = false,
  onToggleCompare,
  onShare,
}) => {
  const [imgError, setImgError] = React.useState(false);
  const t = translations[lang];
  const isAr = lang === 'ar';
  const name = isAr ? institution.name_ar : institution.name_fr;
  const parentUniv = isAr ? institution.university_name_ar : institution.university_name_fr;
  const city = isAr ? institution.city_ar : institution.city_fr;
  const campus = isAr ? institution.campus_ar : institution.campus_fr;
  const typeLabel = isAr ? institution.type_label_ar : institution.type_label_fr;

  const disciplineLabels = isAr
    ? institution.disciplines_labels_ar
    : institution.disciplines_labels_fr || institution.disciplines_labels_ar;

  return (
    <div className="group bg-white/90 backdrop-blur-sm rounded-2xl border border-stone-200/60 hover:border-[#143D30]/30 transition-all duration-200 flex flex-col justify-between overflow-hidden shadow-sm hover:shadow-lg hover:-translate-y-1">
      <div>
        {/* Visual Header */}
        <div className="relative h-44 w-full bg-stone-100 overflow-hidden">
          {!imgError ? (
            <img
              src={institution.image_url}
              alt={name}
              onError={() => setImgError(true)}
              className="w-full h-full object-cover group-hover:scale-102 transition-transform duration-300"
              referrerPolicy="no-referrer"
            />
          ) : (
            <div className="w-full h-full bg-gradient-to-br from-[#143D30] to-[#0d2a21] flex flex-col items-center justify-center p-4 text-white">
              <span className="font-mono text-2xl font-extrabold tracking-widest text-emerald-300 opacity-95">
                {institution.code}
              </span>
              <span className="text-xs text-stone-300 font-medium mt-1 truncate max-w-full">
                {institution.city_ar}
              </span>
            </div>
          )}
          <div className="absolute inset-0 bg-gradient-to-t from-black/60 via-black/10 to-transparent" />
          <div className="absolute top-3 right-3 left-3 flex items-center justify-between">
            <span className="font-mono text-xs font-semibold px-2.5 py-1 bg-white/95 text-stone-900 rounded shadow-xs">
              {institution.code}
            </span>
            <DataStatusBadge status={institution.data_status} lang={lang} size="sm" />
          </div>
          <div className="absolute bottom-3 right-3 left-3 text-white">
            <div className="flex items-center gap-1.5 text-xs text-stone-200">
              <MapPin className="w-3.5 h-3.5 text-amber-300 shrink-0" />
              <span>{city}</span>
              <span aria-hidden="true">·</span>
              <span className="truncate max-w-[180px]">{campus}</span>
            </div>
          </div>
        </div>

        {/* Content */}
        <div className="p-5">
          {/* Parent University Banner */}
          <div className="flex items-center gap-1.5 text-xs text-[#143D30] font-medium mb-2 bg-[#FAF7F0] px-2.5 py-1 rounded-md border border-[#E7E2D6] w-fit">
            <Building2 className="w-3.5 h-3.5 shrink-0 text-[#143D30]" />
            <span className="truncate max-w-[240px]">{parentUniv}</span>
          </div>

          {/* Discipline Unboxed Text Metadata */}
          {disciplineLabels && disciplineLabels.length > 0 && (
            <div className="flex flex-wrap items-center gap-1.5 text-[11px] font-semibold text-[#143D30] mb-2">
              {disciplineLabels.map((tag, idx) => (
                <React.Fragment key={idx}>
                  <span>{tag}</span>
                  {idx < disciplineLabels.length - 1 && <span className="text-stone-300 font-normal" aria-hidden="true">·</span>}
                </React.Fragment>
              ))}
            </div>
          )}

          <button
            type="button"
            onClick={() => onSelect(institution)}
            className="w-full text-start group-hover:text-[#143D30] transition-colors focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded"
            tabIndex={0}
            aria-label={`عرض تفاصيل ${name}`}
          >
            <h3 className="text-base font-bold text-stone-900 leading-snug">
              {name}
            </h3>
          </button>
          <p className="text-xs text-stone-500 font-normal mt-0.5 mb-3 line-clamp-1">
            {isAr ? institution.name_fr : institution.name_ar}
          </p>

          {/* Academic Specialties Highlights */}
          <div className="space-y-1 mb-4">
            <span className="text-[11px] font-medium uppercase tracking-wider text-stone-400 block">
              {typeLabel}
            </span>
            <ul className="text-xs text-stone-600 space-y-1">
              {(isAr ? institution.programs_summary_ar : institution.programs_summary_fr).slice(0, 2).map((prog, idx) => (
                <li key={idx} className="truncate flex items-center gap-1.5">
                  <span className="w-1 h-1 rounded-full bg-stone-400 shrink-0" />
                  <span className="truncate">{prog}</span>
                </li>
              ))}
            </ul>
          </div>

          {/* Unboxed Metadata with · separator & Freshness Badge */}
          <div className="flex flex-wrap items-center justify-between gap-1.5 text-xs text-stone-500 border-t border-stone-100 pt-3">
            <span className="truncate max-w-[150px]">{isAr ? institution.access_mode_ar.split('،')[0] : institution.access_mode_fr.split(',')[0]}</span>
            <span className="font-mono text-[10px] text-emerald-800 font-semibold bg-emerald-50 px-2 py-0.5 rounded border border-emerald-200/60 inline-flex items-center gap-1 shrink-0">
              <span className="w-1.5 h-1.5 rounded-full bg-emerald-600 animate-pulse" />
              <span>{isAr ? 'مُراجع منذ 3 أيام' : 'Révisé il y a 3 jours'}</span>
            </span>
          </div>
        </div>
      </div>

      {/* Card Footer Actions */}
      <div className="px-5 pb-5 pt-2 flex items-center justify-between gap-2 border-t border-stone-100 bg-stone-50/50">
        <a
          href={institution.website_url}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center gap-1 text-xs text-stone-600 hover:text-stone-900 font-medium focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-1"
          title={t.official_website}
          aria-label={`${t.official_website} ${name}`}
          tabIndex={0}
        >
          <Globe className="w-3.5 h-3.5 text-[#143D30]" />
          <span className="truncate max-w-[80px]">{institution.code} Portal</span>
          <ExternalLink className="w-3 h-3" />
        </a>

        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onShare ? onShare(institution) : shareItem(institution, lang);
            }}
            className="p-1.5 rounded-lg bg-stone-100 hover:bg-stone-200 text-stone-700 transition-colors focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none"
            title={isAr ? 'مشاركة رابط المؤسسة' : 'Partager'}
            aria-label={isAr ? `مشاركة ${name}` : `Partager ${name}`}
            tabIndex={0}
          >
            <Share2 className="w-3.5 h-3.5 text-[#143D30]" />
          </button>

          {onToggleCompare && (
            <button
              type="button"
              onClick={() => onToggleCompare(institution)}
              className={`inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-xs font-semibold transition-colors focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none ${
                isCompared
                  ? 'bg-amber-100 text-amber-900 border border-amber-300'
                  : 'bg-stone-100 hover:bg-stone-200 text-stone-700'
              }`}
              title="إضافة أو حذف من قائمة المقارنة"
              aria-label={isAr ? `مقارنة ${name}` : `Comparer ${name}`}
              tabIndex={0}
            >
              <Scale className="w-3.5 h-3.5 text-[#143D30]" />
              <span>{isCompared ? (isAr ? 'مقارَن' : 'Comparé') : (isAr ? '+ مقارنة' : '+ Comparer')}</span>
            </button>
          )}

          <button
            type="button"
            onClick={() => onSelect(institution)}
            className="inline-flex items-center gap-1 px-3 py-1.5 rounded-lg bg-[#143D30] text-white text-xs font-medium hover:bg-[#0f2e24] transition-colors focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none"
            aria-label={isAr ? `التفاصيل الكاملة لـ ${name}` : `Détails de ${name}`}
            tabIndex={0}
          >
            <span>{t.view_details}</span>
            {isAr ? <ArrowLeft className="w-3.5 h-3.5" /> : <ArrowRight className="w-3.5 h-3.5" />}
          </button>
        </div>
      </div>
    </div>
  );
};
