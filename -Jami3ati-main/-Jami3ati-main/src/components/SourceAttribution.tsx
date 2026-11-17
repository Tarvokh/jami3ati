import React from 'react';
import { Language, DataStatus } from '../types';
import { translations } from '../data/translations';
import { ExternalLink, ShieldCheck, Calendar, Globe } from 'lucide-react';
import { DataStatusBadge } from './DataStatusBadge';

interface Props {
  sourceName: string;
  sourceUrl: string;
  verifiedAt: string;
  dataStatus: DataStatus;
  lang: Language;
}

export const SourceAttribution: React.FC<Props> = ({
  sourceName,
  sourceUrl,
  verifiedAt,
  dataStatus,
  lang,
}) => {
  const t = translations[lang];

  return (
    <div className="bg-[#FAF7F0] border border-[#E7E2D6] rounded-xl p-4 text-stone-800 text-sm">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-[#E7E2D6] pb-2.5 mb-3">
        <div className="flex items-center gap-2 font-semibold text-stone-900">
          <ShieldCheck className="w-4 h-4 text-[#143D30]" />
          <span>{t.source_attribution}</span>
        </div>
        <DataStatusBadge status={dataStatus} lang={lang} size="sm" />
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-3 text-xs md:text-sm">
        <div>
          <span className="text-stone-500 block mb-0.5">{t.source_name_label}</span>
          <span className="font-medium text-stone-900">{sourceName}</span>
        </div>

        <div>
          <span className="text-stone-500 block mb-0.5">{t.verified_at_label}</span>
          <div className="flex items-center gap-1.5 font-medium text-stone-900 font-mono">
            <Calendar className="w-3.5 h-3.5 text-stone-500" />
            <span>{verifiedAt}</span>
          </div>
        </div>

        <div className="md:col-span-2">
          <span className="text-stone-500 block mb-0.5">{t.source_link_label}</span>
          {sourceUrl ? (
            <a
              href={sourceUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 text-[#143D30] hover:text-[#0c261e] underline underline-offset-2 break-all font-mono text-xs"
            >
              <Globe className="w-3.5 h-3.5 shrink-0" />
              <span className="truncate">{sourceUrl}</span>
              <ExternalLink className="w-3 h-3 shrink-0" />
            </a>
          ) : (
            <span className="text-stone-400 italic">{t.unavailable}</span>
          )}
        </div>
      </div>
    </div>
  );
};
