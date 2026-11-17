# جامعتي — الخطة العميقة (Jami3ati Deep Plan)

> **الغاية من هذا الملف:** أن يكون المرجع الوحيد لبناء «جامعتي» فوق Tira Mail.
> كل قرار هنا مُبرَّر، وكل رقم مقاس، وكل ما لم يُبنَ بعد مكتوب صراحةً أنه لم يُبنَ.
>
> **التاريخ:** 17 نوفمبر 2026
> **آخر أجل للترشّح:** الجمعة 20 نوفمبر 2026 — 16:30
> **العرض:** 21–22 ديسمبر 2026، ENSIAS، الرباط
> **الإصدار:** 1.0

---

## 0. الخلاصة التنفيذية

جامعتي منتج واحد بحلقتين، فوق بنية تحتية واحدة:

| | الحلقة 1 — البلاغ | الحلقة 2 — التنبيه |
|---|---|---|
| الاتجاه | طالب → منسّق | مسؤول/مصدر → طلاب |
| النمط | reactive | proactive |
| المُشغِّل | `POST /reports` | `poller` يكتشف تغييرًا |
| المُخرَج | بريد للمنسّق | بريد للمشتركين |

**البنية التحتية المشتركة — وهذا هو جوهر الخطة:**

1. **طابور واحد** (`outbound_queue` في Tira) بسياسة إعادة محاولة واحدة و`dead letter` واحدة.
2. **مسار بريد واحد** (`mail_relay`) — لا يوجد «بريد جامعتي» و«بريد Tira»، بل بريد واحد موقّع.
3. **`subscriptions` + double opt-in** في Jami3ati (لأن Jami3ati يملك بيانات الأشخاص، لا Tira).
4. **`audit_mirror`**: كل قرار (قبول إرسال، تسليم، فشل، إلغاء اشتراك) له أثر.

**الفصل الحاسم:**

> Jami3ati يقرّر **ماذا** يُقال ولمن. Tira تنفّذ **كيف** يصل، وتضمن أنه وصل أو تُبلّغ بفشله.

لا يقرأ Tira بيانات Jami3ati، ولا يكتب Jami3ati في قاعدة Tira. الاتصال عبر HTTP بعقد واحد.

> **حالة 17 نوفمبر 2026:** العقد مبنيّ على الطرفين. Tira: `POST /api/v1/outbound` يعيد `202`.
> Jami3ati: `subscriptions` + double opt-in + `notification_deliveries` + worker —
> **51 اختبارًا أخضر** وبرهان حيّ عبر لغتين (`CORRECTIONS.md` §9.42).
> الحلقة 2 (poller + `/n/{id}`) **لم تُبنَ بعد**.

---

## 1. المبادئ المعمارية (Non-negotiables)

### 1.1 ضمانتان منفصلتان — لا تُخلطان أبدًا

| الضمانة | ما تثبته | ما لا تثبته | أين تُقاس |
|---|---|---|---|
| DKIM / SPF / DMARC | البريد صادر من نطاقنا | أن المعلومة صحيحة | ترويسات النسخة المستلمة |
| `source_url` + `verified_at` + `notification_id` | المعلومة مأخوذة من مصدر رسمي في تاريخ محدّد | أن البريد لم يُزوَّر | صفحة `/n/{id}` + صف في قاعدة البيانات |

**صياغة العرض المسموحة:** «هذه المعلومة من هذا الرابط في هذا التاريخ، وهذا البريد من نطاقنا».
**صياغة ممنوعة:** «موقّع من الوزارة»، «بريد رسمي معتمد».

### 1.2 القبول ليس التسليم

`POST /api/v1/outbound` يعيد **`202 Accepted`**، لا `200 OK`. القبول يعني: «حُفظ الطلب بشكل دائم، وسيُعاد المحاولة حتى ينجح أو يُسجَّل فشلًا». أي رد آخر (خصوصًا `200`) سيكون كذبًا بنيويًا.

### 1.3 At-least-once + Idempotency

الشبكة قد تُسقط الرد بعد نجاح الطلب. لذلك:

