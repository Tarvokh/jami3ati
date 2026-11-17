import React, { useEffect, useState } from 'react';
import { Language } from '../types';
import {
  ApiError,
  listInstitutions,
  subscribe,
  type InstitutionRecord,
  type SubscriptionRecord,
} from '../lib/api';
import {
  AlertTriangle,
  Bell,
  CheckCircle2,
  Loader2,
  Mail,
  Send,
} from 'lucide-react';

interface Props {
  lang: Language;
}

type Phase = 'idle' | 'submitting' | 'sent' | 'error';

function describeError(error: unknown, lang: Language): string {
  const isAr = lang === 'ar';
  if (error instanceof ApiError) {
    if (error.isNetworkError) {
      return isAr
        ? 'تعذّر الاتصال بالخادم. تأكد من أن الـAPI يعمل على المنفذ 8000.'
        : 'Serveur injoignable. Vérifiez que l’API écoute sur le port 8000.';
    }
    return error.message;
  }
  return isAr ? 'خطأ غير متوقع.' : 'Erreur inattendue.';
}

export const AcademicAlertsSubscription: React.FC<Props> = ({ lang }) => {
  const isAr = lang === 'ar';

  const [email, setEmail] = useState('');
  const [institutionId, setInstitutionId] = useState('');
  const [institutions, setInstitutions] = useState<InstitutionRecord[]>([]);
  const [institutionsFailed, setInstitutionsFailed] = useState(false);
  const [phase, setPhase] = useState<Phase>('idle');
  const [errorMessage, setErrorMessage] = useState('');
  const [created, setCreated] = useState<SubscriptionRecord | null>(null);

  // The institution list is real: it comes from the same table the archive
  // reads. If it fails, the form still works for "all institutions" and says so.
  useEffect(() => {
    let cancelled = false;
    listInstitutions()
      .then((rows) => {
        if (!cancelled) setInstitutions(rows);
      })
      .catch(() => {
        if (!cancelled) setInstitutionsFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const handleSubscribe = async (e: React.FormEvent) => {
    e.preventDefault();
    const address = email.trim();
    if (!address) return;

    setPhase('submitting');
    setErrorMessage('');
    try {
      const record = await subscribe({
        email: address,
        institution_id: institutionId || null,
      });
      setCreated(record);
      setPhase('sent');
    } catch (error) {
      setErrorMessage(describeError(error, lang));
      setPhase('error');
    }
  };

  const resetForm = () => {
    setCreated(null);
    setPhase('idle');
    setErrorMessage('');
  };

  return (
    <div className="bg-[#FAF7F0] border border-[#EDE7D9] rounded-2xl p-6 sm:p-8 relative overflow-hidden shadow-xs">
      <div className="flex flex-col lg:flex-row items-start lg:items-center justify-between gap-6">
        {/* Text Header */}
        <div className="space-y-2 max-w-xl">
          <div className="inline-flex items-center gap-1.5 px-3 py-1 rounded-md bg-[#143D30]/10 text-[#143D30] text-xs font-semibold">
            <Bell className="w-3.5 h-3.5 text-[#143D30]" />
            <span>{isAr ? 'تنبيهات بريدية قابلة للتحقق' : 'Alertes e-mail vérifiables'}</span>
          </div>
          <h3 className="text-xl sm:text-2xl font-bold text-stone-900 tracking-tight">
            {isAr ? 'اشترك في تنبيهات المواعيد الجامعية' : 'Abonnez-vous aux alertes des concours'}
          </h3>
          <p className="text-xs sm:text-sm text-stone-600 leading-relaxed">
            {isAr
              ? 'رسالة واحدة لكل تغيير موثّق في المؤسسات التي تختارها، مع رابط المصدر وتاريخ التحقق. الاشتراك لا يُفعَّل إلا بعد ضغطك على رابط التأكيد (double opt-in)، ويمكنك إلغاؤه من كل رسالة.'
              : 'Un seul e-mail par changement vérifié dans les établissements choisis, avec le lien source et la date de vérification. L’abonnement n’est activé qu’après le clic sur le lien de confirmation (double opt-in), et se résilie depuis chaque message.'}
          </p>
        </div>

        {/* Form or Result Card */}
        <div className="w-full lg:w-auto shrink-0 max-w-md">
          {phase === 'sent' && created ? (
            <div
              className="bg-white p-5 rounded-xl border border-emerald-300 shadow-sm space-y-3 animate-in fade-in zoom-in-95 duration-150"
              aria-live="polite"
            >
              <div className="flex items-center gap-2 text-emerald-800 font-bold text-sm">
                <CheckCircle2 className="w-5 h-5 text-emerald-600 shrink-0" />
                <span>{isAr ? 'تحقّق من بريدك' : 'Vérifiez votre boîte mail'}</span>
              </div>
              <p className="text-xs text-stone-700 leading-relaxed">
                {isAr
                  ? `أرسلنا رسالة تأكيد إلى ${created.email}. الاشتراك ما زال pending: لا يصبح نشطًا إلا بعد فتح الرابط.`
                  : `Un message de confirmation a été envoyé à ${created.email}. L’abonnement reste « pending » jusqu’au clic sur le lien.`}
              </p>

              <div className="flex items-center justify-between gap-2 px-3 py-2 bg-stone-50 border border-stone-200 rounded-lg">
                <span className="text-[10px] uppercase tracking-wider text-stone-500">
                  {isAr ? 'رقم الاشتراك' : 'Référence'}
                </span>
                <span className="font-mono text-[11px] text-[#143D30]">{created.id}</span>
              </div>

              <div className="p-3 bg-amber-50 border border-amber-200/90 rounded-lg text-[11px] text-amber-900 flex items-start gap-2">
                <AlertTriangle className="w-4 h-4 text-amber-700 shrink-0 mt-0.5" />
                <p>
                  {isAr
                    ? 'في هذا العرض المحلي يصل البريد إلى صندوق اختبار (SMTP sink) على نفس الجهاز، وليس إلى صندوقك الشخصي.'
                    : 'Dans cette démo locale, l’e-mail arrive dans un puits de test SMTP sur la même machine, pas dans votre boîte personnelle.'}
                </p>
              </div>

              <button
                onClick={resetForm}
                className="text-xs text-[#143D30] font-semibold hover:underline"
              >
                {isAr ? 'تسجيل بريد آخر' : 'Inscrire un autre contact'}
              </button>
            </div>
          ) : (
            <form
              onSubmit={handleSubscribe}
              aria-busy={phase === 'submitting'}
              className="bg-white p-4 sm:p-5 rounded-xl border border-stone-200 shadow-sm space-y-3 text-xs"
            >
              <div>
                <label htmlFor="alert-email" className="block text-[11px] text-stone-500 mb-1">
                  {isAr ? 'البريد الإلكتروني' : 'Adresse e-mail'}
                </label>
                <div className="relative">
                  <Mail className="w-4 h-4 text-stone-400 absolute start-3 top-2.5" />
                  <input
                    id="alert-email"
                    type="email"
                    required
                    autoComplete="email"
                    placeholder="etudiant@example.ma"
                    value={email}
                    onChange={(e) => setEmail(e.target.value)}
                    disabled={phase === 'submitting'}
                    className="w-full ps-9 pe-3 py-2 bg-stone-50 border border-stone-300 rounded-lg text-stone-900 focus:outline-none focus:ring-1 focus:ring-[#143D30] disabled:opacity-60"
                  />
                </div>
              </div>

              <div>
                <label htmlFor="alert-institution" className="block text-[11px] text-stone-500 mb-1">
                  {isAr ? 'المؤسسة' : 'Établissement'}
                </label>
                <select
                  id="alert-institution"
                  value={institutionId}
                  onChange={(e) => setInstitutionId(e.target.value)}
                  disabled={phase === 'submitting'}
                  className="w-full px-2 py-2 bg-stone-50 border border-stone-200 rounded-md text-[11px] disabled:opacity-60"
                >
                  <option value="">{isAr ? 'كل المؤسسات' : 'Tous les établissements'}</option>
                  {institutions.map((institution) => (
                    <option key={institution.id} value={institution.id}>
                      {isAr ? institution.name_ar : institution.name_fr} —{' '}
                      {isAr ? institution.city_ar : institution.city_fr}
                    </option>
                  ))}
                </select>
                {institutionsFailed && (
                  <span className="text-[10px] text-amber-800 mt-1 block">
                    {isAr
                      ? 'تعذّر تحميل قائمة المؤسسات؛ يمكنك الاشتراك في كل المؤسسات.'
                      : 'Liste des établissements indisponible ; vous pouvez vous abonner à tous.'}
                  </span>
                )}
              </div>

              {phase === 'error' && (
                <div
                  className="flex items-start gap-2 p-2.5 bg-red-50 border border-red-200 text-red-700 rounded-lg text-[11px]"
                  aria-live="polite"
                >
                  <AlertTriangle className="w-4 h-4 shrink-0 mt-0.5" />
                  <span>{errorMessage}</span>
                </div>
              )}

              <button
                type="submit"
                disabled={phase === 'submitting'}
                className="w-full py-2.5 bg-[#143D30] hover:bg-[#0f2e24] text-white font-semibold rounded-lg transition-colors flex items-center justify-center gap-2 shadow-2xs disabled:opacity-60 disabled:cursor-not-allowed"
              >
                {phase === 'submitting' ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <Send className="w-3.5 h-3.5" />
                )}
                <span>
                  {phase === 'submitting'
                    ? isAr
                      ? 'جارٍ الإرسال…'
                      : 'Envoi…'
                    : isAr
                      ? 'تفعيل التنبيهات'
                      : 'Activer les alertes'}
                </span>
              </button>

              <p className="text-[10px] text-stone-500 leading-relaxed">
                {isAr
                  ? 'لا نشارك بريدك. رسالة تأكيد واحدة، ثم لا شيء حتى يقع تغيير موثّق.'
                  : 'Votre adresse n’est pas partagée. Un e-mail de confirmation, puis rien avant un changement vérifié.'}
              </p>
            </form>
          )}
        </div>
      </div>
    </div>
  );
};
