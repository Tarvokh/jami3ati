import React from 'react';
import { ComparableItem } from './CompareModal';
import { Language } from '../types';
import { Scale, X, ArrowLeft, ArrowRight, Trash2 } from 'lucide-react';

interface Props {
  items: ComparableItem[];
  lang: Language;
  onOpenCompare: () => void;
  onRemoveItem: (id: string) => void;
  onClearAll: () => void;
}

export const CompareTray: React.FC<Props> = ({
  items,
  lang,
  onOpenCompare,
  onRemoveItem,
  onClearAll,
}) => {
  const isAr = lang === 'ar';

  if (items.length === 0) return null;

  return (
    <div className="fixed bottom-16 md:bottom-6 inset-x-4 max-w-xl mx-auto z-40 animate-in slide-in-from-bottom-3 duration-200">
      <div className="bg-stone-900/95 text-white backdrop-blur-md p-3.5 rounded-2xl shadow-2xl border border-stone-700 flex items-center justify-between gap-3">
        {/* Chips list */}
        <div className="flex items-center gap-2 overflow-x-auto min-w-0 py-0.5">
          <div className="flex items-center gap-1.5 font-bold text-xs shrink-0 text-amber-300">
            <Scale className="w-4 h-4" />
            <span>({items.length}/3)</span>
          </div>

          <div className="flex items-center gap-1.5 overflow-x-auto no-scrollbar">
            {items.map(({ item }) => (
              <span
                key={item.id}
                className="inline-flex items-center gap-1 px-2.5 py-1 rounded-lg bg-stone-800 text-stone-200 text-xs font-mono border border-stone-700 shrink-0"
              >
                <span>{item.code}</span>
                <button
                  onClick={() => onRemoveItem(item.id)}
                  className="p-0.5 hover:text-red-400 rounded transition-colors"
                  title="حذف"
                >
                  <X className="w-3 h-3" />
                </button>
              </span>
            ))}
          </div>
        </div>

        {/* Action Button */}
        <div className="flex items-center gap-2 shrink-0">
          <button
            onClick={onClearAll}
            className="p-2 text-stone-400 hover:text-stone-200 transition-colors"
            title="تفريغ القائمة"
          >
            <Trash2 className="w-4 h-4" />
          </button>
          <button
            onClick={onOpenCompare}
            className="inline-flex items-center gap-1.5 px-4 py-2 bg-[#143D30] hover:bg-emerald-700 text-white rounded-xl text-xs font-bold transition-all shadow-md active:scale-95"
          >
            <span>{isAr ? 'مقارنة الآن' : 'Comparer'}</span>
            {isAr ? <ArrowLeft className="w-3.5 h-3.5" /> : <ArrowRight className="w-3.5 h-3.5" />}
          </button>
        </div>
      </div>
    </div>
  );
};