- كل طلب يحمل `idempotency_key` إلزاميًا.
- `UNIQUE (tenant_id, idempotency_key)` في قاعدة Tira.
- إعادة الطلب بنفس المفتاح تُعيد **نفس** `id` مع `"duplicate": true`، ولا تُنشئ صفًا ثانيًا.

النتيجة: لا بريد مزدوج، ولا بريد ضائع.

### 1.4 لا ترويسة عشوائية

لا نقبل `headers: {...}` من المتصل. نقبل **أربعة حقول فقط**، كل واحد يُتحقَّق منه في Rust قبل الوصول إلى قاعدة البيانات:

| الحقل | الترويسة الناتجة | التحقق |
|---|---|---|
| `correlation_id` | `X-Jami3ati-Notification` | `[A-Za-z0-9._:-]{1,128}` |
| `list_unsubscribe` | `List-Unsubscribe` + `List-Unsubscribe-Post` | يبدأ بـ `<https:` أو `<mailto:` وينتهي بـ `>`، ASCII مطبوع فقط، ≤ 512 بايت |
| `reply_to` | `Reply-To` | عنوان بريد صالح (يُحلَّل كـ `Mailbox`) |
| `idempotency_key` | لا ترويسة (عمود فقط) | `[A-Za-z0-9._:-]{8,128}` |

**السبب:** حقن ترويسات (`CRLF`، `Bcc:`، `From:` مزيّف) هو ثغرة بريد كلاسيكية. القائمة المغلقة تُلغيها بنيويًا.

### 1.5 الفشل يُعلَن، لا يُخفى

| الحالة | الرد | لماذا |
|---|---|---|
| لا يوجد `mail_relay` مهيّأ | `503` | لا نقبل بريدًا لن نستطيع تسليمه |
| لا توجد قاعدة بيانات | `503` | لا مكان للحفظ |
| تجاوز الحد | `429` + `Retry-After` | يُعلِم المتصل بدل الإسقاط الصامت |
| مفتاح مكرر | `202` + `duplicate: true` | إعادة المحاولة ليست خطأ |

---

## 2. المعمارية

```
┌──────────────────────────── Jami3ati (Python / FastAPI) ────────────────────────────┐
│                                                                                     │
│  POST /reports ──► reports ──► notifications ──► notification_deliveries (outbox)    │
│  poller ────────► facts  ────► notifications ──┘         │                          │
│  POST /subscriptions ──► subscriptions (pending)          │                          │
│  GET  /confirm ────────► subscriptions (active)           │                          │
│  GET/POST /u/{token} ──► subscriptions (unsubscribed)     │                          │
│                                                            │  worker (thread)        │
│                                                            ▼                        │
│                                     POST /api/v1/outbound  (Bearer token)            │
└────────────────────────────────────────────────────────────┼────────────────────────┘
                                                             │ 202 { id, duplicate }
                                                             ▼
┌──────────────────────────── Tira Mail (Rust / Axum) ────────────────────────────────┐
│  Auth (Bearer) ──► validate ──► INSERT outbound_queue (pending, idempotent)         │
│                                        │                                            │
│                                        ▼  every 2s, FOR UPDATE SKIP LOCKED           │
│                                  outbound_worker                                    │
│                                        │                                            │
│                    ┌───────────────────┼───────────────────┐                        │
│                    ▼                   ▼                   ▼                        │
│                 delivered          retry (backoff)     dead letter + bounce          │
│                    │                   │                   │                        │
│                    └───────────────────┴───────────────────┘                        │
│                                        ▼                                            │
│                              audit_log (mail.send / mail.bounce)                    │
└────────────────────────────────────────┼────────────────────────────────────────────┘
                                         ▼
                                  SMTP relay (STARTTLS)
                                         ▼
                                  صندوق الطالب / المنسّق
```

### 2.1 لماذا لا نشترك في قاعدة بيانات واحدة؟

