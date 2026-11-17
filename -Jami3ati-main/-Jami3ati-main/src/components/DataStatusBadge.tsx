import React from 'react';
import { DataStatus, Language } from '../types';
import { translations } from '../data/translations';
import { CheckCircle2, AlertTriangle, Sparkles } from 'lucide-react';

interface Props {
  status: DataStatus;
  lang: Language;
  size?: 'sm' | 'md';
}

export const DataStatusBadge: React.FC<Props> = ({ status, lang, size = 'sm' }) => {
  const t = translations[lang];

  if (status === 'verified') {
    return (
      <span
        className={`inline-flex items-center gap-1.5 font-medium text-emerald-800 bg-emerald-50/90 border border-emerald-200/80 rounded-md ${
          size === 'sm' ? 'px-2 py-0.5 text-xs' : 'px-3 py-1 text-sm'
        }`}
      >
        <CheckCircle2 className={size === 'sm' ? 'w-3.5 h-3.5 text-emerald-600' : 'w-4 h-4 text-emerald-600'} />
        <span>{t.verified_badge}</span>
      </span>
    );
  }

  if (status === 'needs_review') {
    return (
      <span
        className={`inline-flex items-center gap-1.5 font-medium text-amber-800 bg-amber-50/90 border border-amber-200/80 rounded-md ${
          size === 'sm' ? 'px-2 py-0.5 text-xs' : 'px-3 py-1 text-sm'
        }`}
      >
        <AlertTriangle className={size === 'sm' ? 'w-3.5 h-3.5 text-amber-600' : 'w-4 h-4 text-amber-600'} />
        <span>{t.needs_review_badge}</span>
      </span>
    );
  }

  return (
    <span
      className={`inline-flex items-center gap-1.5 font-medium text-stone-700 bg-stone-100 border border-stone-200 rounded-md ${
        size === 'sm' ? 'px-2 py-0.5 text-xs' : 'px-3 py-1 text-sm'
      }`}
    >
      <Sparkles className={size === 'sm' ? 'w-3.5 h-3.5 text-stone-500' : 'w-4 h-4 text-stone-500'} />
      <span>{t.demo_badge}</span>
    </span>
  );
};
