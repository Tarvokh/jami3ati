/**
 * The one place that talks HTTP to the Jami3ati API.
 *
 * Base URL resolution:
 *   1. `VITE_API_BASE_URL` (absolute URL) when the API is hosted separately, else
 *   2. `/api/v1`, which the Vite dev/preview server proxies to the local FastAPI
 *      process (see `vite.config.ts`). The proxy is what makes the UI work from
 *      a phone on the same Wi-Fi: the phone calls Vite, Vite calls the API.
 *
 * Nothing here fabricates a result. A failed call throws `ApiError`; callers
 * must render an error state instead of pretending the write happened.
 */

export const API_BASE_URL: string = (
  import.meta.env.VITE_API_BASE_URL || '/api/v1'
).replace(/\/+$/, '');

/** A non-2xx answer, or a transport failure (status `0`). */
export class ApiError extends Error {
  readonly status: number;
  readonly isNetworkError: boolean;

  constructor(message: string, status = 0, isNetworkError = false) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.isNetworkError = isNetworkError;
  }
}

const REQUEST_TIMEOUT_MS = 12_000;

async function readErrorDetail(response: Response): Promise<string> {
  try {
    const body: unknown = await response.json();
    if (typeof body === 'object' && body !== null && 'detail' in body) {
      const detail = (body as { detail: unknown }).detail;
      if (typeof detail === 'string' && detail.trim()) {
        return detail;
      }
      if (Array.isArray(detail)) {
        const parts = detail.map((item) => {
          if (typeof item === 'object' && item !== null && 'msg' in item) {
            return String((item as { msg: unknown }).msg);
          }
          return String(item);
        });
        if (parts.length > 0) {
          return parts.join(' · ');
        }
      }
    }
  } catch {
    // The body was not JSON; fall through to the status line.
  }
  return `HTTP ${response.status}`;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);

  let response: Response;
  try {
    response = await fetch(`${API_BASE_URL}${path}`, {
      ...init,
      signal: controller.signal,
      headers: {
        Accept: 'application/json',
        ...(init.body ? { 'Content-Type': 'application/json' } : {}),
        ...(init.headers ?? {}),
      },
    });
  } catch {
    // A browser hides the reason (CORS, DNS, refused connection); the UI says
    // "unreachable" rather than guessing which one it was.
    throw new ApiError('network unreachable', 0, true);
  } finally {
    clearTimeout(timer);
  }

  if (!response.ok) {
    throw new ApiError(await readErrorDetail(response), response.status);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  return (await response.json()) as T;
}

// --- Wire shapes (mirror the Pydantic models; no invented fields) ---

export interface SubscriptionRecord {
  id: string;
  email: string;
  institution_id: string | null;
  status: string;
  created_at: string;
}

export interface NotificationRecord {
  id: string;
  institution_id: string | null;
  kind: string;
  title_ar: string;
  body_ar: string;
  source_url: string;
  verified_at: string | null;
  source_notification_id: string | null;
  created_at: string;
}

export interface ReportRecord {
  id: string;
  target_id: string;
  target_name: string;
  field_name: string;
  current_value: string | null;
  proposed_correction: string;
  source_proof_url: string;
  user_notes: string | null;
  reporter_email: string | null;
  status: string;
  created_at: string;
}

export interface ReportInput {
  target_id: string;
  target_name: string;
  field_name: string;
  current_value: string | null;
  proposed_correction: string;
  source_proof_url: string;
  user_notes: string | null;
  reporter_email: string | null;
}

export interface InstitutionRecord {
  id: string;
  code: string;
  name_ar: string;
  name_fr: string;
  city_ar: string;
  city_fr: string;
  data_status: string;
}

// --- Calls ---

/** `201` for a new address, `200` when the address already exists. */
export function subscribe(input: {
  email: string;
  institution_id?: string | null;
}): Promise<SubscriptionRecord> {
  return request<SubscriptionRecord>('/subscriptions', {
    method: 'POST',
    body: JSON.stringify({
      email: input.email,
      institution_id: input.institution_id ?? null,
    }),
  });
}

/** The public archive. No token: reading a published fact needs no account. */
export function listNotifications(limit = 50): Promise<NotificationRecord[]> {
  return request<NotificationRecord[]>(`/notifications?limit=${limit}`);
}

export function listInstitutions(): Promise<InstitutionRecord[]> {
  return request<InstitutionRecord[]>('/institutions');
}

export function submitReport(input: ReportInput): Promise<ReportRecord> {
  return request<ReportRecord>('/reports', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}