| البديل | المشكلة |
|---|---|
| Jami3ati يكتب مباشرة في `outbound_queue` | اقتران مخططين (schema coupling)؛ أي تغيير في Tira يكسر Python، وبالعكس |
| Tira يقرأ جداول Jami3ati | Tira يصبح مضطرًا لمعرفة `institutions` و`facts` — عكس الفصل المطلوب |
| HTTP بعقد مُصدَّر | ✅ الحد الوحيد المُوثَّق؛ يمكن استبدال أي طرف دون الآخر |

### 2.2 لماذا لا نستخدم SMTP مباشرة من Jami3ati؟

لأن ذلك سيتجاوز: الطابور الدائم، وإعادة المحاولة، و`dead letter`، وسجل `bounces`، وسجل التدقيق. أي إرسال مباشر = إعادة بناء Tira داخل Python بشكل أسوأ.

---

## 3. نموذج البيانات

### 3.1 Tira — المهاجرة `0010_outbound_api.sql` (مُنفَّذة في هذه الجلسة)

```sql
ALTER TABLE outbound_queue
    ADD COLUMN idempotency_key  TEXT,
    ADD COLUMN correlation_id   TEXT,
    ADD COLUMN list_unsubscribe TEXT,
    ADD COLUMN reply_to         TEXT;

CREATE UNIQUE INDEX outbound_queue_idempotency_key_idx
    ON outbound_queue (tenant_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
```

**لماذا أعمدة صريحة بدل `JSONB headers`؟** لأن `JSONB` يفتح باب حقن الترويسات؛ الأعمدة الصريحة تجعل مجموعة الترويسات الممكنة **مغلقة وقت الترجمة**.

**التوافق الخلفي:** صفوف الواجهة (`/mail/{id}/send`) تبقى `NULL` في الأعمدة الأربعة، فلا يتغيّر سلوكها.

### 3.2 Jami3ati — الجداول الجديدة (المرحلة القادمة)

> **تحديث 17 نوفمبر 2026 (مساءً) — بُنِيَت.** هذا الـDDL يبقى سجل **تصميم**.
> ما نُفِّذ فعلًا في Jami3ati يخالف أربعة أسطر منه، والفرق مسجَّل بجدول في
> `docs/CORRECTIONS.md` §9.42. أهمها: `UNIQUE (email)` بدل
> `UNIQUE (email_normalized, institution_code)` (صف واحد لكل عنوان)؛ لا عمود
> `unsubscribe_token_hash` (التوكن مشتق HMAC، لا يُخزَّن)؛ `status` بلا `bounced`
> (لا webhook بعد)؛ و`dead` بدل `queued` في طابور Jami3ati. الأسماء أدناه
> (`payload JSONB`، `recipient`) استُبدلت بأعمدة صريحة
> (`title_ar`/`body_ar`/`recipient_email`). **51 اختبارًا أخضر** يقيس المبني، لا هذا الـDDL.

```sql
-- 1) الاشتراك: صف واحد لكل (بريد، مؤسسة). institution_code = NULL يعني «الكل».
CREATE TABLE subscriptions (
    id                    UUID PRIMARY KEY,
    email                 TEXT NOT NULL,              -- كما أدخله المستخدم
    email_normalized      TEXT NOT NULL,              -- lower(trim(email))
    institution_code      TEXT,                       -- NULL = كل المؤسسات
    status                TEXT NOT NULL CHECK (status IN ('pending','active','unsubscribed','bounced')),
    consent_at            TIMESTAMPTZ,                -- لحظة النقر على رابط التأكيد
    confirm_token_hash    BYTEA NOT NULL,             -- sha256(التوكن)، لا يُخزَّن الأصل
    confirm_expires_at    TIMESTAMPTZ NOT NULL,       -- 48 ساعة
    unsubscribe_token_hash BYTEA NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    confirmed_at          TIMESTAMPTZ,
    unsubscribed_at       TIMESTAMPTZ,
    UNIQUE (email_normalized, institution_code)
);

-- 2) الإشعار: قرار «هناك ما يستحق أن يُقال».
CREATE TABLE notifications (
    id            UUID PRIMARY KEY,
    kind          TEXT NOT NULL CHECK (kind IN ('fact_change','report_received','report_status')),
    fact_id       TEXT,
    report_id     TEXT,
    source_url    TEXT NOT NULL,
    verified_at   TIMESTAMPTZ NOT NULL,
    payload       JSONB NOT NULL,                     -- العنوان + النص + الحقول المتغيّرة
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 3) طابور Jami3ati: صف لكل (إشعار، مستلم). هذا هو transactional outbox.
CREATE TABLE notification_deliveries (
    id              UUID PRIMARY KEY,
    notification_id UUID NOT NULL REFERENCES notifications(id) ON DELETE CASCADE,
    subscription_id UUID REFERENCES subscriptions(id) ON DELETE SET NULL,
    recipient       TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,             -- 'jami3ati:<notification>:<subscription>'
    status          TEXT NOT NULL CHECK (status IN ('queued','accepted','failed','bounced')),
    tira_outbound_id UUID,                            -- معرّف الطابور في Tira
    attempts        INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_error      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX notification_deliveries_due_idx
    ON notification_deliveries (next_attempt_at) WHERE status = 'queued';
```

