import React, { useState } from 'react';
import { University, Institution, Language, InaccuracyReport } from '../types';
import { translations } from '../data/translations';
import { X, CheckCircle2, AlertOctagon, Send, ShieldAlert, Loader2 } from 'lucide-react';
import { ModalBackdrop } from './ModalBackdrop';
import { useEscapeKey } from '../hooks/useEscapeKey';
import { ApiError } from '../lib/api';

interface Props {
  isOpen: boolean;
  targetItem: University | Institution | null;
  lang: Language;
  onClose: () => void;
  /** Persists the report and resolves with the stored row (its id is the ticket). */
  onSubmitReport: (
    report: Omit<InaccuracyReport, 'id' | 'created_at' | 'status'>
  ) => Promise<InaccuracyReport>;
}

export const ReportModal: React.FC<Props> = ({
  isOpen,
  targetItem,
  lang,
  onClose,
  onSubmitReport,
}) => {
  const t = translations[lang];

  const [fieldName, setFieldName] = useState('رابط الموقع الرسمي / Website URL');
  const [proposedCorrection, setProposedCorrection] = useState('');
  const [sourceProofUrl, setSourceProofUrl] = useState('');
  const [userNotes, setUserNotes] = useState('');
  const [reporterEmail, setReporterEmail] = useState('');
  const [submittedTicket, setSubmittedTicket] = useState<string | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleResetAndClose = () => {
    setSubmittedTicket(null);
    setProposedCorrection('');
    setSourceProofUrl('');
    setUserNotes('');
    setReporterEmail('');
    setErrorMsg(null);
    setIsSubmitting(false);
    onClose();
  };

  // Listen for Escape key globally to close this modal
  useEscapeKey(handleResetAndClose, isOpen);

  if (!isOpen) return null;

  const targetName = targetItem
    ? `${lang === 'ar' ? targetItem.name_ar : targetItem.name_fr} (${targetItem.code})`
    : 'مؤسسة عامة / دليل عام';

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setErrorMsg(null);

    if (!proposedCorrection.trim()) {
      setErrorMsg(lang === 'ar' ? 'يرجى إدخال التصحيح المقترح.' : 'Veuillez saisir la correction proposée.');
      return;
    }

    if (!sourceProofUrl.trim() || !sourceProofUrl.startsWith('http')) {
      setErrorMsg(
        lang === 'ar'
          ? 'يرجى تقديم رابط مصدر رسمي صالح يبدأ بـ http:// أو https:// لإثبات التصحيح.'
          : 'Veuillez renseigner un lien source officiel valide commençant par http:// ou https://.'
      );
      return;
    }

    setIsSubmitting(true);
    try {
      const stored = await onSubmitReport({
        target_id: targetItem ? targetItem.id : 'general',
        target_name: targetName,
        field_name: fieldName,
        current_value: targetItem?.phone || targetItem?.website_url || 'N/A',
        proposed_correction: proposedCorrection,
        source_proof_url: sourceProofUrl,
        user_notes: userNotes,
        reporter_email: reporterEmail,
      });
      // The ticket is the id the server stored, not a number minted in the browser.
      setSubmittedTicket(stored.id);
    } catch (error) {
      if (error instanceof ApiError && error.isNetworkError) {
        setErrorMsg(
          lang === 'ar'
            ? 'تعذّر الاتصال بالخادم. لم يُسجَّل البلاغ؛ أعد المحاولة بعد تشغيل الـAPI.'
            : 'Serveur injoignable. Le signalement n’a pas été enregistré ; réessayez une fois l’API démarrée.'
        );
      } else {
        setErrorMsg(
          error instanceof Error
            ? error.message
            : lang === 'ar'
              ? 'تعذّر تسجيل البلاغ.'
              : 'Le signalement n’a pas pu être enregistré.'
        );
      }
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <ModalBackdrop onClose={handleResetAndClose} ariaLabelledBy="report-modal-title">
      <div className="relative w-full max-w-xl max-h-[90vh] my-auto flex flex-col bg-white rounded-2xl shadow-2xl border border-stone-200 overflow-hidden select-text animate-in zoom-in-95 duration-150">
        {/* Modal Top Bar */}
        <div className="px-6 py-4 bg-[#FAF7F0] border-b border-[#E7E2D6] flex items-center justify-between shrink-0">
          <div className="flex items-center gap-2 text-stone-900 font-semibold">
            <ShieldAlert className="w-5 h-5 text-[#B43425]" />
            <h3 id="report-modal-title" className="text-base sm:text-lg">{t.report_modal_title}</h3>
          </div>
          <button
            onClick={handleResetAndClose}
            className="p-1.5 text-stone-500 hover:text-stone-900 rounded-lg hover:bg-stone-200/60 transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {submittedTicket ? (
          <div className="p-6 sm:p-8 text-center space-y-4 overflow-y-auto overscroll-contain flex-1">
            <div className="w-12 h-12 bg-emerald-100 text-emerald-700 rounded-full flex items-center justify-center mx-auto">
              <CheckCircle2 className="w-7 h-7" />
            </div>
            <h4 className="text-lg font-bold text-stone-900">{t.report_success}</h4>
            <div className="p-4 bg-stone-50 border border-stone-200 rounded-xl inline-block max-w-sm w-full">
              <span className="text-xs text-stone-500 block">{t.ticket_id}</span>
              <span className="font-mono text-lg font-bold text-[#143D30] tracking-wider">
                {submittedTicket}
              </span>
            </div>
            <p className="text-xs text-stone-500 max-w-md mx-auto">
              {lang === 'ar'
                ? 'سُجّل البلاغ بحالة pending. لا يُعتمد أي تصحيح إلا بعد مطابقة الرابط الرسمي من قبل مشرفي المنصة.'
                : 'Signalement enregistré avec le statut « pending ». Aucune correction n’est validée avant vérification manuelle du lien officiel.'}
            </p>
            <div className="pt-2">
              <button
                onClick={handleResetAndClose}
                className="px-6 py-2.5 bg-[#143D30] text-white text-sm font-medium rounded-lg hover:bg-[#0f2e24] transition-colors"
              >
                {t.close}
              </button>
            </div>
          </div>
        ) : (
          <form onSubmit={handleSubmit} className="p-6 space-y-4 text-sm text-stone-800 overflow-y-auto overscroll-contain flex-1">
            <p className="text-xs text-stone-600 bg-amber-50/80 border border-amber-200/80 rounded-lg p-3">
              {t.report_modal_desc}
            </p>

            {/* Target institution name */}
            <div>
              <label className="block text-xs font-medium text-stone-700 mb-1">
                المؤسسة المعنية / Établissement
              </label>
              <input
                type="text"
                disabled
                value={targetName}
                className="w-full px-3 py-2 bg-stone-100 border border-stone-200 rounded-lg text-stone-800 text-xs sm:text-sm font-medium cursor-not-allowed"
              />
            </div>

            {/* Field */}
            <div>
              <label className="block text-xs font-medium text-stone-700 mb-1">
                {t.field_to_correct}
              </label>
              <select
                value={fieldName}
                onChange={(e) => setFieldName(e.target.value)}
                className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg text-stone-900 focus:outline-none focus:ring-2 focus:ring-[#143D30] text-sm"
              >
                <option value="رابط الموقع الرسمي / Website URL">رابط الموقع الرسمي / URL officiel</option>
                <option value="الهاتف المعتمد / Téléphone">رقم الهاتف الرسمي المعتمد / Téléphone</option>
                <option value="البريد الإلكتروني / Email">البريد الإلكتروني المعتمد / Courriel</option>
                <option value="العلاقة والتبعية الجامعية / Affiliation">التبعية للجامعة الأم / Rattachement universitaire</option>
                <option value="المسلك أو الشعبة التكوينية / Formation">شعبة أو مسلك تكويني / Filière</option>
                <option value="الموقع أو العنوان / Adresse">العنوان أو الحي الجامعي / Adresse du campus</option>
              </select>
            </div>

            {/* Proposed correction */}
            <div>
              <label className="block text-xs font-medium text-stone-700 mb-1">
                {t.proposed_correction} <span className="text-[#B43425]">*</span>
              </label>
              <input
                type="text"
                required
                placeholder={lang === 'ar' ? 'اكتب المعلومة الصحيحة بدقة...' : 'Saisissez la valeur exacte corrigée...'}
                value={proposedCorrection}
                onChange={(e) => setProposedCorrection(e.target.value)}
                className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg text-stone-900 focus:outline-none focus:ring-2 focus:ring-[#143D30] text-sm"
              />
            </div>

            {/* Proof source URL */}
            <div>
              <label className="block text-xs font-medium text-stone-700 mb-1">
                {t.proof_source_url} <span className="text-[#B43425]">*</span>
              </label>
              <input
                type="url"
                required
                placeholder="https://..."
                value={sourceProofUrl}
                onChange={(e) => setSourceProofUrl(e.target.value)}
                className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg text-stone-900 focus:outline-none focus:ring-2 focus:ring-[#143D30] font-mono text-xs sm:text-sm"
              />
              <span className="text-[11px] text-stone-500 mt-1 block">
                {lang === 'ar'
                  ? 'رابط منشور بمرسوم رسمي، موقع data.gov.ma، أو صفحة تواصل رسمية بموقع المؤسسة.'
                  : 'Lien d’un décret, arrêté, publication data.gov.ma ou page officielle de contact.'}
              </span>
            </div>

            {/* Optional Notes */}
            <div>
              <label className="block text-xs font-medium text-stone-700 mb-1">
                {t.notes_optional}
              </label>
              <textarea
                rows={2}
                placeholder={lang === 'ar' ? 'سياق إضافي أو توضيح...' : 'Contexte ou détails additionnels...'}
                value={userNotes}
                onChange={(e) => setUserNotes(e.target.value)}
                className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg text-stone-900 focus:outline-none focus:ring-2 focus:ring-[#143D30] text-xs sm:text-sm resize-none"
              />
            </div>

            {errorMsg && (
              <div className="flex items-center gap-2 p-2.5 bg-red-50 border border-red-200 text-red-700 rounded-lg text-xs">
                <AlertOctagon className="w-4 h-4 shrink-0" />
                <span>{errorMsg}</span>
              </div>
            )}

            {/* Actions */}
            <div className="flex items-center justify-end gap-3 pt-3 border-t border-stone-200">
              <button
                type="button"
                onClick={handleResetAndClose}
                className="px-4 py-2 text-xs font-medium text-stone-600 hover:text-stone-900"
              >
                {t.close}
              </button>
              <button
                type="submit"
                disabled={isSubmitting}
                className="inline-flex items-center gap-2 px-5 py-2.5 bg-[#B43425] hover:bg-[#922619] text-white text-xs sm:text-sm font-medium rounded-lg transition-colors shadow-xs disabled:opacity-60 disabled:cursor-not-allowed"
              >
                {isSubmitting ? (
                  <Loader2 className="w-4 h-4 animate-spin" />
                ) : (
                  <Send className="w-4 h-4" />
                )}
                <span>
                  {isSubmitting
                    ? lang === 'ar'
                      ? 'جارٍ الإرسال…'
                      : 'Envoi…'
                    : t.submit_report}
                </span>
              </button>
            </div>
          </form>
        )}
      </div>
    </ModalBackdrop>
  );
};
