import React, { useCallback, useEffect, useState } from 'react';
import { Language } from '../types';
import { translations } from '../data/translations';
import {
  ApiError,
  listNotifications,
  type NotificationRecord,
} from '../lib/api';
import { AlertTriangle, Calendar, ExternalLink, RefreshCw, ShieldCheck } from 'lucide-react';

interface Props {
  lang: Language;
}

type Phase = 'loading' | 'ready' | 'error';

function formatDate(value: string | null, lang: Language): string {
  if (!value) return '—';
  // The API stores naive UTC; without the suffix a browser reads it as local
  // time and the date can slide by one day.
  const normalised = /[zZ]|[+-]\d{2}:\d{2}$/.test(value) ? value : `${value}Z`;
  const date = new Date(normalised);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString(lang === 'ar' ? 'ar-MA' : 'fr-MA', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  });
}

function sourceHost(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return url;
  }
}

function kindLabel(kind: string, isAr: boolean): string {
  if (kind === 'official_change') return isAr ? 'تغيير رسمي' : 'Changement officiel';
  return kind;
}

export const AnnouncementsSection: React.FC<Props> = ({ lang }) => {
  const t = translations[lang];
  const isAr = lang === 'ar';

  const [phase, setPhase] = useState<Phase>('loading');
  const [items, setItems] = useState<NotificationRecord[]>([]);
  const [errorMessage, setErrorMessage] = useState('');

  const load = useCallback(() => {
    setPhase('loading');
    setErrorMessage('');
    listNotifications()
      .then((rows) => {
        setItems(rows);
        setPhase('ready');
      })
      .catch((error: unknown) => {
        setErrorMessage(
          error instanceof ApiError && error.isNetworkError
            ? isAr
              ? 'تعذّر الاتصال بالخادم. تأكد من أن الـAPI يعمل على المنفذ 8000.'
              : 'Serveur injoignable. Vérifiez que l’API écoute sur le port 8000.'
            : error instanceof Error
              ? error.message
              : String(error),
        );
        setPhase('error');
      });
  }, [isAr]);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <section id="announcements" className="py-12 border-t border-stone-200 w-full max-w-full overflow-x-hidden">
      <div className="max-w-6xl mx-auto px-4 sm:px-6 w-full">
        <div className="flex flex-col md:flex-row md:items-end justify-between gap-4 mb-8 w-full">
          <div>
            <div className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-[#143D30] mb-1">
              <ShieldCheck className="w-4 h-4 text-emerald-700" />
              <span>{isAr ? 'أرشيف عام من الـAPI — بلا حساب' : 'Archive publique via API — sans compte'}</span>
            </div>
            <h2 className="text-2xl sm:text-3xl font-bold text-stone-900 tracking-tight">
              {t.announcements_title}
            </h2>
            <p className="text-sm text-stone-600 mt-1 max-w-2xl">
              {t.announcements_desc}
            </p>
          </div>

          <div className="text-xs text-stone-500 bg-stone-100 px-3 py-1.5 rounded-lg border border-stone-200 shrink-0 font-mono">
            {phase === 'ready'
              ? isAr
                ? `${items.length} إشعارًا منشورًا`
                : `${items.length} publication(s)`
              : phase === 'loading'
                ? isAr
                  ? 'جارٍ التحميل…'
                  : 'Chargement…'
                : isAr
                  ? 'تعذّر التحميل'
                  : 'Échec du chargement'}
          </div>
        </div>

        {phase === 'loading' && (
          <div className="grid grid-cols-1 md:grid-cols-3 gap-5 w-full" aria-busy="true">
            {[0, 1, 2].map((key) => (
              <div key={key} className="bg-white rounded-xl border border-stone-200 p-5 h-48 animate-pulse">
                <div className="h-3 w-24 bg-stone-200 rounded mb-4" />
                <div className="h-3 w-full bg-stone-100 rounded mb-2" />
                <div className="h-3 w-5/6 bg-stone-100 rounded" />
              </div>
            ))}
          </div>
        )}

        {phase === 'error' && (
          <div className="bg-white rounded-xl border border-red-200 p-8 text-center space-y-3 w-full">
            <div className="w-12 h-12 bg-red-50 text-red-600 rounded-full flex items-center justify-center mx-auto">
              <AlertTriangle className="w-6 h-6" />
            </div>
            <h3 className="text-base font-bold text-stone-900">
              {isAr ? 'تعذّر تحميل الأرشيف' : 'Archive indisponible'}
            </h3>
            <p className="text-xs sm:text-sm text-stone-600 max-w-md mx-auto">{errorMessage}</p>
            <p className="text-[11px] text-stone-500 max-w-md mx-auto">
              {isAr
                ? 'لا نعرض بيانات تجريبية بدلًا من الأرشيف: ما لا يأتي من الخادم لا يُعرض كإشعار.'
                : 'Aucune donnée de démonstration n’est substituée : ce qui ne vient pas du serveur n’est pas affiché comme publication.'}
            </p>
            <button
              onClick={load}
              className="inline-flex items-center gap-1.5 px-4 py-2 bg-[#143D30] text-white text-xs font-medium rounded-lg hover:bg-[#0f2e24]"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              <span>{isAr ? 'إعادة المحاولة' : 'Réessayer'}</span>
            </button>
          </div>
        )}

        {phase === 'ready' && items.length === 0 && (
          <div className="bg-white rounded-xl border border-stone-200 p-12 text-center space-y-3 w-full">
            <div className="w-12 h-12 bg-stone-100 text-stone-400 rounded-full flex items-center justify-center mx-auto">
              <Calendar className="w-6 h-6" />
            </div>
            <h3 className="text-base font-bold text-stone-900">
              {isAr ? 'لا توجد إشعارات منشورة بعد' : 'Aucune publication pour le moment'}
            </h3>
            <p className="text-xs sm:text-sm text-stone-500 max-w-md mx-auto">
              {isAr
                ? 'الأرشيف يبدأ فارغًا عن قصد. يُنشر الإشعار فقط بعد التحقق من التغيير في الصفحة الرسمية، ويحمل حينها رابط المصدر وتاريخ التحقق.'
                : 'L’archive démarre vide volontairement. Une publication n’apparaît qu’après vérification du changement sur la page officielle, avec lien source et date de vérification.'}
            </p>
          </div>
        )}

        {phase === 'ready' && items.length > 0 && (
          <div className="grid grid-cols-1 md:grid-cols-3 gap-5 w-full">
            {items.map((item) => (
              <div
                key={item.id}
                className="bg-white rounded-xl border border-stone-200 p-5 flex flex-col justify-between shadow-xs hover:border-stone-300 transition-colors"
              >
                <div>
                  <div className="flex items-center justify-between text-xs text-stone-500 mb-3 pb-2 border-b border-stone-100 gap-2">
                    <span className="font-mono font-medium text-[#143D30] bg-[#FAF7F0] px-2 py-0.5 rounded border border-[#E7E2D6] truncate">
                      {item.id}
                    </span>
                    <span className="shrink-0 text-[10px] uppercase tracking-wider text-stone-500">
                      {kindLabel(item.kind, isAr)}
                    </span>
                  </div>

                  <h3 className="text-sm font-bold text-stone-900 leading-snug mb-2" dir="auto">
                    {item.title_ar}
                  </h3>

                  <p className="text-xs text-stone-600 leading-relaxed line-clamp-4" dir="auto">
                    {item.body_ar}
                  </p>

                  {!isAr && (
                    <p className="text-[10px] text-stone-400 mt-2">
                      Texte publié en arabe (langue source de l’avis).
                    </p>
                  )}
                </div>

                <div className="pt-3 mt-4 border-t border-stone-100 flex flex-col gap-2">
                  <div className="text-[11px] text-stone-500 flex items-center justify-between gap-2">
                    <span className="inline-flex items-center gap-1">
                      <Calendar className="w-3 h-3 text-stone-400" />
                      {isAr ? 'تحقّق' : 'Vérifié'} {formatDate(item.verified_at, lang)}
                    </span>
                    {item.institution_id && (
                      <span className="font-mono text-[10px] text-stone-400 truncate">
                        {item.institution_id}
                      </span>
                    )}
                  </div>

                  <a
                    href={item.source_url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center justify-between text-xs text-[#143D30] hover:text-[#0b211a] font-medium pt-1 border-t border-dashed border-stone-200 gap-2"
                  >
                    <span className="truncate max-w-[200px]">{sourceHost(item.source_url)}</span>
                    <ExternalLink className="w-3.5 h-3.5 shrink-0" />
                  </a>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
};