**لماذا طابوران؟** لأن لكل طرف مسؤوليته: طابور Jami3ati يحفظ **القرار** (حتى لو كان Tira متوقفًا)، وطابور Tira يحفظ **التسليم**. إسقاط أحدهما يجعل النظام يفقد إما القرار أو المحاولة.

**`email_normalized`:** التطبيع قبل التخزين يمنع اشتراكين متطابقين بفروق حالة أحرف. `UNIQUE (email_normalized, institution_code)` يمنع التكرار بنيويًا.

**التوكنات:** لا تُخزَّن أبدًا كنص صريح. `sha256` كافٍ لأن التوكن عشوائي 256-بت (مثل `api_tokens` في Tira)، وليس كلمة مرور بشرية.

---

## 4. عقد الـ API

### 4.1 `POST /api/v1/outbound`

**المصادقة:** `Authorization: Bearer tira_...` (نفس `api_tokens`، ونفس RLS).

**الجسم:**

```json
{
  "mailbox_id": "33333333-3333-4333-8333-333333333333",
  "to": ["etudiant@example.ma"],
  "subject": "تحديث موعد الترشيح — ENSIAS",
  "text": "…نص عادي UTF-8…",
  "idempotency_key": "jami3ati:6f1c...:a91e...",
  "correlation_id": "6f1c2a4e-...",
  "list_unsubscribe": "<https://jami3ati.ma/u/eyJ...>",
  "reply_to": "no-reply@jami3ati.ma"
}
```

| الحقل | إلزامي | القاعدة |
|---|---|---|
| `mailbox_id` | فقط إذا كان التوكن غير مقيَّد بصندوق | UUID تابع لنفس المستأجر |
| `to` | نعم | 1..=50 عنوانًا صالحًا |
| `subject` | نعم | 1..=255 حرفًا، بلا `CR`/`LF` |
| `text` | نعم | 1..=65,536 بايت |
| `idempotency_key` | نعم | 8..=128، `[A-Za-z0-9._:-]` |
| `correlation_id` | لا | 1..=128، نفس المجموعة |
| `list_unsubscribe` | لا | `<https:` أو `<mailto:` … `>`، ASCII مطبوع، ≤ 512 |
| `reply_to` | لا | `Mailbox` صالح |

**الردود:**

| الحالة | الجسم | المعنى |
|---|---|---|
| `202` | `{"id":"…","status":"queued","duplicate":false}` | حُفظ، وسيُسلَّم أو يُسجَّل فشله |
| `202` | `{"id":"…","status":"queued","duplicate":true}` | مفتاح مكرر — نفس الصف |
| `400` | `{"error":"…"}` | حقل غير صالح |
| `401` | `{"error":"missing bearer token"}` | لا توكن |
| `404` | `{"error":"mailbox not found"}` | صندوق غير موجود أو لمستأجر آخر (ردّ واحد للحالتين) |
| `429` | `{"error":"too many sends"}` + `Retry-After: 600` | تجاوز حد التوكن |
| `503` | `{"error":"outbound relay not configured"}` | لا يمكن التسليم |

