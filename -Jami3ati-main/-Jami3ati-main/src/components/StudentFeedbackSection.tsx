import React, { useState, useMemo } from 'react';
import { StudentFeedback, FeedbackServiceCategory, University, Institution, Language } from '../types';
import { translations } from '../data/translations';
import { ModalBackdrop } from './ModalBackdrop';
import { useEscapeKey } from '../hooks/useEscapeKey';
import {
  Star,
  MessageSquare,
  ShieldCheck,
  Plus,
  CheckCircle2,
  AlertCircle,
  X,
  Send,
  Building2,
  GraduationCap,
  Sparkles,
  Lock,
} from 'lucide-react';

interface Props {
  feedbacks: StudentFeedback[];
  universities: University[];
  institutions: Institution[];
  lang: Language;
  onAddFeedback: (feedback: Omit<StudentFeedback, 'id' | 'created_at' | 'status' | 'moderated_at'>) => void;
}

export const StudentFeedbackSection: React.FC<Props> = ({
  feedbacks,
  universities,
  institutions,
  lang,
  onAddFeedback,
}) => {
  const t = translations[lang];
  const isAr = lang === 'ar';

  const [selectedCategory, setSelectedCategory] = useState<'all' | FeedbackServiceCategory>('all');
  const [selectedInstFilter, setSelectedInstFilter] = useState<string>('all');
  const [isModalOpen, setIsModalOpen] = useState(false);

  // Form states
  const [targetId, setTargetId] = useState<string>(institutions[0]?.id || universities[0]?.id || '');
  const [category, setCategory] = useState<FeedbackServiceCategory>('digital_services');
  const [rating, setRating] = useState<number>(5);
  const [comment, setComment] = useState('');
  const [studentLevel, setStudentLevel] = useState('طالب بالسنة الأولى / Étudiant 1re année');
  const [policyAccepted, setPolicyAccepted] = useState(false);
  const [submittedNotice, setSubmittedNotice] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Filter approved feedbacks for public display
  const approvedFeedbacks = useMemo(() => {
    return feedbacks.filter((f) => {
      const isApproved = f.status === 'approved';
      const matchCat = selectedCategory === 'all' || f.service_category === selectedCategory;
      const matchInst = selectedInstFilter === 'all' || f.institution_id === selectedInstFilter;
      return isApproved && matchCat && matchInst;
    });
  }, [feedbacks, selectedCategory, selectedInstFilter]);

  // Overall average rating
  const avgRating = useMemo(() => {
    const list = feedbacks.filter((f) => f.status === 'approved');
    if (!list.length) return 5.0;
    const sum = list.reduce((acc, curr) => acc + curr.rating, 0);
    return (sum / list.length).toFixed(1);
  }, [feedbacks]);

  // Escape key for modal
  useEscapeKey(() => setIsModalOpen(false), isModalOpen);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setErrorMsg(null);

    if (!comment.trim() || comment.trim().length < 10) {
      setErrorMsg(isAr ? 'يرجى كتابة رأي بنّاء لا يقل عن 10 أحرف.' : 'Veuillez saisir un avis constructif d’au moins 10 caractères.');
      return;
    }

    if (!policyAccepted) {
      setErrorMsg(isAr ? 'يجب الموافقة على ميثاق النزاهة وعدم نشر بيانات شخصية.' : 'Veuillez accepter la charte d’intégrité.');
      return;
    }

    // Find target institution or university
    const targetInst = institutions.find((i) => i.id === targetId);
    const targetUniv = universities.find((u) => u.id === targetId);

    const instCode = targetInst ? targetInst.code : targetUniv ? targetUniv.code : 'INST';
    const instNameAr = targetInst ? targetInst.name_ar : targetUniv ? targetUniv.name_ar : 'مؤسسة جامعية';
    const instNameFr = targetInst ? targetInst.name_fr : targetUniv ? targetUniv.name_fr : 'Établissement';

    const categoryLabels: { [key in FeedbackServiceCategory]: { ar: string; fr: string } } = {
      digital_services: { ar: 'المنصات الرقمية وفضاء الطالب', fr: 'Plateformes numériques & ENT' },
      library: { ar: 'المكتبة وفضاءات العمل والابتكار', fr: 'Bibliothèque & Espaces d’études' },
      administration: { ar: 'الشؤون الطلابية والمصالح الإدارية', fr: 'Scolarité & Démarches administratives' },
      student_life: { ar: 'الحياة الطلابية والأندية والمرافق', fr: 'Vie étudiante & Clubs' },
    };

    onAddFeedback({
      institution_id: targetId,
      institution_code: instCode,
      institution_name_ar: instNameAr,
      institution_name_fr: instNameFr,
      service_category: category,
      service_label_ar: categoryLabels[category].ar,
      service_label_fr: categoryLabels[category].fr,
      rating,
      comment_ar: comment,
      comment_fr: comment,
      student_level_ar: studentLevel,
      student_level_fr: studentLevel,
    });

    setSubmittedNotice(true);
  };

  const handleCloseModal = () => {
    setIsModalOpen(false);
    setSubmittedNotice(false);
    setComment('');
    setPolicyAccepted(false);
    setErrorMsg(null);
  };

  return (
    <section id="student-feedback" className="py-14 border-t border-stone-200 bg-white w-full max-w-full overflow-x-hidden">
      <div className="max-w-6xl mx-auto px-4 sm:px-6 w-full">
        {/* Section Header */}
        <div className="flex flex-col md:flex-row md:items-end justify-between gap-6 mb-8">
          <div>
            <div className="inline-flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-[#143D30] mb-1.5">
              <MessageSquare className="w-4 h-4 text-emerald-700" />
              <span>تقييمات مجهولة الهوية وخاضعة للتدقيق المسبق</span>
            </div>
            <h2 className="text-2xl sm:text-3xl font-bold text-stone-900 tracking-tight">
              {isAr ? 'آراء وتجارب الطلبة للخدمات الجامعية' : 'Retours d’expérience & avis des étudiants'}
            </h2>
            <p className="text-xs sm:text-sm text-stone-600 mt-1 max-w-2xl leading-relaxed">
              {isAr
                ? 'فضاء مخصص لتقييم جودة الخدمات (المنصات الرقمية، المكتبات، الشؤون الإدارية) بهوية مجهولة 100% ودون جمع أي بيانات شخصية، مع إخضاع كل مشاركة للتدقيق المسبق.'
                : 'Évaluations anonymes sur la qualité des services universitaires (ENT, bibliothèque, scolarité), modérées a priori dans le strict respect de la charte de confidentialité.'}
            </p>
          </div>

          {/* Quick Action Button & Rating KPI */}
          <div className="flex items-center gap-3 shrink-0">
            <div className="text-end hidden sm:block">
              <div className="flex items-center gap-1 font-mono font-bold text-lg text-[#143D30]">
                <Star className="w-5 h-5 fill-amber-400 text-amber-400" />
                <span>{avgRating} / 5</span>
              </div>
              <span className="text-[11px] text-stone-400">معدل رضا الطلبة</span>
            </div>

            <button
              onClick={() => setIsModalOpen(true)}
              className="inline-flex items-center gap-2 px-4 py-2.5 rounded-xl bg-[#143D30] hover:bg-[#0f2e24] text-white text-xs sm:text-sm font-semibold transition-colors shadow-xs"
            >
              <Plus className="w-4 h-4" />
              <span>{isAr ? 'إضافة تقييم مجهول' : 'Ajouter un avis anonyme'}</span>
            </button>
          </div>
        </div>

        {/* Filter Strip */}
        <div className="flex flex-wrap items-center justify-between gap-3 p-3 bg-stone-50 rounded-xl border border-stone-200 mb-6 text-xs font-medium">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="text-stone-500 me-1">الخدمة:</span>
            <button
              onClick={() => setSelectedCategory('all')}
              className={`px-3 py-1.5 rounded-lg transition-colors ${
                selectedCategory === 'all'
                  ? 'bg-white text-stone-900 shadow-xs font-bold'
                  : 'text-stone-600 hover:text-stone-900'
              }`}
            >
              الكل ({feedbacks.filter((f) => f.status === 'approved').length})
            </button>
            <button
              onClick={() => setSelectedCategory('digital_services')}
              className={`px-3 py-1.5 rounded-lg transition-colors ${
                selectedCategory === 'digital_services'
                  ? 'bg-white text-stone-900 shadow-xs font-bold'
                  : 'text-stone-600 hover:text-stone-900'
              }`}
            >
              فضاء الطالب الرقمي (ENT)
            </button>
            <button
              onClick={() => setSelectedCategory('library')}
              className={`px-3 py-1.5 rounded-lg transition-colors ${
                selectedCategory === 'library'
                  ? 'bg-white text-stone-900 shadow-xs font-bold'
                  : 'text-stone-600 hover:text-stone-900'
              }`}
            >
              المكتبة والمرافق
            </button>
            <button
              onClick={() => setSelectedCategory('administration')}
              className={`px-3 py-1.5 rounded-lg transition-colors ${
                selectedCategory === 'administration'
                  ? 'bg-white text-stone-900 shadow-xs font-bold'
                  : 'text-stone-600 hover:text-stone-900'
              }`}
            >
              الشؤون الإدارية
            </button>
            <button
              onClick={() => setSelectedCategory('student_life')}
              className={`px-3 py-1.5 rounded-lg transition-colors ${
                selectedCategory === 'student_life'
                  ? 'bg-white text-stone-900 shadow-xs font-bold'
                  : 'text-stone-600 hover:text-stone-900'
              }`}
            >
              الحياة الطلابية والأندية
            </button>
          </div>

          <div className="text-[11px] text-stone-500 font-mono flex items-center gap-1">
            <Lock className="w-3.5 h-3.5 text-stone-400" />
            <span>مجهول 100% · لا كوكيز ولا جمع لهويات الطلبة</span>
          </div>
        </div>

        {/* Feedback Cards Grid */}
        {approvedFeedbacks.length === 0 ? (
          <div className="p-12 text-center text-stone-500 text-xs sm:text-sm bg-stone-50 rounded-xl border border-stone-200">
            لا توجد تقييمات منشورة ضمن هذا التصنيف حالياً. كن أول من يشارك تجربة مجهولة!
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
            {approvedFeedbacks.map((item) => (
              <div
                key={item.id}
                className="p-5 rounded-xl border border-stone-200 bg-[#FCFBF8] flex flex-col justify-between hover:border-stone-300 transition-colors shadow-xs"
              >
                <div>
                  <div className="flex items-center justify-between gap-2 mb-2 pb-2 border-b border-stone-100">
                    <div className="flex items-center gap-1.5">
                      <span className="font-mono text-xs font-bold text-[#143D30] bg-[#FAF7F0] px-2 py-0.5 rounded border border-[#E7E2D6]">
                        {item.institution_code}
                      </span>
                      <span className="text-xs text-stone-700 font-semibold truncate max-w-[180px]">
                        {isAr ? item.institution_name_ar : item.institution_name_fr}
                      </span>
                    </div>

                    {/* Star Rating Display */}
                    <div className="flex items-center gap-0.5">
                      {[1, 2, 3, 4, 5].map((star) => (
                        <Star
                          key={star}
                          className={`w-3.5 h-3.5 ${
                            star <= item.rating
                              ? 'fill-amber-400 text-amber-400'
                              : 'text-stone-300'
                          }`}
                        />
                      ))}
                    </div>
                  </div>

                  {/* Service tag & student persona */}
                  <div className="flex items-center gap-2 text-[11px] text-stone-500 mb-3">
                    <span className="text-emerald-800 font-medium">
                      {isAr ? item.service_label_ar : item.service_label_fr}
                    </span>
                    <span aria-hidden="true">·</span>
                    <span>{isAr ? item.student_level_ar : item.student_level_fr}</span>
                  </div>

                  {/* Comment text */}
                  <p className="text-xs sm:text-sm text-stone-700 leading-relaxed mb-4">
                    « {isAr ? item.comment_ar : item.comment_fr} »
                  </p>
                </div>

                {/* Footer with verification note */}
                <div className="pt-2.5 border-t border-stone-100 flex items-center justify-between text-[11px] text-stone-400">
                  <span className="font-mono">{item.created_at}</span>
                  <span className="inline-flex items-center gap-1 text-emerald-700 font-medium">
                    <ShieldCheck className="w-3.5 h-3.5" />
                    <span>تم التدقيق المسبق والمطابقة</span>
                  </span>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Submission Modal */}
      {isModalOpen && (
        <ModalBackdrop onClose={handleCloseModal} ariaLabelledBy="feedback-modal-title">
          <div className="relative w-full max-w-xl max-h-[90vh] my-auto flex flex-col bg-white rounded-2xl shadow-2xl border border-stone-200 overflow-hidden select-text animate-in zoom-in-95 duration-150">
            {/* Modal Header */}
            <div className="px-6 py-4 bg-[#FAF7F0] border-b border-[#E7E2D6] flex items-center justify-between shrink-0">
              <div className="flex items-center gap-2 text-stone-900 font-bold">
                <MessageSquare className="w-5 h-5 text-[#143D30]" />
                <h3 id="feedback-modal-title" className="text-base sm:text-lg">
                  {isAr ? 'مشاركة تجربة أو تقييم مجهول للخدمات' : 'Partager un avis anonyme'}
                </h3>
              </div>
              <button
                onClick={handleCloseModal}
                className="p-1.5 text-stone-500 hover:text-stone-900 rounded-lg hover:bg-stone-200/60 transition-colors"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            {submittedNotice ? (
              <div className="p-8 text-center space-y-4 overflow-y-auto overscroll-contain flex-1">
                <div className="w-12 h-12 bg-emerald-100 text-emerald-700 rounded-full flex items-center justify-center mx-auto">
                  <CheckCircle2 className="w-7 h-7" />
                </div>
                <h4 className="text-lg font-bold text-stone-900">
                  {isAr ? 'تم استلام تقييمك بنجاح!' : 'Votre retour a été enregistré avec succès !'}
                </h4>
                <p className="text-xs sm:text-sm text-stone-600 max-w-md mx-auto leading-relaxed">
                  {isAr
                    ? 'احتراماً لميثاق البيانات والنزاهة، يدخل التقييم طابور التدقيق المسبق وسيُنشر تلقائياً بعد التحقق من خلوه من أي بيانات شخصية أو إشاعات.'
                    : 'Conformément à la politique d’intégrité, votre avis est en cours de modération et sera publié dès vérification.'}
                </p>
                <div className="pt-2">
                  <button
                    onClick={handleCloseModal}
                    className="px-6 py-2.5 bg-[#143D30] text-white text-xs sm:text-sm font-semibold rounded-lg hover:bg-[#0f2e24] transition-colors"
                  >
                    {t.close}
                  </button>
                </div>
              </div>
            ) : (
              <form onSubmit={handleSubmit} className="p-6 space-y-4 text-xs sm:text-sm text-stone-800 overflow-y-auto overscroll-contain flex-1">
                <div className="p-3 bg-amber-50/80 border border-amber-200/80 rounded-xl text-xs text-amber-900 flex items-start gap-2">
                  <Lock className="w-4 h-4 text-amber-700 shrink-0 mt-0.5" />
                  <p leading-relaxed>
                    {isAr
                      ? 'التقييم مجهول الهوية 100%. لا نطلب اسمك أو بريدك الإلكتروني. يرجى تقديم ملاحظات موضوعية وبناءة تفيد زملاءك الطلبة.'
                      : 'Soumission 100% anonyme. Aucun nom ou email n’est collecté. Vos retours constructifs aident la communauté étudiante.'}
                  </p>
                </div>

                {/* Institution Picker */}
                <div>
                  <label className="block text-xs font-semibold text-stone-700 mb-1">
                    المؤسسة الجامعية المعنية *
                  </label>
                  <select
                    value={targetId}
                    onChange={(e) => setTargetId(e.target.value)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg text-stone-900 text-xs sm:text-sm focus:outline-none focus:ring-1 focus:ring-[#143D30]"
                  >
                    <optgroup label="المدارس والمعاهد">
                      {institutions.map((i) => (
                        <option key={i.id} value={i.id}>
                          {i.code} — {i.name_ar} ({i.city_ar})
                        </option>
                      ))}
                    </optgroup>
                    <optgroup label="الجامعات المركزية">
                      {universities.map((u) => (
                        <option key={u.id} value={u.id}>
                          {u.code} — {u.name_ar}
                        </option>
                      ))}
                    </optgroup>
                  </select>
                </div>

                {/* Service Category */}
                <div>
                  <label className="block text-xs font-semibold text-stone-700 mb-1">
                    الخدمة الجامعية المراد تقييمها *
                  </label>
                  <select
                    value={category}
                    onChange={(e) => setCategory(e.target.value as FeedbackServiceCategory)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg text-stone-900 text-xs sm:text-sm focus:outline-none focus:ring-1 focus:ring-[#143D30]"
                  >
                    <option value="digital_services">المنصات الرقمية وفضاء الطالب (ENT / Moodle)</option>
                    <option value="library">المكتبة وفضاءات العمل والابتكار</option>
                    <option value="administration">الشؤون الطلابية والمصالح الإدارية والشهادات</option>
                    <option value="student_life">الحياة الطلابية والنوادي والمرافق والأنشطة</option>
                  </select>
                </div>

                {/* Rating selection (Interactive stars) */}
                <div>
                  <label className="block text-xs font-semibold text-stone-700 mb-1.5">
                    التقييم الإجمالي *
                  </label>
                  <div className="flex items-center gap-2">
                    {[1, 2, 3, 4, 5].map((star) => (
                      <button
                        type="button"
                        key={star}
                        onClick={() => setRating(star)}
                        className="p-1 hover:scale-110 transition-transform"
                      >
                        <Star
                          className={`w-6 h-6 ${
                            star <= rating
                              ? 'fill-amber-400 text-amber-400'
                              : 'text-stone-300'
                          }`}
                        />
                      </button>
                    ))}
                    <span className="ms-2 font-mono font-bold text-stone-800 text-sm">
                      {rating} / 5
                    </span>
                  </div>
                </div>

                {/* Anonymous Persona */}
                <div>
                  <label className="block text-xs font-semibold text-stone-700 mb-1">
                    الصفة الدراسية (مجهولة - للاسترشاد فقط)
                  </label>
                  <select
                    value={studentLevel}
                    onChange={(e) => setStudentLevel(e.target.value)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg text-stone-900 text-xs sm:text-sm focus:outline-none focus:ring-1 focus:ring-[#143D30]"
                  >
                    <option value="طالب بالسنة الأولى">طالب بالسنة الأولى (1re année)</option>
                    <option value="طالب مهندس (سلك المهندسين)">طالب مهندس (Cycle Ingénieur)</option>
                    <option value="طالب إجازة أساسية أو مهنية">طالب إجازة أساسية أو مهنية (Licence)</option>
                    <option value="طالب ماستر أو دكتوراه">طالب ماستر أو دكتوراه (Master / Doctorat)</option>
                    <option value="خريج حديث">خريج حديث (Lauréat)</option>
                  </select>
                </div>

                {/* Comment Text Area */}
                <div>
                  <label className="block text-xs font-semibold text-stone-700 mb-1">
                    رأيك أو ملاحظتك البنّاءة *
                  </label>
                  <textarea
                    rows={3}
                    required
                    placeholder={
                      isAr
                        ? 'صف تجربتك مع الخدمة (سرعة الاستجابة، سهولة الاستخدام، نقاط القوة أو التحسين المطلوب)...'
                        : 'Décrivez votre expérience avec ce service...'
                    }
                    value={comment}
                    onChange={(e) => setComment(e.target.value)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg text-stone-900 text-xs sm:text-sm focus:outline-none focus:ring-1 focus:ring-[#143D30] resize-none"
                  />
                </div>

                {/* Policy Compliance Checkbox */}
                <div className="pt-1">
                  <label className="flex items-start gap-2.5 cursor-pointer text-xs text-stone-600">
                    <input
                      type="checkbox"
                      checked={policyAccepted}
                      onChange={(e) => setPolicyAccepted(e.target.checked)}
                      className="mt-0.5 rounded text-[#143D30] focus:ring-[#143D30]"
                    />
                    <span>
                      {isAr
                        ? 'أتعهد بعدم إدراج أي أسماء شخصية أو بيانات تشهيرية، وأعلم أن المنشور يخضع لمراجعة مسبقة من فريق الحكامة.'
                        : 'Je m’engage à ne divulguer aucune donnée personnelle et je reconnais que mon avis est soumis à modération préalable.'}
                    </span>
                  </label>
                </div>

                {errorMsg && (
                  <div className="flex items-center gap-2 p-2.5 bg-red-50 border border-red-200 text-red-700 rounded-lg text-xs">
                    <AlertCircle className="w-4 h-4 shrink-0" />
                    <span>{errorMsg}</span>
                  </div>
                )}

                {/* Actions */}
                <div className="flex items-center justify-end gap-3 pt-3 border-t border-stone-200">
                  <button
                    type="button"
                    onClick={handleCloseModal}
                    className="px-4 py-2 text-xs font-medium text-stone-600 hover:text-stone-900"
                  >
                    {t.close}
                  </button>
                  <button
                    type="submit"
                    className="inline-flex items-center gap-2 px-5 py-2.5 bg-[#143D30] hover:bg-[#0f2e24] text-white text-xs sm:text-sm font-semibold rounded-lg transition-colors shadow-xs"
                  >
                    <Send className="w-4 h-4" />
                    <span>{isAr ? 'إرسال التقييم للتدقيق' : 'Soumettre à la modération'}</span>
                  </button>
                </div>
              </form>
            )}
          </div>
        </ModalBackdrop>
      )}
    </section>
  );
};
