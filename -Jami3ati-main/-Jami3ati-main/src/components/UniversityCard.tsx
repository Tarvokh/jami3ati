import React from 'react';
import { University, Language } from '../types';
import { translations } from '../data/translations';
import { DataStatusBadge } from './DataStatusBadge';
import { MapPin, Globe, ExternalLink, ArrowRight, ArrowLeft, Scale, Share2 } from 'lucide-react';
import { shareItem } from '../utils/shareUtils';

interface Props {
  university: University;
  lang: Language;
  onSelect: (univ: University) => void;
  onReport: (univ: University) => void;
  isCompared?: boolean;
  onToggleCompare?: (univ: University) => void;
  onShare?: (univ: University) => void;
}

export const UniversityCard: React.FC<Props> = ({
  university,
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
  const name = isAr ? university.name_ar : university.name_fr;
  const city = isAr ? university.city_ar : university.city_fr;
  const desc = isAr ? university.description_ar : university.description_fr;

  return (
    <div className="group bg-white/90 backdrop-blur-sm rounded-2xl border border-stone-200/60 hover:border-[#143D30]/30 transition-all duration-200 flex flex-col justify-between overflow-hidden shadow-sm hover:shadow-lg hover:-translate-y-1">
      <div>
        {/* Visual Header */}
        <div className="relative h-44 w-full bg-stone-100 overflow-hidden">
          {!imgError ? (
            <img
              src={university.image_url}
              alt={name}
              onError={() => setImgError(true)}
              className="w-full h-full object-cover group-hover:scale-102 transition-transform duration-300"
              referrerPolicy="no-referrer"
            />
          ) : (
            <div className="w-full h-full bg-gradient-to-br from-[#143D30] to-[#0a1f18] flex flex-col items-center justify-center p-4 text-white">
              <span className="font-mono text-2xl font-extrabold tracking-widest text-amber-300 opacity-90">
                {university.code}
              </span>
              <span className="text-xs text-stone-300 font-medium mt-1 truncate max-w-full">
                {university.city_ar}
              </span>
            </div>
          )}
          <div className="absolute inset-0 bg-gradient-to-t from-black/60 via-black/10 to-transparent" />
          <div className="absolute top-3 right-3 left-3 flex items-center justify-between">
            <span className="font-mono text-xs font-semibold px-2.5 py-1 bg-white/95 text-stone-900 rounded shadow-xs">
              {university.code}
            </span>
            <DataStatusBadge status={university.data_status} lang={lang} size="sm" />
          </div>
          <div className="absolute bottom-3 right-3 left-3 text-white">
            <div className="flex items-center gap-1.5 text-xs text-stone-200">
              <MapPin className="w-3.5 h-3.5 text-amber-300 shrink-0" />
              <span>{city}</span>
              <span aria-hidden="true">·</span>
              <span>تأسست {university.established_year}</span>
            </div>
          </div>
        </div>

        {/* Content */}
        <div className="p-5">
          <button
            type="button"
            onClick={() => onSelect(university)}
            className="w-full text-start group-hover:text-[#143D30] transition-colors focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded"
            tabIndex={0}
            aria-label={`عرض تفاصيل ${name}`}
          >
            <h3 className="text-base font-bold text-stone-900 leading-snug">
              {name}
            </h3>
          </button>
          <p className="text-xs text-stone-500 font-normal mt-0.5 mb-3 line-clamp-1">
            {isAr ? university.name_fr : university.name_ar}
          </p>

          <p className="text-xs text-stone-600 line-clamp-2 leading-relaxed mb-4">
            {desc}
          </p>

          {/* Unboxed Metadata with · separator */}
          <div className="flex items-center gap-2 text-xs text-stone-500 border-t border-stone-100 pt-3 mb-1">
            <span>{university.institution_count} مؤسسة وكلية</span>
            <span aria-hidden="true">·</span>
            <span className="truncate max-w-[140px]">{university.source_name}</span>
          </div>
        </div>
      </div>

      {/* Card Footer Actions */}
      <div className="px-5 pb-5 pt-2 flex items-center justify-between gap-2 border-t border-stone-100 bg-stone-50/50">
        <a
          href={university.website_url}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center gap-1 text-xs text-stone-600 hover:text-stone-900 font-medium focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none rounded px-1"
          title={t.official_website}
          aria-label={`${t.official_website} ${name}`}
          tabIndex={0}
        >
          <Globe className="w-3.5 h-3.5 text-[#143D30]" />
          <span className="truncate max-w-[80px]">{university.website_url.replace('https://', '')}</span>
          <ExternalLink className="w-3 h-3" />
        </a>

        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onShare ? onShare(university) : shareItem(university, lang);
            }}
            className="p-1.5 rounded-lg bg-stone-100 hover:bg-stone-200 text-stone-700 transition-colors focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none"
            title={isAr ? 'مشاركة رابط الجامعة' : 'Partager'}
            aria-label={isAr ? `مشاركة ${name}` : `Partager ${name}`}
            tabIndex={0}
          >
            <Share2 className="w-3.5 h-3.5 text-[#143D30]" />
          </button>

          {onToggleCompare && (
            <button
              type="button"
              onClick={() => onToggleCompare(university)}
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
            onClick={() => onSelect(university)}
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