**حدّ المعدّل:** 120 قبولًا لكل توكن في نافذة 10 دقائق (in-process، مثل بقية الـ limiter). موثَّق كقيد نسخة العرض؛ التوسّع الأفقي يحتاج مخزنًا مشتركًا.

**سؤال تصميمي: لماذا `202` حتى عند `duplicate`؟** لأن نية المتصل تحقّقت في الحالتين. `409 Conflict` سيجبر كل عميل على معالجة «خطأ» ليس خطأً.

### 4.2 ما يحدث داخل Tira عند القبول

1. `Auth` يحلّ التوكن → `(tenant_id, mailbox_id?)`.
2. تحديد الصندوق: توكن مقيَّد → صندوقه؛ توكن مستأجر → `mailbox_id` من الجسم.
3. `get_mailbox` + مقارنة `tenant_id` صراحةً (لأن `get_mailbox` يعمل بدور المالك ويتجاوز RLS).
4. التحقق من كل الحقول (بما فيها بناء الرسالة فعليًا بـ `build_message_with` والتخلّص منها — إن قُبلت الآن، فالـ worker سيبنيها لاحقًا بنفس النجاح).
5. `INSERT … ON CONFLICT (tenant_id, idempotency_key) DO NOTHING RETURNING id`.
6. إن لم يُعِد الصف → `SELECT id` لنفس المفتاح → `duplicate: true`.
7. `audit_log`: `mail.send.accepted` بحالة `ok`.
8. `202`.

### 4.3 كيف يبني الـ worker الرسالة

```
build_message_with(from, to, subject, body, [
  ("X-Jami3ati-Notification", correlation_id),
  ("List-Unsubscribe",        list_unsubscribe),
  ("List-Unsubscribe-Post",   "List-Unsubscribe=One-Click"),
  ("Reply-To",                reply_to),
])
```

أسماء الترويسات **ثوابت نصية** (لا تُبنى من مدخلات)، والقيم تُتحقَّق قبل الوصول إلى الطابور. `HeaderValue::new` في lettre يُرمِّز RFC 2047 ويطوي الأسطر؛ لذلك نرفض أي حرف خارج ASCII المطبوع في القيم.

---

## 5. تسلسل الحلقة 1 — البلاغ

```
طالب                Jami3ati                         Tira                    منسّق
  │  POST /reports     │                               │                        │
  ├───────────────────►│ تحقّق source_proof_url يبدأ http│                        │
  │                    │ INSERT reports (pending)      │                        │
  │                    │ INSERT notifications          │                        │
  │                    │ INSERT notification_deliveries│                        │
  │  201 + rep-id      │ (queued)                      │                        │
  ◄────────────────────┤                               │                        │
                       │ worker: POST /api/v1/outbound │                        │
                       ├──────────────────────────────►│ validate + INSERT queue │
                       │        202 {id}               │                        │
                       ◄──────────────────────────────┤                        │
                       │ status='accepted'             │ worker tick            │
                       │                               ├─────── SMTP ──────────►│
                       │                               │ audit mail.send        │
```

**نقطة الصدق:** إذا فشل البريد، يبقى البلاغ في Jami3ati بحالة `pending`، ويظهر للمنسّق في لوحة التحكم. **البريد ليس شرطًا لوجود البلاغ** — هذه هي قيمة فصل الطابورين.

## 6. تسلسل الحلقة 2 — التنبيه

```
poller              Jami3ati                    Tira                 مشترك
  │ GET source page    │                          │                     │
  ├───────────────────►│ hash(content)            │                     │
  │                    │ تغيّر؟ → INSERT fact +   │                     │
  │                    │ notifications + deliveries│                    │
  │                    │ worker → POST /outbound  │                     │
  │                    ├─────────────────────────►│ 202                 │
  │                    │                          ├──── SMTP ──────────►│
  │                    │                          │  List-Unsubscribe   │
  │                    │                          │  X-Jami3ati-Notif.  │
  │                    │ GET /n/{id}              │                     │
  │                    │◄─────────────────────────┼─────────────────────┤
  │                    │ source_url + verified_at │                     │
```

