import React from 'react';
import { Language } from '../types';
import { translations } from '../data/translations';
import { ShieldCheck, Database, Lock, EyeOff, AlertTriangle, FileText, CheckCircle } from 'lucide-react';

interface Props {
  lang: Language;
}

export const AboutDataSection: React.FC<Props> = ({ lang }) => {
  const t = translations[lang];
  const isAr = lang === 'ar';

  return (
    <section id="about-data" className="py-16 bg-[#FAF8F5] border-t border-stone-200 w-full max-w-full overflow-x-hidden">
      <div className="max-w-5xl mx-auto px-4 sm:px-6 space-y-10 w-full">
        <div className="text-center max-w-2xl mx-auto space-y-3 w-full">
          <div className="inline-flex items-center gap-2 px-3 py-1 rounded-md bg-[#143D30]/10 text-[#143D30] text-xs font-semibold">
            <ShieldCheck className="w-4 h-4" />
            <span>ميثاق الشفافية والبيانات المفتوحة</span>
          </div>
          <h2 className="text-2xl sm:text-3xl font-bold text-stone-900 tracking-tight">
            {t.data_policy_title}
          </h2>
          <p className="text-sm text-stone-600 leading-relaxed">
            {isAr
              ? 'نهدف إلى إنهاء تشتت المعلومة الجامعية دون المساس بسيادة المؤسسات أو اختلاق أي معطيات غير موثقة.'
              : 'Notre mission est de centraliser l’accès à l’information universitaire en s’appuyant exclusivement sur les données publiques ouvertes et vérifiées.'}
          </p>
        </div>

        {/* 4 Pillars of Data Governance */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6 w-full">
          <div className="bg-white p-6 rounded-xl border border-stone-200 space-y-2.5 shadow-xs">
            <div className="w-9 h-9 rounded-lg bg-emerald-50 text-emerald-700 flex items-center justify-center">
              <Database className="w-5 h-5" />
            </div>
            <h3 className="text-base font-bold text-stone-900">
              {isAr ? '1. الاعتماد الحصري على البيانات المفتوحة' : '1. Données ouvertes & Portails officiels'}
            </h3>
            <p className="text-xs sm:text-sm text-stone-600 leading-relaxed">
              {isAr
                ? 'نعتمد بالدرجة الأولى على بوابات البيانات المفتوحة الرسمية للمملكة (data.gov.ma)، والمنشورات الرسمية لقطاع التعليم العالي، ومواقع الجامعات المعتمدة قانونياً.'
                : 'Priorité absolue accordée aux jeux de données de data.gov.ma, aux arrêtés d’accréditation et aux domaines web institutionnels (.ac.ma / .gov.ma / .ma).'}
            </p>
          </div>

          <div className="bg-white p-6 rounded-xl border border-stone-200 space-y-2.5 shadow-xs">
            <div className="w-9 h-9 rounded-lg bg-amber-50 text-amber-700 flex items-center justify-center">
              <AlertTriangle className="w-5 h-5" />
            </div>
            <h3 className="text-base font-bold text-stone-900">
              {isAr ? '2. قاعدة الصفر تخمين (Zero Hallucination)' : '2. Zéro extrapolation'}
            </h3>
            <p className="text-xs sm:text-sm text-stone-600 leading-relaxed">
              {isAr
                ? 'إذا لم يتوفر رقم هاتف رسمي أو عنوان بريد موثق في المنشورات العامة، نتركه فارغاً ونشير إلى أنه "غير متاح"، بدلاً من اختلاق أرقام افتراضية.'
                : 'Tout numéro de téléphone, courriel ou modalité d’admission non publié dans les sources officielles est expressément étiqueté "Non disponible".'}
            </p>
          </div>

          <div className="bg-white p-6 rounded-xl border border-stone-200 space-y-2.5 shadow-xs">
            <div className="w-9 h-9 rounded-lg bg-blue-50 text-blue-700 flex items-center justify-center">
              <EyeOff className="w-5 h-5" />
            </div>
            <h3 className="text-base font-bold text-stone-900">
              {isAr ? '3. عدم جمع بيانات شخصية للطلبة' : '3. Zéro collecte de données personnelles'}
            </h3>
            <p className="text-xs sm:text-sm text-stone-600 leading-relaxed">
              {isAr
                ? 'المنصة لا تطلب كلمات مرور، ولا تطلب تسجيل الدخول بحسابات شخصية، ولا تجمع أي بريد إلكتروني إجباري. المنصة دليل استكشافي عام ومفتوح.'
                : 'Aucun identifiant étudiant, mot de passe ou donnée confidentielle n’est sollicité ou stocké sur ce prototype public.'}
            </p>
          </div>

          <div className="bg-white p-6 rounded-xl border border-stone-200 space-y-2.5 shadow-xs">
            <div className="w-9 h-9 rounded-lg bg-stone-100 text-stone-700 flex items-center justify-center">
              <Lock className="w-5 h-5" />
            </div>
            <h3 className="text-base font-bold text-stone-900">
              {isAr ? '4. تصنيف حالات البيانات الصريح' : '4. Statuts de traçabilité explicites'}
            </h3>
            <p className="text-xs sm:text-sm text-stone-600 leading-relaxed">
              {isAr
                ? 'يحمل كل سجل علامة واضحة: "موثق بمصدر رسمي" (Verified)، "قيد التدقيق" (Needs Review)، أو "نموذج تجريبي" (Demo Seed) لتأكيد حدود النموذج.'
                : 'Chaque fiche indique clairement : Vérifié (Verified), À réviser (Needs Review) ou Prototype démo (Demo Seed), avec date de contrôle.'}
            </p>
          </div>
        </div>

        {/* Official Disclaimer Banner */}
        <div className="bg-[#FAF4ED] border border-[#E9DFCE] rounded-xl p-5 sm:p-6 text-stone-800 space-y-2">
          <div className="flex items-center gap-2 font-bold text-[#B43425] text-sm">
            <FileText className="w-4 h-4" />
            <span>إشعار قانوني واستقلالية النموذج (Disclaimer)</span>
          </div>
          <p className="text-xs sm:text-sm leading-relaxed text-stone-700">
            {isAr
              ? 'مشروع "جامعتي | Jami3ati" هو نموذج أولي مطور في إطار هاكاثون الابتكار البرمجي. المنصة مستقلة تمامًا وغير حكومية، ولا تمثل وزارة التعليم العالي والبحث العلمي والابتكار أو أي جامعة مغربية، ولا تدعي أي اتفاق أو شراكة رسمية مسبقة. تظل المواقع الرسمية للجامعات هي المرجع الإداري والقانوني الوحيد لكافة المساطر الجامعية.'
              : 'Le projet "Jami3ati" est un prototype expérimental conçu dans le cadre d’un hackathon d’innovation publique. Il est indépendant, non gouvernemental, et ne prétend aucunement représenter une université ou un ministère. Les portails officiels des universités demeurent l’unique référence légale.'}
          </p>
        </div>
      </div>
    </section>
  );
};
