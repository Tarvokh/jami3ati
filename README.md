<h1 align="center">جامعتي · Jami3ati</h1>

<p align="center"><strong>مرصد مواطن للبيانات المفتوحة في التعليم العالي</strong></p>

<p align="center">
سجلٌ عام قابل للاستشهاد لكل تغيير رسمي يهمّ الطالب المغربي —<br/>
مع طبقة تسليم وثقة <strong>Tira Mail</strong> تنقل التغيير إلى من يهمّه، وتُثبت من قاله ومتى.
</p>

---

## لماذا؟

معلومة واحدة — تاريخ التسجيل، شرط الترشّح، نتيجة توجيه — تكون مبعثرة بين
`data.gov.ma` و`enssup.gov.ma` وصفحات المؤسسات، ثم تُنسخ في مجموعات الواتساب
بلا مصدر. النتيجة: طالب يبني قرارًا على معلومة لا يعرف من قالها ولا متى.

## ما نبنيه

ثلاث وظائف، لا أكثر:

1. **أرشفة مؤرّخة** — كل معلومة لها نسخة دائمة على `/n/{id}`، مع `source_url`
   و`verified_at`، متاحة بلا حساب.
2. **تحقّق بشري** — كل كيان مصنّف `verified` أو `needs_review` أو `demo`، والتصنيف
   ظاهر في الواجهة لا مخفيّ في قاعدة البيانات.
3. **تسليم موثوق** — التغيير يصل المشترك بالبريد عبر Tira Mail، مع `notification_id`
   يربط البريد بالنسخة المؤرشفة.

> **قاعدة صارمة:** DKIM/SPF/DMARC تُثبت أن الرسالة جاءت من نطاقنا، لا أن المعلومة
> صحيحة. سلسلة الثقة هي `source_url` + `verified_at` + `notification_id`. لا نخلط
> بينهما، ولا ندّعي توقيعًا من أي وزارة.

## الحالة المقيسة اليوم — 19 نوفمبر 2026

| المكوّن | ما هو | القياس الفعلي |
|---|---|---|
| **Tira Mail** | Rust workspace، 8 crates، 10 migrations، axum 0.8، lettre 0.11.23 (rustls) | **302** اختبارًا ناجحًا، **0** فاشل، **0** متجاهَل — 33 target |
| **Jami3ati API** | FastAPI + PostgreSQL 16 مع RLS | **78** اختبارًا ناجحًا، **0** فاشل — منها **21** لحلقة الرصد |
| **حلقة الرصد** | `sources` · `fact_history` · `poller` · `/n/{id}` | مصدران: رسمي واحد + تقني موسوم غير رسمي؛ **واقعة واحدة** معتمدة؛ صفحة إثبات حيّة (`200`، CSP صارم، بلا JavaScript) |
| **كتالوج العرض** | `src/data/seedData.ts` | **12** كيانًا: 3 جامعات + 9 مؤسسات — منها **10 `verified`** |
| **قاعدة العرض** | `jami3ati_demo` | **3** كيانات: جامعتان + مؤسسة واحدة؛ ومصدران وواقعة واحدة معتمدة |

**آخر تحقق حيّ (19 نوفمبر 2026):** المصدر الرسمي
`https://www.uit.ac.ma/category/admission/` أُعيد استقصاؤه فعاد `not_modified`
بلا واقعة — لا نختلق تغييرًا. والمصدر التقني المحلي موسوم `is_official=false`،
وواقعته تحمل `source_change` لا `official_change`.

الكتالوج بيانات عرض موسومة (`demo` / `needs_review` ظاهرة للمستخدم)، وليست تغطية
وطنية. القاعدة تحتوي عيّنة أصغر من الكتالوج، ونقول ذلك صراحةً بدل رقم واحد يصلح
لسياق ولا يصلح لآخر.

## العمارة

