import React, { useState } from 'react';
import { Language } from '../types';
import { translations } from '../data/translations';
import { ModalBackdrop } from './ModalBackdrop';
import { useEscapeKey } from '../hooks/useEscapeKey';
import {
  HelpCircle,
  X,
  Search,
  CheckCircle2,
  AlertCircle,
  HelpCircle as QuestionIcon,
  BookOpen,
  ArrowLeft,
  ExternalLink,
  ShieldCheck,
  Scale,
  Sparkles,
  Keyboard,
} from 'lucide-react';

interface Props {
  isOpen: boolean;
  onClose: () => void;
  lang: Language;
}

export const HelpCenterModal: React.FC<Props> = ({ isOpen, onClose, lang }) => {
  useEscapeKey(onClose, isOpen);

  const t = translations[lang];
  const isAr = lang === 'ar';

  const [activeFaq, setActiveFaq] = useState<number | null>(0);

  if (!isOpen) return null;

  const faqs = [
    {
      id: 0,
      question_ar: 'كيف أستفيد من محرك البحث والتصفيات المتقدمة؟',
      question_fr: 'Comment utiliser la recherche et les filtres avancés ?',
      answer_ar:
        'يمكنك استخدام محرك البحث الذكي في الأعلى بإدخال اسم الجامعة أو المدينة (الرباط، مراكش...) أو رمز المؤسسة (ENSIAS، EST...). كما يمكنك الضغط على المفتاح "/" في أي وقت للتركيز المباشر على حقل البحث. تتيح لك شريحة التخصصات تصفية المؤسسات حسب الشعبة الدراسية (هندسة، طب، اقتصاد، علوم...).',
      answer_fr:
        'Utilisez la barre de recherche intelligente en saisissant le nom, la ville ou l’acronyme (ENSIAS, EST...). Appuyez sur la touche "/" pour cibler le champ de recherche instantanément. Filtrez ensuite par filière (ingénierie, médecine, économie...).',
    },
    {
      id: 1,
      question_ar: 'ماذا تعني بطاقات التوثيق المختلفة (Verified / Needs Review)؟',
      question_fr: 'Que signifient les différents statuts de traçabilité ?',
      answer_ar:
        '• "موثق بمصدر رسمي" (Verified): البيانات مُراجعة ومربوطة برابط منشور على data.gov.ma أو البوابة الرسمية للمؤسسة.\n• "قيد التدقيق" (Needs Review): المعلومات قيد التطابق وتحديث التاريخ من المصادر الرسمية.\n• "نموذج تجريبي" (Demo Seed): بيانات محاكاة تجريبية لأهداف العرض الهاكاثوني عند عدم توفر بيانات مفتوحة.',
      answer_fr:
        '• Vérifié (Verified) : Données auditées et liées à une source officielle (.ac.ma / data.gov.ma).\n• À réviser (Needs Review) : Données en cours de vérification.\n• Démo (Demo Seed) : Données de démonstration pour le prototype.',
    },
    {
      id: 2,
      question_ar: 'كيف أستعمل خاصية مقارنة المؤسسات والمشاركة؟',
      question_fr: 'Comment comparer les établissements et partager une fiche ?',
      answer_ar:
        'يمكنك اختيار حتى 3 مؤسسات بالضغط على زر "+ مقارنة" في أي بطاقة. يظهر شريط مقارنة عائم في أسفل الشاشة يعرض المقارنة الجانبية. كما يمكنك الضغط على زر "مشاركة" لنسخ رابط مباشر فريد للمؤسسة أو مشاركته على التواصل الاجتماعي.',
      answer_fr:
        'Sélectionnez jusqu’à 3 établissements en cliquant sur "+ Comparer". Un bandeau flottant vous permet de lancer la comparaison côte à côte. Cliquez sur "Partager" pour copier le lien direct.',
    },
    {
      id: 3,
      question_ar: 'ما مدى رسمية وحداثة المعلومات المنشورة بالمنصة؟',
      question_fr: 'Quelle est la gouvernance et la fraîcheur des données ?',
      answer_ar:
        'جامعتي هي منصة تجريبية مستقلة (Hackathon MVP) تعتمد حصرًا على البيانات المفتوحة والمصادر الرسمية المنشورة دون أي تخمين أو الادعاء بتمثيل رسمي. لكل بطاقة تاريخ مراجعة ورابط تحقق صريح. لمزيد من التفاصيل يرجى الاطلاع على صفحة "حول البيانات ومصادرها".',
      answer_fr:
        'Jami3ati est un prototype indépendant basé sur les données ouvertes. Chaque fiche comporte la date de révision et la source officielle.',
    },
  ];

  return (
    <ModalBackdrop onClose={onClose} ariaLabelledBy="help-center-title">
      <div className="relative w-full max-w-3xl max-h-[90vh] my-auto flex flex-col bg-white rounded-2xl shadow-2xl border border-stone-200 overflow-hidden select-text animate-in zoom-in-95 duration-150">
        {/* Modal Header */}
        <div className="px-6 py-4 bg-[#FAF7F0] border-b border-stone-200 flex items-center justify-between shrink-0">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-xl bg-[#143D30] text-white flex items-center justify-center font-bold shrink-0">
              <HelpCircle className="w-5 h-5 text-amber-300" />
            </div>
            <div>
              <h2 id="help-center-title" className="text-base sm:text-lg font-bold text-stone-900 leading-tight">
                {isAr ? 'مركز المساعدة ودليل الاستخدام' : 'Centre d’aide & Guide d’utilisation'}
              </h2>
              <p className="text-xs text-stone-500">
                {isAr
                  ? 'إجابات عن الأسئلة الشائعة وشرح حوكمة البيانات وميزات المنصة'
                  : 'Foire aux questions, explication des statuts et fonctionnalités'}
              </p>
            </div>
          </div>

          <button
            onClick={onClose}
            className="p-1.5 text-stone-500 hover:text-stone-900 hover:bg-stone-200 rounded-lg transition-colors focus-visible:ring-2 focus-visible:ring-[#143D30]"
            aria-label={t.close}
            tabIndex={0}
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Scrollable Content Container */}
        <div className="p-6 overflow-y-auto space-y-6 flex-1 overscroll-contain text-xs sm:text-sm">
          {/* Quick Concept Badges Overview */}
          <div className="p-4 bg-stone-50 rounded-xl border border-stone-200 space-y-3">
            <span className="text-xs font-bold text-stone-900 block flex items-center gap-1.5">
              <ShieldCheck className="w-4 h-4 text-[#143D30]" />
              <span>{isAr ? 'دليل رموز التوثيق والشفافية:' : 'Guide des badges de traçabilité :'}</span>
            </span>

            <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 text-xs">
              <div className="p-3 bg-white rounded-lg border border-emerald-200 space-y-1">
                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-bold bg-emerald-100 text-emerald-800">
                  <CheckCircle2 className="w-3 h-3" />
                  <span>Verified</span>
                </span>
                <p className="text-stone-600 leading-relaxed text-[11px]">
                  {isAr
                    ? 'معلومة مُدققة ورابطها منشور وموثق من data.gov.ma أو البوابة الرسمية.'
                    : 'Donnée auditée et liée à un arrêté ou portail officiel.'}
                </p>
              </div>

              <div className="p-3 bg-white rounded-lg border border-amber-200 space-y-1">
                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-bold bg-amber-100 text-amber-800">
                  <AlertCircle className="w-3 h-3" />
                  <span>Needs Review</span>
                </span>
                <p className="text-stone-600 leading-relaxed text-[11px]">
                  {isAr
                    ? 'المعلومة جاري تحديث تاريخ مراجعتها أو التأكد من استمرارية الرابط.'
                    : 'Information en cours d’actualisation.'}
                </p>
              </div>

              <div className="p-3 bg-white rounded-lg border border-stone-200 space-y-1">
                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-bold bg-stone-200 text-stone-800">
                  <Sparkles className="w-3 h-3" />
                  <span>Demo Seed</span>
                </span>
                <p className="text-stone-600 leading-relaxed text-[11px]">
                  {isAr
                    ? 'بيانات تجريبية محاكاة لعرض النموذج عند عدم إتاحة البيانات المفتوحة.'
                    : 'Données de démonstration du prototype.'}
                </p>
              </div>
            </div>
          </div>

          {/* FAQ Accordion List */}
          <div className="space-y-3">
            <h3 className="text-sm font-bold text-stone-900 flex items-center gap-2">
              <BookOpen className="w-4 h-4 text-[#143D30]" />
              <span>{isAr ? 'الأسئلة الأكثر تداولاً:' : 'Foire aux questions :'}</span>
            </h3>

            <div className="space-y-2">
              {faqs.map((faq) => {
                const isExpanded = activeFaq === faq.id;
                const question = isAr ? faq.question_ar : faq.question_fr;
                const answer = isAr ? faq.answer_ar : faq.answer_fr;

                return (
                  <div
                    key={faq.id}
                    className="border border-stone-200 rounded-xl overflow-hidden bg-white shadow-2xs"
                  >
                    <button
                      type="button"
                      onClick={() => setActiveFaq(isExpanded ? null : faq.id)}
                      className="w-full text-start px-4 py-3 font-bold text-stone-900 hover:bg-[#FAF7F0] transition-colors flex items-center justify-between gap-3 text-xs sm:text-sm focus-visible:ring-2 focus-visible:ring-[#143D30] focus-visible:outline-none"
                      aria-expanded={isExpanded}
                      tabIndex={0}
                    >
                      <span className="flex items-center gap-2">
                        <QuestionIcon className="w-4 h-4 text-[#143D30] shrink-0" />
                        <span>{question}</span>
                      </span>
                      <span className="text-stone-400 font-bold text-base">
                        {isExpanded ? '−' : '+'}
                      </span>
                    </button>

                    {isExpanded && (
                      <div className="px-4 pb-4 pt-1 text-xs sm:text-sm text-stone-700 leading-relaxed border-t border-stone-100 bg-[#FAF8F5]/50 whitespace-pre-line animate-in fade-in duration-100">
                        {answer}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>

          {/* Quick Links & Shortcuts section */}
          <div className="pt-2 border-t border-stone-200 flex flex-col sm:flex-row items-center justify-between gap-4">
            <div className="flex items-center gap-2 text-xs text-stone-600">
              <Keyboard className="w-4 h-4 text-[#143D30]" />
              <span>
                اختصار البحث السريع: اضغط على <kbd className="px-1.5 py-0.5 bg-stone-200 rounded font-mono font-bold text-[10px]">/</kbd> للتركيز على محرك البحث.
              </span>
            </div>

            <a
              href="#about-data"
              onClick={() => {
                onClose();
                setTimeout(() => {
                  const el = document.getElementById('about-data');
                  el?.scrollIntoView({ behavior: 'smooth' });
                }, 100);
              }}
              className="inline-flex items-center gap-1.5 px-4 py-2 bg-[#143D30] hover:bg-[#0f2e24] text-white text-xs font-semibold rounded-lg transition-colors shrink-0 focus-visible:ring-2 focus-visible:ring-[#143D30]"
              tabIndex={0}
            >
              <span>الانتقال لصفحة «حول البيانات»</span>
              <ArrowLeft className="w-3.5 h-3.5" />
            </a>
          </div>
        </div>
      </div>
    </ModalBackdrop>
  );
};
