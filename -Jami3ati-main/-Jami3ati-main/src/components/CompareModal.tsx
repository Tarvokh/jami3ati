import React, { useState } from 'react';
import { University, Institution, Language } from '../types';
import { translations } from '../data/translations';
import { ModalBackdrop } from './ModalBackdrop';
import { DataStatusBadge } from './DataStatusBadge';
import { useEscapeKey } from '../hooks/useEscapeKey';
import {
  X,
  Building2,
  GraduationCap,
  MapPin,
  Globe,
  CheckCircle2,
  ExternalLink,
  ShieldCheck,
  Phone,
  Mail,
  Scale,
  Sparkles,
  Trash2,
  Plus,
} from 'lucide-react';

export type ComparableItem = {
  item: University | Institution;
  type: 'university' | 'institution';
};

interface Props {
  items: ComparableItem[];
  lang: Language;
  onClose: () => void;
  onRemoveItem: (id: string) => void;
  onClearAll: () => void;
}

export const CompareModal: React.FC<Props> = ({ items, lang, onClose, onRemoveItem, onClearAll }) => {
  const t = translations[lang];
  const isAr = lang === 'ar';
  const [showDifferencesOnly, setShowDifferencesOnly] = useState(false);

  useEscapeKey(onClose, items.length > 0);

  if (items.length === 0) return null;

  // Row helper: Determine if a row's values are identical across all compared items
  const isIdentical = (rowValues: any[]) => {
    const firstValue = JSON.stringify(rowValues[0]);
    return rowValues.every(val => JSON.stringify(val) === firstValue);
  };

  const getRow = (label: string, renderCell: (item: ComparableItem) => React.ReactNode, rowValues: any[]) => ({
    label,
    render: () => (
      <tr>
        <td className="p-3 bg-stone-50 font-semibold text-stone-700">{label}</td>
        {items.map(({ item }) => (
          <td key={item.id} className="p-3 bg-white">{renderCell({ item, type: items[0].type })}</td>
        ))}
        {items.length < 3 && <td className="bg-stone-50/30" />}
      </tr>
    ),
    isDifferent: !isIdentical(rowValues),
  });

  const rowDefinitions = [
    getRow(isAr ? 'النوع / الجامعة الأم' : 'Type / Université', ({ item }) => (
      'type' in item && (item as Institution).type ? (
        <div className="space-y-0.5">
          <span className="font-bold text-stone-900 block">{(item as Institution).type_label_ar}</span>
          <span className="text-[11px] text-[#143D30] block">تابع لـ: {isAr ? (item as Institution).university_name_ar : (item as Institution).university_name_fr}</span>
        </div>
      ) : (
        <div>
          <span className="font-bold text-stone-900 block">جامعة عمومية مركزية</span>
          <span className="text-[11px] text-stone-500 block">تأسست سنة {(item as University).established_year}</span>
        </div>
      )
    ), items.map(i => i.item.code)),
    getRow(isAr ? 'حالة التوثيق والمصدر' : 'Statut & Traçabilité', ({ item }) => (
      <div className="space-y-1">
        <DataStatusBadge status={item.data_status} lang={lang} size="sm" />
        <span className="text-[11px] text-stone-500 block">{item.source_name}</span>
        <span className="text-[10px] font-mono text-stone-400 block">تاريخ التحقق: {item.verified_at}</span>
      </div>
    ), items.map(i => i.item.data_status)),
    getRow(isAr ? 'الاتصال المعتمد' : 'Contacts vérifiés', ({ item }) => (
      <div className="space-y-1 text-[11px]">
        <div className="flex items-center gap-1 text-stone-700">
          <Phone className="w-3 h-3 text-[#143D30] shrink-0" />
          <span className="font-mono">{item.phone || <span className="text-stone-400 italic">غير متاح</span>}</span>
        </div>
        <div className="flex items-center gap-1 text-stone-700">
          <Mail className="w-3 h-3 text-[#143D30] shrink-0" />
          <span className="font-mono truncate max-w-[150px]">{item.email || <span className="text-stone-400 italic">غير متاح</span>}</span>
        </div>
      </div>
    ), items.map(i => `${i.item.phone}-${i.item.email}`)),
  ];

  return (
    <ModalBackdrop onClose={onClose} ariaLabelledBy="compare-modal-title">
      <div className="relative w-full max-w-5xl max-h-[90vh] my-auto flex flex-col bg-[#FAF8F5] rounded-2xl shadow-2xl border border-stone-200 overflow-hidden select-text animate-in zoom-in-95 duration-150">
        {/* Header */}
        <div className="p-4 sm:p-5 bg-white border-b border-stone-200 flex items-center justify-between shrink-0">
          <div className="flex items-center gap-2.5">
            <div className="w-10 h-10 rounded-xl bg-[#143D30] text-white flex items-center justify-center font-bold">
              <Scale className="w-5 h-5 text-amber-300" />
            </div>
            <div>
              <div className="flex items-center gap-2">
                <h3 id="compare-modal-title" className="text-base sm:text-lg font-bold text-stone-900">
                  {isAr ? 'مقارنة مباشرة بين المؤسسات الجامعية' : 'Comparatif côte à côte'}
                </h3>
                <span className="font-mono text-xs font-bold px-2 py-0.5 rounded bg-emerald-100 text-emerald-800">
                  ({items.length}/3)
                </span>
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => setShowDifferencesOnly(!showDifferencesOnly)}
              className={`px-3 py-1.5 text-xs font-medium flex items-center gap-1.5 rounded-lg transition-colors ${
                showDifferencesOnly ? 'bg-[#143D30] text-white' : 'bg-stone-100 text-stone-700 hover:bg-stone-200'
              }`}
            >
              <CheckCircle2 className="w-3.5 h-3.5" />
              <span>{isAr ? (showDifferencesOnly ? 'عرض الكل' : 'الاختلافات فقط') : (showDifferencesOnly ? 'Tout afficher' : 'Différences seulement')}</span>
            </button>
            <button
              onClick={onClearAll}
              className="px-3 py-1.5 text-xs text-stone-500 hover:text-red-600 font-medium flex items-center gap-1 hover:bg-stone-100 rounded-lg transition-colors"
            >
              <Trash2 className="w-3.5 h-3.5" />
              <span>{isAr ? 'تفريغ' : 'Effacer'}</span>
            </button>
          </div>
        </div>

        {/* Scrollable Content Table */}
        <div className="overflow-y-auto overscroll-contain p-4 sm:p-6 flex-1">
          <div className="overflow-x-auto">
            <table className="w-full text-xs border-collapse min-w-[640px]">
              <thead>
                <tr>
                  <th className="p-3 bg-stone-100/80 border-b border-stone-200 text-start w-44 text-stone-600 font-bold">
                    {isAr ? 'معيار المقارنة' : 'Critère'}
                  </th>
                  {items.map(({ item }) => (
                    <th key={item.id} className="p-4 bg-white border-b-2 border-[#143D30] text-start align-top">
                      <div className="font-bold text-sm text-stone-900 leading-snug truncate max-w-[150px]">
                        {isAr ? item.name_ar : item.name_fr}
                      </div>
                      <span className="font-mono text-xs text-[#143D30] block">{item.code}</span>
                    </th>
                  ))}
                  {items.length < 3 && <th className="p-4 bg-stone-50/50 border-b border-stone-200" />}
                </tr>
              </thead>
              <tbody className="divide-y divide-stone-200/80 text-stone-800">
                {rowDefinitions
                  .filter(r => !showDifferencesOnly || r.isDifferent)
                  .map(r => r.render())}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </ModalBackdrop>
  );
};