```text
data.gov.ma    ─┐
enssup.gov.ma  ─┼─▶ poller ─▶ PostgreSQL 16 (RLS) ─┬─▶ /n/{id}   public archive
institutions   ─┘        (robots.txt)               │
                                                    ├─▶ React 19  works without JS
                                                    │
                                                    └─▶ outbox ─▶ Tira Mail
                                                                  POST /api/v1/outbound
                                                                  202 Accepted
                                                                       │
                                                                       ▼
                                                                  SMTP relay ─▶ inbox
```

**حدّ المسؤولية:** Jami3ati تقرّر *ماذا* و*لمن*؛ Tira تنفّذ *كيف*. بينهما عقد HTTP
فقط — لا قاعدة بيانات مشتركة، ولا استيراد كود.

## عقد التكامل

| العنصر | القيمة |
|---|---|
| المسار | `POST /api/v1/outbound` |
| المصادقة | `Authorization: Bearer <token>` |
| منع التكرار | `idempotency_key` إلزامي، `UNIQUE (tenant_id, idempotency_key)` جزئي |
| الاستجابة | `202 Accepted` — **قبول ≠ تسليم** |
| الترويسات | `X-Jami3ati-Notification`، `Reply-To`، و`List-Unsubscribe` فقط عندما يبدأ `JAMI3ATI_PUBLIC_BASE_URL` بـ`https://` |
| إعادة المحاولة | `429` / `5xx` / خطأ نقل → `30s → 900s`، ثم `dead` بعد 3 محاولات |

أُثبت العقد حيًّا بلغتين: Tira قبلت الحمولة بـ`202` وأعادت **نفس المعرّف** عند
إعادة الإرسال بنفس المفتاح، ورفضت بـ`400` مفتاحًا غير صالح و`List-Unsubscribe`
غير `https`.

## بنية المستودع

```
.
├── mail.ma/                      ← Tira Mail (Rust workspace)
│   ├── crates/                   ← 8 crates: api · auth · frontend · module
│   │                                provision · smtp · store · vault
│   ├── scripts/dev.ps1           ← بوابات fmt · clippy · check · test-db
│   └── docs/                     ← DEEP_MAP · DECISIONS · CORRECTIONS
└── -Jami3ati-main/
    └── -Jami3ati-main/           ← Jami3ati (React + FastAPI)
        ├── src/                  ← واجهة React 19 · RTL-first
        ├── backend/              ← FastAPI · tests · seed
        └── docs/                 ← api · concept · data-sources · decisions
```

> اسم المجلد المزدوج `-Jami3ati-main/-Jami3ati-main` أثر فكّ أرشيف المشروع الأصلي؛
> جذر التطبيق هو المجلد الداخلي.

## التشغيل محليًا

> الدليل الكامل المقيس من نسخة نظيفة — قاعدة البيانات، المتغيّرات، الاختبارات:
> **[QUICKSTART.md](QUICKSTART.md)**.

**Tira Mail** (يتطلب PostgreSQL على `127.0.0.1:55432`):

```powershell
cd mail.ma
$env:TIRA_REQUIRE_DATABASE = "1"
.\scripts\dev.ps1 test-db
cargo run -p tira-api          # http://127.0.0.1:8080
```

**Jami3ati** (طرفان):

```powershell
cd ".\-Jami3ati-main\-Jami3ati-main"
npm ci
npm run dev                    # http://127.0.0.1:3000

cd backend
pip install -r requirements.txt
uvicorn app.main:app --host 127.0.0.1 --port 8000
```

الواجهة تنادي `/api/v1` على أصلها نفسه، وVite يمرّرها إلى `127.0.0.1:8000`
(`server.proxy` و`preview.proxy`). لا حاجة إلى `VITE_API_BASE_URL` إلا عند نشر
`dist/` على مضيف ثابت بلا proxy.

## الأمان والخصوصية

- **CSP صارم:** `default-src 'self'`، `frame-ancestors 'none'`، و`X-Frame-Options: DENY`؛
  بلا `style` أو `script` مضمّنين.