**قاعدة الـ poller:** معدّل منخفض (كل 6 ساعات افتراضيًا)، احترام `robots.txt`، و`If-Modified-Since` / `ETag` حيث يدعمها المصدر. لا scraping واسع، ولا تجاوز حماية.

**كشف التغيير:** `sha256` على النص المُطبَّع (إزالة المسافات الزائدة ووسوم التخطيط)، لا على HTML الخام — وإلا فكل تغيير في قالب الموقع سيبدو «خبرًا».

---

## 7. الأمان والخصوصية

| الخطر | الضابط |
|---|---|
| توكن مسرَّب | `sha256` فقط في القاعدة؛ إلغاء = تحديث صف؛ انتهاء صلاحية |
| توكن مستأجر يرسل باسم مستأجر آخر | مقارنة `tenant_id` صريحة بعد `get_mailbox` |
| حقن ترويسات | قائمة ترويسات مغلقة + رفض `CR/LF` + ASCII مطبوع |
| إغراق الطابور | 120/10د لكل توكن + `max_attempts = 3` + dead letter |
| بريد مزدوج | `idempotency_key` فريد لكل مستأجر |
| تسريب بريد المشتركين | `List-Unsubscribe` بتوكن HMAC؛ لا كشف لقائمة المشتركين عبر API عام |
| بيانات شخصية | بريد فقط، double opt-in، حذف عند الإلغاء بعد مدة محدّدة، لا SMS |
| إطار الصفحات | `frame-ancestors 'none'` + `X-Frame-Options: DENY` (قائم) |
| XSS في `/n/{id}` | نفس CSP الصارم، بلا inline |

**تصميم توكن إلغاء الاشتراك:** `base64url(subscription_id || HMAC-SHA256(secret, subscription_id))`. التحقق بلا قاعدة بيانات أولًا (توقيع)، ثم قراءة الصف. التوكن لا يُخزَّن في القاعدة، فيبقى صالحًا بعد أي نسخة احتياطية ولا يحتاج جدولًا إضافيًا.

**الاحتفاظ:** `unsubscribed` يبقى 30 يومًا (لمنع إعادة الاشتراك الصامت) ثم يُحذف البريد ويبقى صف مجهول للإحصاء. يُوثَّق في سياسة الخصوصية قبل العرض.

---

## 8. مصفوفة الفشل

| السيناريو | ما يحدث | ما يراه المستخدم | ما يُسجَّل |
|---|---|---|---|
| Tira متوقف | Jami3ati worker يفشل في الاتصال، يعيد الجدولة | لا شيء (البريد ليس واجهة) | `last_error`, `attempts` |
| SMTP relay معطّل | `503` عند القبول | المتصل يرى الخطأ فورًا | لا صف في الطابور |
| relay يفشل أثناء التسليم | retry 30s → 60s → 120s | لا شيء | `mail.send` error + `bounces` |
| استنفاد المحاولات | dead letter + bounce دائم | لا شيء | `mail.bounce` |
| ردّ HTTP ضائع بعد القبول | إعادة الطلب بنفس المفتاح | `duplicate: true` | لا صف ثانٍ |
| عامل Jami3ati يموت بعد `INSERT` وقبل `POST` | الصف يبقى `queued`، يُلتقط عند إعادة التشغيل | لا شيء | `next_attempt_at` |
| مشترك ينقر إلغاء | `unsubscribed` فورًا، الطابور يتخطاه | صفحة تأكيد | صف محدَّث |
| مفتاح مكرر بقيم مختلفة | يُعاد الصف الأول؛ القيم الجديدة **تُتجاهل** | `duplicate: true` | يُنبَّه في السجل |
| عنوان غير صالح | `400` | خطأ واضح | لا صف |

**قاعدة:** لا يوجد سيناريو يُرجع `200` مع «تم الإرسال» بينما لم يُحفظ شيء.

---

## 9. القابلية للملاحظة (Observability)

لا metrics server جديد. نستخدم ما هو موجود:

| السؤال | الاستعلام |
|---|---|
| كم بريدًا في الطابور؟ | `SELECT status, count(*) FROM outbound_queue GROUP BY status` |
| كم فشل اليوم؟ | `SELECT count(*) FROM audit_log WHERE action='mail.bounce' AND created_at > now() - interval '1 day'` |
| هل التسليم يتأخر؟ | `SELECT max(now() - created_at) FROM outbound_queue WHERE status='pending'` |
| هل الـ worker يعمل؟ | `attempts` يتغيّر خلال دقيقة |
| ما مصير إشعار معيّن؟ | `SELECT * FROM outbound_queue WHERE correlation_id = $1` |

`/api/v1/control-plane-proof` (loopback فقط) يبقى كما هو.

**للعرض:** أمر واحد يطبع الجدول أعلاه قبل الدخول إلى القاعة — رقم حقيقي، لا شاشة مزخرفة.

---

## 10. خطة الاختبار

### 10.1 اختبارات Tira (Rust)

| النوع | ما يثبته |
|---|---|
| Unit (`send.rs`) | رفض `CR/LF`، رفض مفتاح غير صالح، رفض `List-Unsubscribe` غير صالح، قبول طلب سليم |
| Unit (`client.rs`) | `build_message_with` يُصدر الترويسات، ويرفض قيمة بغير ASCII مطبوع |
| Integration (`tests/outbound.rs`, DB) | 202 + صف `pending`؛ idempotency يعيد نفس `id`؛ 401 بلا توكن؛ 404 لمستأجر آخر؛ 400 لحقن |
| Regression | 269 اختبارًا قائمًا تبقى خضراء (كانت 251 قبل هذا الإضاف: +18) |

### 10.2 اختبارات Jami3ati (pytest)

**مقاس 17 نوفمبر 2026 (مساءً): 51 خضراء، 0 فاشلة، 8.17 ثانية.**
التوزيع: `test_deliveries.py` 18، `test_subscriptions.py` 12، `test_tira_outbound.py` 12،
`test_tokens.py` 5 (كلها جديدة = 47)، و`test_health.py` 2 + `test_universities.py` 2 (قائمة).
بالإضافة إلى برهان حيّ عبر لغتين على `tira-api` حقيقي (`202` + نفس `id` عند الإعادة +
`400` لمفتاح غير صالح + `400` لـ`List-Unsubscribe` غير `https`) — التفصيل في §9.42.

| النوع | ما يثبته | الحالة |
|---|---|---|
| Unit | تطبيع البريد، HMAC للتوكن، اختيار المستلمين حسب المؤسسة | ✅ مبني |
| API | `POST /subscriptions` ينشئ `pending` فقط؛ التأكيد ينشئ `active`؛ الإلغاء يعمل مرة واحدة | ✅ مبني |
| Worker | `202` → `accepted`؛ `503` → إعادة جدولة؛ `400` → `failed` بلا إعادة | ✅ مبني |
| E2E | بلاغ → صف `accepted` في Jami3ati + صف `pending/sent` في Tira | ⚠️ نصفه: الطرفان موجودان، والرابط الحيّ جُرِّب بـrelay مغلق (صف Tira `pending`). **لا إثبات تسليم بعد.** |

### 10.3 بروفة العرض (قائمة يدوية)

1. `curl /healthz` على Tira → `{"status":"ok"}`.
2. `SELECT status, count(*) …` → الأرقام مطبوعة ومُصوَّرة.
3. إرسال بلاغ من الواجهة → وصول بريد للمنسّق.
4. فتح `/n/{id}` → `source_url` و`verified_at` ظاهران.
5. إلغاء اشتراك بنقرة → `unsubscribed` في القاعدة.
6. إيقاف relay عمدًا → `503` (إثبات الصدق، لا الادّعاء).

---

## 11. الجدولة

