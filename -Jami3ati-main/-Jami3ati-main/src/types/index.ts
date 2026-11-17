export type DataStatus = 'verified' | 'needs_review' | 'demo';

export type InstitutionType = 'engineering_school' | 'faculty' | 'institute' | 'business_school' | 'general_university';

export interface University {
  id: string;
  code: string;
  name_ar: string;
  name_fr: string;
  acronym: string;
  city_ar: string;
  city_fr: string;
  region_ar: string;
  region_fr: string;
  established_year: number;
  website_url: string;
  portal_student_url: string | null;
  description_ar: string;
  description_fr: string;
  image_url: string;
  phone: string | null;
  email: string | null;
  address_ar: string;
  address_fr: string;
  source_url: string;
  source_name: string;
  verified_at: string;
  data_status: DataStatus;
  institution_count: number;
  institutions_list?: string[];
  disciplines?: DisciplineKey[];
}

export type DisciplineKey =
  | 'engineering'
  | 'medicine'
  | 'economics'
  | 'science_tech'
  | 'law'
  | 'humanities';

export interface DisciplineOption {
  key: DisciplineKey;
  label_ar: string;
  label_fr: string;
  iconName: string;
  description_ar: string;
  description_fr: string;
}

export interface Institution {
  id: string;
  code: string;
  university_id: string;
  university_name_ar: string;
  university_name_fr: string;
  name_ar: string;
  name_fr: string;
  type: InstitutionType;
  type_label_ar: string;
  type_label_fr: string;
  city_ar: string;
  city_fr: string;
  campus_ar: string;
  campus_fr: string;
  website_url: string;
  phone: string | null;
  email: string | null;
  source_url: string;
  source_name: string;
  verified_at: string;
  data_status: DataStatus;
  disciplines?: DisciplineKey[];
  disciplines_labels_ar?: string[];
  disciplines_labels_fr?: string[];
  programs_summary_ar: string[];
  programs_summary_fr: string[];
  access_mode_ar: string;
  access_mode_fr: string;
  image_url: string;
}

export interface OfficialAnnouncement {
  id: string;
  title_ar: string;
  title_fr: string;
  institution_code: string;
  institution_name_ar: string;
  institution_name_fr: string;
  event_date: string;
  deadline_date?: string;
  source_url: string;
  source_name: string;
  verified_at: string;
  data_status: 'verified';
  category: 'concours' | 'inscription' | 'bourse' | 'soutenance';
}

export interface InaccuracyReport {
  id: string;
  target_id: string;
  target_name: string;
  field_name: string;
  current_value: string;
  proposed_correction: string;
  source_proof_url: string;
  user_notes?: string;
  reporter_email?: string;
  created_at: string;
  status: 'pending' | 'reviewed' | 'approved' | 'rejected';
}

export type FeedbackServiceCategory = 'digital_services' | 'library' | 'administration' | 'student_life';

export interface StudentFeedback {
  id: string;
  institution_id: string;
  institution_code: string;
  institution_name_ar: string;
  institution_name_fr: string;
  service_category: FeedbackServiceCategory;
  service_label_ar: string;
  service_label_fr: string;
  rating: number; // 1 to 5
  comment_ar: string;
  comment_fr: string;
  student_level_ar: string; // e.g. "طالب مهندس (سنة 2)", "طالب ماستر", "خريج حديث"
  student_level_fr: string;
  created_at: string;
  status: 'approved' | 'pending' | 'rejected';
  moderated_at?: string;
}

export type Language = 'ar' | 'fr';