- **بلا JavaScript أصلًا:** القراءة والإرسال يعملان بـ`<form>` عادي (progressive enhancement).
- **RLS على PostgreSQL:** العزل على مستوى الصف، لا على مستوى التطبيق وحده.
- **بريد فقط في النسخة التجريبية:** لا SMS ولا هاتف؛ double opt-in، ومدة حفظ محدّدة،
  ورابط إلغاء اشتراك في كل بريد — وفق **loi 09-08 / CNDP**.
- **لا تسجيل عام قبل الحدث:** العرض يستعمل صندوقًا مُهيّأً مسبقًا.
- **الأسرار خارج الشجرة:** `.env*` و`data/` و`target/` و`node_modules/` مستبعدة بقواعد
  صريحة، ولا يُكتب أي توكن في المستودع.

## ما لا ندّعيه

- لا ندّعي توقيعًا رقميًا من أي وزارة أو مؤسسة.
- لا ندّعي أن `202` يعني وصول البريد؛ التسليم يُقاس في `notification_deliveries`.
- لا ندّعي إرسال بريد في هذه النسخة: بيانات Tira (`TIRA_OUTBOUND_TOKEN` /
  `TIRA_MAILBOX_ID`) غير مهيّأة، وصفّا التسليم ما زالا `pending`.
- لا ندّعي تغطية وطنية: الكتالوج 12 كيانًا (10 `verified`)، والقاعدة 3.
- لا يوجد API رسمي بعد؛ نعتمد استقصاءً دوريًا منخفض المعدّل محترمًا لـ`robots.txt`.
- لا نضمن أن يظهر DKIM «أخضر» على المسرح؛ الدليل الأساسي هو `/n/{id}` و`source_url`.

## الفريق

| الدور | الاسم |
|---|---|
| قيادة تقنية / معمارية | Sami Benali |
| واجهة أمامية | Ismail Benali |
| تحقّق من المصادر | Elmoufid Hamza |

## الترخيص

- **Tira Mail** (`mail.ma/`): `AGPL-3.0-or-later`، كما هو معلَن في `Cargo.toml`.
- **Jami3ati** (`-Jami3ati-main/`): جميع الحقوق محفوظة للفريق. الشيفرة منشورة
  للاطّلاع والتقييم في إطار الهاكاثون، ولا تمنح ترخيصًا ضمنيًا بإعادة الاستخدام
  التجاري.

---

## English summary

**Jami3ati** (*"my university"*) is a citizen observatory for open data in Moroccan
higher education: every official change gets a dated, citable page at `/n/{id}`,
with `source_url` and `verified_at`, readable without an account.

**Tira Mail** is the delivery and trust layer: a Rust workspace (8 crates, axum 0.8,
PostgreSQL 16 with RLS, outbox pattern, DKIM, strict CSP, RTL-first, works without
JavaScript). Jami3ati decides *what* and *to whom*; Tira executes *how* — over a
single HTTP contract (`POST /api/v1/outbound`, `202 Accepted`, mandatory
`idempotency_key`, closed header set). No shared database.

**Measured on 19 November 2026:** 302 Tira tests passed (0 failed, 0 ignored,
33 targets); 78 Jami3ati tests passed (21 for the observation loop). The loop is
live: 2 registered sources (1 official, 1 non-official rehearsal), 1 approved
fact, and a public `/n/{id}` page (200, strict CSP, no JavaScript). Demo
catalogue: 12 entities, 10 `verified`. Demo database: 3 entities (2 universities
+ 1 institution), plus 2 sources and 1 fact.

**No email was sent in this build:** Tira credentials are not configured, so the
two delivery rows remain `pending`.

**What we do not claim:** no ministry signature; `202` is not delivery; no official
API exists yet (polite polling only); DKIM authenticates the domain, not the fact.
The trust chain is `source_url` + `verified_at` + `notification_id`.