| التاريخ | المخرَج | معيار «تم» |
|---|---|---|
| **17–19 نوفمبر** | ترشّح + هذا الملف + عقد API في Tira | `cargo test` أخضر، ملف الترشّح جاهز |
| **20 نوفمبر 16:30** | إرسال الترشّح | إيصال الإرسال |
| **24–30 نوفمبر** | `subscriptions` + double opt-in + worker Jami3ati | اشتراك → بريد تأكيد → `active` |
| **1–7 ديسمبر** | `fact_history` + poller + `/n/{id}` | تغيير → تنبيه → صفحة تحقّق |
| **8–14 ديسمبر** | ثلاثة إصلاحات Tira (تأكيد الإرسال، حذف الشارة الكاذبة، لوحة تحقّق صادقة) | فحص بصري + اختبارات |
| **15–20 ديسمبر** | بروفة، قياس، لقطة نهائية | كل الأرقام مقاسة في نفس اليوم |
| **21–22 ديسمبر** | العرض | — |

> **تحديث 17 نوفمبر 2026:** صف **24–30 نوفمبر** أُنجز قبل موعده — `subscriptions` +
> double opt-in + `outbox worker` مبنية ومقاسة (51 اختبارًا، `CORRECTIONS.md` §9.42).
> ما بقي من ذلك الصف: `mail_relay` المشترك وقالب البريد **النهائي** (القالب الحالي نص UTF-8
> بسيط داخل `deliveries.py`). بقية الصفوف كما هي.

---

## 12. البوابات (Gates) والتراجع

**قبل كل دفع:** `fmt → clippy → check → dev.ps1 test-db → فحص اتساق الوثائق`. التوقف عند أول فشل مع رقم البوابة وأول سطر خطأ.

**التراجع:** النسخة الاحتياطية لهذه الجلسة في
`C:\Users\HSYSTEM\AppData\Local\Temp\opencode\observatory-pre-20261117-004721\`
(ستة ملفات: `db.rs`, `model.rs`, `store.rs`, `client.rs`, `send.rs`, `lib.rs`).

**المهاجرة 0010 قابلة للتراجع يدويًا:**

```sql
DROP INDEX IF EXISTS outbound_queue_idempotency_key_idx;
ALTER TABLE outbound_queue
    DROP COLUMN IF EXISTS idempotency_key,
    DROP COLUMN IF EXISTS correlation_id,
    DROP COLUMN IF EXISTS list_unsubscribe,
    DROP COLUMN IF EXISTS reply_to;
```

(لا تُنفَّذ إلا إذا لم يكن هناك صفوف API؛ الصفوف القديمة تبقى صالحة لأن الأعمدة `NULL`.)

---

## 13. المخاطر والقرارات المؤجّلة

| الخطر | الاحتمال | الأثر | التخفيف |
|---|---|---|---|
| DKIM غير أخضر على المسرح | متوسط | متوسط | لا نعده؛ `/n/{id}` هو الدليل |
| سمعة نطاق منخفضة | مرتفع | مرتفع | لا signup عام قبل الحدث؛ صندوق مُهيّأ مسبقًا |
| 4 مؤسسات فقط | مؤكد | منخفض | يُقال كرقم مقاس، لا كنقص |
| poller يكسر موقعًا | منخفض | مرتفع | معدّل منخفض + `robots.txt` + User-Agent واضح |
| بيانات شخصية بلا سند قانوني | منخفض | مرتفع | double opt-in + إلغاء + احتفاظ محدّد (loi 09-08) |
| فشل relay أثناء العرض | متوسط | متوسط | dead letter مرئي + `/healthz` قبل الدخول |

**مؤجَّل صراحةً (ليس نقصًا، بل قرار):**

- signup عام (بعد 22 ديسمبر).
- SMS / واتساب (شراكة inwi على المدى الطويل).
- DKIM محلي كامل داخل Tira (المسار الحالي يوقّع عبر relay).
- multi-instance worker (يحتاج مخزنًا مشتركًا للـ limiter).

---

## 14. ما لن نبنيه

- لوحة تحليلات (dashboard) للمشتركين — لا قيمة في العرض.
- إدارة أدوار (RBAC) — التوكن يكفي.
- API عام لقائمة المشتركين — خطر خصوصية بلا مقابل.
- قوالب بريد HTML معقّدة — نص UTF-8 + RTL، لا صور، لا تتبّع.
- تتبّع فتح الرسائل (open pixels) — يتعارض مع خطاب الخصوصية.
