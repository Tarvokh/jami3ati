import React, { useState } from 'react';
import { University, Institution, InaccuracyReport, Language, DataStatus } from '../types';
import { translations } from '../data/translations';
import { ShieldCheck, Plus, Download, AlertCircle, Check, X, FileSpreadsheet, Eye } from 'lucide-react';
import { ModalBackdrop } from './ModalBackdrop';
import { useEscapeKey } from '../hooks/useEscapeKey';

interface Props {
  isOpen: boolean;
  onClose: () => void;
  universities: University[];
  institutions: Institution[];
  reports: InaccuracyReport[];
  lang: Language;
  onAddInstitution: (inst: Institution) => void;
  onUpdateReportStatus: (reportId: string, newStatus: InaccuracyReport['status']) => void;
}

export const AdminPanel: React.FC<Props> = ({
  isOpen,
  onClose,
  universities,
  institutions,
  reports,
  lang,
  onAddInstitution,
  onUpdateReportStatus,
}) => {
  useEscapeKey(onClose, isOpen);

  const t = translations[lang];
  const isAr = lang === 'ar';

  const [activeTab, setActiveTab] = useState<'reports' | 'add_institution' | 'export'>('reports');

  // Form state for adding an institution
  const [code, setCode] = useState('');
  const [nameAr, setNameAr] = useState('');
  const [nameFr, setNameFr] = useState('');
  const [parentUnivId, setParentUnivId] = useState(universities[0]?.id || '');
  const [cityAr, setCityAr] = useState('الرباط');
  const [cityFr, setCityFr] = useState('Rabat');
  const [campusAr, setCampusAr] = useState('');
  const [campusFr, setCampusFr] = useState('');
  const [websiteUrl, setWebsiteUrl] = useState('');
  const [phone, setPhone] = useState('');
  const [sourceName, setSourceName] = useState('البوابة الرسمية للمؤسسة');
  const [sourceUrl, setSourceUrl] = useState('');
  const [dataStatus, setDataStatus] = useState<DataStatus>('verified');
  const [successNotice, setSuccessNotice] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleAddSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const parentUniv = universities.find((u) => u.id === parentUnivId) || universities[0];

    const newInst: Institution = {
      id: `inst-${code.toLowerCase().trim()}-${Date.now()}`,
      code: code.toUpperCase().trim(),
      university_id: parentUniv.id,
      university_name_ar: parentUniv.name_ar,
      university_name_fr: parentUniv.name_fr,
      name_ar: nameAr,
      name_fr: nameFr,
      type: 'engineering_school',
      type_label_ar: 'مؤسسة جامعية موثقة',
      type_label_fr: 'Établissement universitaire',
      city_ar: cityAr,
      city_fr: cityFr,
      campus_ar: campusAr || 'الحرم الجامعي الرئيسي',
      campus_fr: campusFr || 'Campus Universitaire Principal',
      website_url: websiteUrl,
      phone: phone.trim() ? phone : null,
      email: null,
      source_name: sourceName,
      source_url: sourceUrl,
      verified_at: new Date().toISOString().split('T')[0],
      data_status: dataStatus,
      programs_summary_ar: ['مسالك التكوين الأساسي والمستمر'],
      programs_summary_fr: ['Formations fondamentales et spécialisées'],
      access_mode_ar: 'مباراة وطنية أو انتقاء رسمي منشور',
      access_mode_fr: 'Concours ou sélection sur dossier officiel',
      image_url: '/src/assets/images/hero_academic_campus_1793985375880.jpg',
    };

    onAddInstitution(newInst);
    setSuccessNotice(
      isAr
        ? `تم إدراج المؤسسة (${newInst.code}) وتوثيق المصدر بنجاح في قاعدة البيانات المحلية!`
        : `L'établissement (${newInst.code}) a été ajouté avec sa source validée !`
    );

    // reset form
    setCode('');
    setNameAr('');
    setNameFr('');
    setWebsiteUrl('');
    setSourceUrl('');
    setPhone('');
    setTimeout(() => setSuccessNotice(null), 4000);
  };

  const handleExportJSON = () => {
    const dataset = {
      meta: {
        platform: 'Jami3ati - Moroccan Higher Education Open Data Guide',
        version: '1.0.0-hackathon-mvp',
        exported_at: new Date().toISOString(),
        sources_origin: 'data.gov.ma & official published domains (.ac.ma)',
      },
      universities,
      institutions,
      reports_count: reports.length,
    };

    const blob = new Blob([JSON.stringify(dataset, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `jami3ati_morocco_universities_${new Date().toISOString().split('T')[0]}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <ModalBackdrop onClose={onClose} ariaLabelledBy="admin-modal-title">
      <div className="relative w-full max-w-4xl max-h-[90vh] my-auto flex flex-col bg-white rounded-2xl shadow-2xl border border-stone-200 overflow-hidden select-text animate-in zoom-in-95 duration-150">
        {/* Modal Header */}
        <div className="px-6 py-4 bg-[#FAF7F0] border-b border-stone-200 flex items-center justify-between shrink-0">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-xl bg-[#143D30] text-white flex items-center justify-center font-bold shrink-0">
              <ShieldCheck className="w-5 h-5 text-emerald-300" />
            </div>
            <div>
              <h2 id="admin-modal-title" className="text-base sm:text-lg font-bold text-stone-900 leading-tight">
                {t.admin_title}
              </h2>
              <p className="text-xs text-stone-500">
                مساحة العمل الإدارية المستقلة (Demo Admin Portal)
              </p>
            </div>
          </div>

          <div className="flex items-center gap-3">
            {/* Desktop Tabs */}
            <div className="hidden sm:flex items-center gap-1 bg-stone-200/80 p-1 rounded-xl text-xs font-medium">
              <button
                onClick={() => setActiveTab('reports')}
                className={`px-3 py-1.5 rounded-lg transition-colors ${
                  activeTab === 'reports'
                    ? 'bg-[#143D30] text-white shadow-xs font-bold'
                    : 'text-stone-700 hover:text-stone-900'
                }`}
              >
                البلاغات ({reports.length})
              </button>
              <button
                onClick={() => setActiveTab('add_institution')}
                className={`px-3 py-1.5 rounded-lg transition-colors ${
                  activeTab === 'add_institution'
                    ? 'bg-[#143D30] text-white shadow-xs font-bold'
                    : 'text-stone-700 hover:text-stone-900'
                }`}
              >
                + إضافة مؤسسة
              </button>
              <button
                onClick={() => setActiveTab('export')}
                className={`px-3 py-1.5 rounded-lg transition-colors ${
                  activeTab === 'export'
                    ? 'bg-[#143D30] text-white shadow-xs font-bold'
                    : 'text-stone-700 hover:text-stone-900'
                }`}
              >
                تصدير JSON
              </button>
            </div>

            {/* Close Button */}
            <button
              onClick={onClose}
              className="p-1.5 text-stone-500 hover:text-stone-900 hover:bg-stone-200 rounded-lg transition-colors"
              aria-label={t.close}
            >
              <X className="w-5 h-5" />
            </button>
          </div>
        </div>

        {/* Mobile Tab Switcher */}
        <div className="sm:hidden flex border-b border-stone-200 bg-stone-50 p-2 gap-1 text-xs font-medium">
          <button
            onClick={() => setActiveTab('reports')}
            className={`flex-1 py-1.5 rounded-lg text-center ${
              activeTab === 'reports' ? 'bg-[#143D30] text-white font-bold' : 'bg-stone-200 text-stone-700'
            }`}
          >
            البلاغات ({reports.length})
          </button>
          <button
            onClick={() => setActiveTab('add_institution')}
            className={`flex-1 py-1.5 rounded-lg text-center ${
              activeTab === 'add_institution' ? 'bg-[#143D30] text-white font-bold' : 'bg-stone-200 text-stone-700'
            }`}
          >
            + إضافة
          </button>
          <button
            onClick={() => setActiveTab('export')}
            className={`flex-1 py-1.5 rounded-lg text-center ${
              activeTab === 'export' ? 'bg-[#143D30] text-white font-bold' : 'bg-stone-200 text-stone-700'
            }`}
          >
            تصدير
          </button>
        </div>

        {/* Modal Scrollable Body */}
        <div className="p-6 overflow-y-auto space-y-6 flex-1 overscroll-contain">
          {/* Tab 1: Reports Moderation */}
          {activeTab === 'reports' && (
            <div className="bg-[#FAF8F5] rounded-xl border border-stone-200 overflow-hidden">
              <div className="p-4 bg-white border-b border-stone-200 flex items-center justify-between">
                <h3 className="font-bold text-sm text-stone-900">
                  بلاغات التدقيق والتصحيح الواردة من المستخدمين
                </h3>
                <span className="text-xs text-stone-500 font-mono">
                  {reports.filter((r) => r.status === 'pending').length} بلاغات قيد الدراسة
                </span>
              </div>

              {reports.length === 0 ? (
                <div className="p-12 text-center text-stone-500 text-sm">
                  لا توجد بلاغات حاليًا. كل المعلومات متطابقة مع مصادرها.
                </div>
              ) : (
                <div className="divide-y divide-stone-200">
                  {reports.map((report) => (
                    <div key={report.id} className="p-5 flex flex-col md:flex-row md:items-center justify-between gap-4">
                      <div className="space-y-1.5 max-w-2xl">
                        <div className="flex items-center gap-2">
                          <span className="font-mono text-xs px-2 py-0.5 bg-stone-200 text-stone-800 rounded font-bold">
                            {report.id}
                          </span>
                          <span className="font-bold text-sm text-stone-900">
                            {report.target_name}
                          </span>
                          <span
                            className={`text-xs px-2 py-0.5 rounded font-medium ${
                              report.status === 'pending'
                                ? 'bg-amber-100 text-amber-800'
                                : report.status === 'approved'
                                ? 'bg-emerald-100 text-emerald-800'
                                : 'bg-stone-200 text-stone-700'
                            }`}
                          >
                            {report.status === 'pending' ? 'قيد المراجعة' : report.status === 'approved' ? 'معتمد' : 'مرفوض'}
                          </span>
                        </div>

                        <div className="text-xs text-stone-700">
                          <span className="text-stone-500 font-medium">الحقل: </span>
                          <span>{report.field_name}</span>
                          <span className="mx-2">←</span>
                          <span className="text-stone-500 font-medium">التصحيح المقترح: </span>
                          <span className="font-medium text-emerald-800">{report.proposed_correction}</span>
                        </div>

                        <div className="text-xs text-stone-600">
                          <span className="text-stone-500 font-medium">مصدر الإثبات: </span>
                          <a
                            href={report.source_proof_url}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="text-[#143D30] hover:underline font-mono"
                          >
                            {report.source_proof_url}
                          </a>
                        </div>
                        {report.user_notes && (
                          <p className="text-xs text-stone-500 italic">« {report.user_notes} »</p>
                        )}
                      </div>

                      <div className="flex items-center gap-2 shrink-0">
                        {report.status === 'pending' && (
                          <>
                            <button
                              onClick={() => onUpdateReportStatus(report.id, 'approved')}
                              className="inline-flex items-center gap-1 px-3 py-1.5 rounded bg-emerald-700 hover:bg-emerald-800 text-white text-xs font-medium"
                            >
                              <Check className="w-3.5 h-3.5" />
                              <span>قبول وتحديث</span>
                            </button>
                            <button
                              onClick={() => onUpdateReportStatus(report.id, 'rejected')}
                              className="inline-flex items-center gap-1 px-3 py-1.5 rounded bg-stone-200 hover:bg-stone-300 text-stone-700 text-xs font-medium"
                            >
                              <X className="w-3.5 h-3.5" />
                              <span>رفض</span>
                            </button>
                          </>
                        )}
                        {report.status !== 'pending' && (
                          <span className="text-xs text-stone-400">تم البت في البلاغ</span>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {/* Tab 2: Add Institution Form */}
          {activeTab === 'add_institution' && (
            <div className="bg-[#FAF8F5] rounded-xl border border-stone-200 p-6">
              <h3 className="font-bold text-base text-stone-900 mb-4 flex items-center gap-2">
                <Plus className="w-4 h-4 text-[#143D30]" />
                <span>إضافة مؤسسة جامعية جديدة مع توثيق المصدر الإلزامي</span>
              </h3>

              {successNotice && (
                <div className="mb-4 p-3 bg-emerald-50 border border-emerald-300 text-emerald-800 rounded-lg text-xs font-medium">
                  {successNotice}
                </div>
              )}

              <form onSubmit={handleAddSubmit} className="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs sm:text-sm">
                <div>
                  <label className="block text-xs font-medium text-stone-700 mb-1">
                    الاسم بالعربية *
                  </label>
                  <input
                    type="text"
                    required
                    placeholder="مثال: المدرسة العليا للتكنولوجيا بسلا"
                    value={nameAr}
                    onChange={(e) => setNameAr(e.target.value)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg"
                  />
                </div>

                <div>
                  <label className="block text-xs font-medium text-stone-700 mb-1">
                    الاسم بالفرنسية *
                  </label>
                  <input
                    type="text"
                    required
                    placeholder="Ex: École Supérieure de Technologie de Salé"
                    value={nameFr}
                    onChange={(e) => setNameFr(e.target.value)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg"
                  />
                </div>

                <div>
                  <label className="block text-xs font-medium text-stone-700 mb-1">
                    رمز المؤسسة (Acronyme) *
                  </label>
                  <input
                    type="text"
                    required
                    placeholder="مثال: EST-SALE"
                    value={code}
                    onChange={(e) => setCode(e.target.value)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg font-mono"
                  />
                </div>

                <div>
                  <label className="block text-xs font-medium text-stone-700 mb-1">
                    الجامعة التابعة لها *
                  </label>
                  <select
                    value={parentUnivId}
                    onChange={(e) => setParentUnivId(e.target.value)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg"
                  >
                    {universities.map((u) => (
                      <option key={u.id} value={u.id}>
                        {u.name_ar} ({u.code})
                      </option>
                    ))}
                  </select>
                </div>

                <div>
                  <label className="block text-xs font-medium text-stone-700 mb-1">
                    المدينة *
                  </label>
                  <input
                    type="text"
                    required
                    value={cityAr}
                    onChange={(e) => {
                      setCityAr(e.target.value);
                      setCityFr(e.target.value);
                    }}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg"
                  />
                </div>

                <div>
                  <label className="block text-xs font-medium text-stone-700 mb-1">
                    رابط الموقع الرسمي *
                  </label>
                  <input
                    type="url"
                    required
                    placeholder="https://..."
                    value={websiteUrl}
                    onChange={(e) => setWebsiteUrl(e.target.value)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg font-mono text-xs"
                  />
                </div>

                <div>
                  <label className="block text-xs font-medium text-stone-700 mb-1">
                    اسم المصدر المعتمد *
                  </label>
                  <input
                    type="text"
                    required
                    value={sourceName}
                    onChange={(e) => setSourceName(e.target.value)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg text-xs"
                  />
                </div>

                <div>
                  <label className="block text-xs font-medium text-stone-700 mb-1">
                    رابط المصدر أو المنشور الرسمي *
                  </label>
                  <input
                    type="url"
                    required
                    placeholder="https://data.gov.ma/..."
                    value={sourceUrl}
                    onChange={(e) => setSourceUrl(e.target.value)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg font-mono text-xs"
                  />
                </div>

                <div>
                  <label className="block text-xs font-medium text-stone-700 mb-1">
                    حالة توثيق البيانات *
                  </label>
                  <select
                    value={dataStatus}
                    onChange={(e) => setDataStatus(e.target.value as DataStatus)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg"
                  >
                    <option value="verified">موثق بمصدر رسمي (Verified)</option>
                    <option value="needs_review">قيد التدقيق (Needs Review)</option>
                    <option value="demo">بيانات تجريبية (Demo Seed)</option>
                  </select>
                </div>

                <div>
                  <label className="block text-xs font-medium text-stone-700 mb-1">
                    الهاتف الرسمي (اتركه فارغاً إن لم يكن موثقاً)
                  </label>
                  <input
                    type="text"
                    placeholder="+212 ..."
                    value={phone}
                    onChange={(e) => setPhone(e.target.value)}
                    className="w-full px-3 py-2 bg-white border border-stone-300 rounded-lg font-mono"
                  />
                </div>

                <div className="md:col-span-2 pt-2 flex justify-end">
                  <button
                    type="submit"
                    className="px-6 py-2.5 bg-[#143D30] hover:bg-[#0f2e24] text-white text-xs sm:text-sm font-medium rounded-lg transition-colors shadow-xs"
                  >
                    حفظ وتوثيق المؤسسة
                  </button>
                </div>
              </form>
            </div>
          )}

          {/* Tab 3: Open Data Export */}
          {activeTab === 'export' && (
            <div className="bg-[#FAF8F5] rounded-xl border border-stone-200 p-8 text-center space-y-4">
              <div className="w-12 h-12 bg-[#143D30]/10 text-[#143D30] rounded-full flex items-center justify-center mx-auto">
                <FileSpreadsheet className="w-6 h-6" />
              </div>
              <h3 className="text-lg font-bold text-stone-900">
                تصدير مجموعة البيانات المفتوحة (Open Data Export)
              </h3>
              <p className="text-xs sm:text-sm text-stone-600 max-w-lg mx-auto">
                احصل على ملف JSON كامل يتضمن جميع الجامعات والمؤسسات المدرجة مع بيانات التوثيق وتواريخ التدقيق ورابط المصدر الرسمي لكل حقل.
              </p>
              <div className="pt-2">
                <button
                  onClick={handleExportJSON}
                  className="inline-flex items-center gap-2 px-6 py-3 bg-[#143D30] hover:bg-[#0f2e24] text-white text-sm font-medium rounded-lg transition-colors shadow-xs"
                >
                  <Download className="w-4 h-4" />
                  <span>تحميل ملف البيانات (JSON)</span>
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </ModalBackdrop>
  );
};
