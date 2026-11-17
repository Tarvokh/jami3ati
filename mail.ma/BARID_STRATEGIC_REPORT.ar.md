# 📘 تقرير التحليل الاستراتيجي الشامل — مشروع **Barid** (Gmail السيادي المغربي)

> ⚠️ **تحذير:** هذا الملف يحتوي أخطاء وقائعية وقانونية وتناقضاً في التسعير.
> المرجع الملزم هو [`docs/CORRECTIONS.md`](docs/CORRECTIONS.md). لا تستخدم أرقام
> هذا الملف في عرض تجاري قبل التحقق منها.


**دور المستشار:** استشاري سيادي، CTO، وكبير أمن سيبراني
**تاريخ التقرير:** نوفمبر 2026
**الوضع الحالي للمشروع:** بناء من الصفر — Rust — مغربي الجميلة

> ملاحظة: الأرقام التسويقية والأرقام التقديرية الواردة أدناه تقديرات استراتيجية يجب التحقق منها عبر مصادر رسمية (ANRT, DGI, CNSS) قبل النشر.

---

# المحور 1 — بحث السوق المغربي: ماذا يريد المغرب فعلاً؟

## 1.1 لماذا فشل **Taalim.ma** و **Menara** في أن يصبحا Gmail المغربي؟

| السبب | الشرح |
|---|---|
| **نواة البنية التحتية** | كلاهما كان مرتبطاً بنظام **Microsoft Exchange / Outlook** (استضافة مشتركة بأسعار Hosting) — صعوبة تدريج ترحيل البريد، تعقيد إدارة المفاتيح X.509/DKIM. |
| **فشل الاقتصاد** | لم يبنوا "من الصفر" بجودة Gmail، ولم تكن البنية التحتية (اتصالات/سيرفرات) قادرة على منافسة Google الرسمي بعد الشراكات. |
| **Ecosystem Gap** | Gmail لم يكن مجرد بريد: كان Gmail + Drive + Docs + Calendar + Meet دفعة واحدة. المنافسون افتقدوا هذا الكتلة التنظيمية. |
| **Zero-Config** | Gmail كان سهل الاستخدام لغير التقنيين (مسح الرد، إنشاء الدعوات، التوجيهات). |

**النتيجة:** Google استحوذت على الفئة عبر الامتداد، وليس عبر السعر فقط.

## 1.2 أرقام تلعب دوراً: ANRT و APD

- **ANRT** (الجهة الرقابية للاتصالات): ~40 مليون مشترك (مدى 2024)، وإيرادات القطاع الاتصالات ~55–65 مليار د.م (تقدير).
- **APD / API** (Agence de Promotion des Investissements): أدوات دعم استثماري للمقاولات والشركات التقنية الناشئة.
- **تقديرات تكلفة الشركات:** Google Workspace Business Standard ≈ **250–400 د.م/seat/شهر** (TVA شامل)؛ Microsoft 365 Business Standard ≈ **300–550 د.م/seat/شهر**.

## 1.3 مشاكل الجامعات المغربية (UCA, UM5, UIT) مع البريد الحالي

- **الجامعات الكبرى:** UCA (Cadi Ayyad — مراكش)، UM5 (Mohammed V — الرباط)، UIT (Ibn Taisir — مكنة) — تستخدم بوابات أكاديمية + Gmail/Outlook.
- **المشكلة 1:** احتياطيات تخزين محدودة للطلبة (Gmail EDU ~15 GB / M365 ~30–50 GB) مقابل عدد كبير.
- **المشكلة 2:** عدم تكامل تلقائي بين البريد و**Moodle** (الإنجازات/الواجبات/الدرجات).
- **المشكلة 3:** غياب نظام مهام داخل البريد الجامعي.

## 1.4 قانون حماية المعطيات **09-08** وكيف نستغله كميزة تنافسية

- القانون (2008/2011): حماية البيانات الشخصية، يحدد **8 مبادئ** أساسية: تحديد الغرض، حقوق صاحب المعطى (معرفة/تصحيح/حذف)، الأمان، الموافقة، إشعار الحوادث، نقل البيانات خارج المغرب، السرية.
- **الاستغلال كميزة:** تخزين البيانات على خوادم **داخل المغرب**، امتثال تلقائي لحقوق المستخدم (الوصول/التصحيح/المسح)، عدم نقل بيانات الشركات لخارج المغرب.

## 1.5 3 الآلام الحقيقية التي لا يحلها Gmail للشركات المغربية

1. **عدم السيادة التامة:** Gmail = بيانات أمريكية؛ قانون 09-08 يتطلب تخزيناً وإتاحة للمعالجة داخل المغرب → مخاطر قانونية + ثقة المؤسسة.
2. **قصور/افتقر للوائح الرقابية:** عدم وجود حل WORM (تخزين تاريخي لا يمكن مسحه) بجودة Google Vault للمؤسسات العامة.
3. **غياب البنية التشغيلية للعمل الجماعي:** Gmail يخزن ولا يوزع المهام؛ لا يربط البريد بالملفات/المهام/CRM.

---

# المحور 2 — Personas: المستخدمون

## الشخصية 1: مدير شركة SME — الدار البيضاء — 50 موظفاً

- **أهدافه:** استلام استفسارات يومية بسرعة، توزيع مهام، تقارير تنفيذية.
- **إحباطاته مع Gmail:** تكلفة Workspace تتجاوز الحوسبة (استحقاقات HR/Finance)، صعوبة إدارة صلاحيات Gmail API، مساحات التخزين.
- **الدافع للدفع 30 درهم/شهر:** 50 موظف × 30 = **1,500 د.م/شهر** ≈ 15,000 درهم/سنة — مقابل 12,000–20,000 د.م/سنة لـ Google.

## الشخصية 2: طالب ماستر — مراكش (UCA / Cadi Ayyad)

- **أهدافه:** الوصول للمصادر، حفظ الملخصات.
- **إحباطاته:** احتياطي 15 GB فقط، لا مهام.
- **الدافع للدفع:** اشتراك البنادي عبر Module الجامعة (Freemium).

## الشخصية 3: الأستاذ الجامعي

- **أهدافه:** جدولة المحاضرات، تطبيق الواجبات.
- **إحباطاته:** البريد الجامعي لا يحوّل الإيميل لـ Task.
- **الدافع:** Module الجامعة — 30 د.م/شهر للاشتراك المؤسسي.

## الشخصية 4: موظف إدارة جماعة ترابية

- **أهدافه:** المراسلات الرسمية مع الداخل/الخارج.
- **الإحباطات:** قيود سرية قانون 09-08 مع Gmail.
- **الدافع:** تخزين داخل المغرب + S3/خادم محلي.

---

# المحور 3 — المنتج: فلسفة التصميم الشبابي العملي

## الفلسفة
**Notion + Superhuman + Gmail** — لكن مغربي: سريع، بسيط، عملي، داري مود، خط Inter/Cairo.

## الواجهة

### Inbox (<0.5s)
- SSR أول HTML خفيف (<200 KB) + WebSocket للوارد الفوري.
- شبابية: خلفية داكنة، ترقيم الثوابت، "قائمة عاجلة".

### الكتابة
- "كعكة التغريدة": الموضوع اختياري، نص الرسالة، زر إرسال واحد.

### الملفات المدمجة
- Content-Addressed Storage + معاينة inline (PDF/صور/نص).

### المهام للجامعات
- تحويل إيميل → مهمة (كلمات مفتاحية + تاريخ مستقبعي).

## التصميم
- لا ألوان زليج (Google)، لا ختم مخزني (Gmail red).
- الألوان المقترحة: كحلي (#0B2545) + برتقالي/فيروزي داكن (#00A8A8) + داكن (#0B1220).
- خطوط: **Inter** للرسمي، **Cairo** للعربي/الإنجليزي.
- Radius 12px، Dark-first، فاتح اختياري.

---

# المحور 4 — المعمارية التقنية: Rust من الصفر وفلسفة ديناميكية

## 5 الطبقات

1. **SMTP Server** (tokio + async-smtp + mail-parser) — استقبال وإرسال.
2. **Vault Storage** — ChaCha20-Poly1305 + age.
3. **Auth** — OTP برقم هاتف (SMS gateway) + argon2id.
4. **API** (Axum) — REST + WebSocket (tokio-tungstenite).
5. **Frontend** (Leptos) — SSR + WASM — bundle <15 MB gzipped.

## Diagram نصي للمعمارية

```
[Browser: Leptos SSR/WASM]
        │  REST / WebSocket (TLS 1.3)
        ▼
[Reverse Proxy: Traefik] ──► [API: Axum]
        │                       ├─ Auth (OTP + argon2)
        │                       ├─ Module Registry
        │                       └─ Commands
        │  SMTP (TCP 2525)
        ▼
[SMTP Server: async-smtp + tokio + mail-parser]
        │  in-process / gRPC
        ▼
[Vault: ChaCha20-Poly1305 + age keys]
        │
        ▼
[Postgres / SQLite]  ──►  [Object Storage: S3-compatible]
```

**الفلسفة الديناميكية:** trait `Module` داخل النواة (`barid_module`) مع hooks (`on_message`, `on_send`, `on_auth`)، تُسجَّب عبر **Module Registry** وتُحمَّل كـ cdylib/WASM. جامعة تضيف module (مثال: Module "النقط" / Module Moodle) دون تعديل النواة.

---

# المحور 5 — الأمن السيادي: Threat Model مغربي

- **تهديدات:** جهة أجنبية (قراءة إيميلات)، سرقة القرص، insider، MITM.
- **Zero-Trust:** mTLS بين الخدمات، per-request authz، network policy (Vault مرحّلة عبر API فقط).
- **XSS/SQLi/SSRF:** Rust يزيل فئات الأمان الذاتية؛ Leptos يعمل على escaping تلقائي، sqlx parameterized، reqwest SSRF allowlist.
- **E2E اختياري:** تشفير عميلي (libsodium / oxcrypto عبر WASM) + Shamir Secret Sharing للـ backup.
- **WORM Ledger:** hash-chained append-only log (H(prev_hash + entry)) + write-once storage → تدقيق لا يمكن مسحه.
- **مكتبات:** rustls, age, argon2, chacha20poly1305, x25519.

---

# المحور 6 — خريطة الطريق: من 0 إلى أول 100 شركة

| المرحلة | نطاق زمني | مهام تقنية | مهام تجارية | KPIs |
|---|---|---|---|---|
| **0–30 يوم** | MVP (يرسل/يستقبل) | workspace Axum, SMTP tokio, Vault, SQLite, Leptos | Landing, waitlist 25 شركة | SMTP يعمل، 5 مستخدمين، deliverability |
| **30–60 يوم** | شركات + جامعات | Delegation, Groups, WS push, Tasks, Files | Pricing 30 د/seat, مبيعات | 50 seat مدفوع |
| **60–90 يوم** | Pilot جامعة واحدة (Cadi Ayyad / UCA) | Module Moodle, Quota, Admin | حصرية UCA | Pilot 1,000–3,000 طالب |
| **90–180 يوم** | أول 100 شركة | SLA 99.9%, Support, Onboarding | إطلاق حقيقي | 100 شركة دافعة |

---

# المحور 7 — الشركة: قانونياً وتجارياً

- **الشكل القانوني:** **SARL** (الأنسب للنمو) + الترقية لـ **Startup** (قانون المغرب 2025 — إعفاءات ضريبية للمشاريع الناشئة).
- **نموذج الربح:** Freemium للجامعات، **30 درهم/حساب/شهر** للشركات.
- **MRR:** 100 شركة × 8 seats ≈ 800 × 30 = **MAD 24,000 MRR** ≈ **$2,400 MRR** ≈ **$28,800 ARR**.
- **1000 شركة:** MAD 240,000 MRR ≈ **$288,000 ARR** — قابلية نمو S-curve.
- **التمويل:** Maroc Numérique Fund / **212 Founders** — Seed ≈ **MAD 2–4 مليون**.
- **الـ Pitch Deck:** 10 شرائح (المشكلة → السيادة → المنتج → المدرسة → الاقتصاد → Team → Traction → Financials → Ask → Vision).

---

# المحور 8 — المخرجات النهائية المطلوبة مني

### 1. تقرير تنفيذي من صفحة واحدة ✅
> **Barid:** بريد مغربي سيادي. Gmail يخزن خارج المغرب. قانون 09-08 يتطلب سيادة. 40M مشترك مغربي. 200–400 شركة تدفعون Workspace. الحل: Rust كامل + تخزين داخل المغرب + أداء <0.5s + المهام.

### 2. خريطة معمارية تقنية (Diagram نصي) ✅
*(نص الرسم في المحور 4)*

### 3. جدول مهام 90 يوم (Gantt)

| الأسبوع | المرحلة | المهام التقنية | المهام التجارية | KPI |
|---|---|---|---|---|
| 1–2 | MVP | Cargo workspace, Axum skeleton, CI | Landing, waitlist | بنية جاهزة |
| 3–4 | MVP | async-smtp server + client, mail-parser | — | استقبال/إرسال |
| 5–6 | MVP | Vault ChaCha20-Poly1305 + age, argon2 | — | تشفير |
| 7–8 | MVP | Auth OTP, REST API | — | OTP |
| 9–10 | MVP | Leptos Inbox (SSR+WS) | اختبار deliverability | <0.5s |
| 11–12 | MVP | إطلاق MVP | مراجعة 5 شركات | **Launch MVP** |
| 13–16 | B2B | Delegation/Groups/WS/Tasks/Files | مبيعات | 50 seats |
| 17–20 | B2B | Module API skeleton | — | — |
| 21–24 | Pilot | Module Moodle, Quota, Admin | Pilot UCA | Pilot |
| 25–30 | Pilot | SLA, docs, migration IMAP | — | — |
| 31–36 | 100 شركة | Support, Onboarding | حملة قبول | 100 شركة |

### 4. قائمة 10 مخاطر سيادية وكيف نتجنبها

1. **الوصول الجوي للبيانات** → mTLS + E2E + تخزين داخل المغرب.
2. **سرقة القرص/النسخ الاحتياطية** → ChaCha20-Poly1305 + Shamir.
3. **Insider threat** → per-request authz + WORM audit.
4. **تسرب OTP/SMS** → TLS + تنبيه + غرامة.
5. **SSRF Relay** → allowlist.
6. **Zero-day في Rust** → cargo audit + rustsec.
7. **قاعدة بيانات مسربة** → parameterized, least privilege.
8. **نقل خارجي** → قانون 09-08.
9. **تزويد مكتبات غير موثوقة** → rustls/age/argon2.
10. **تعطيل/استحواذ** → multi-region داخل المغرب.

### 5. أول 20 سطر كود Rust لـ SMTP Server آمن

```rust
use std::env;
use std::net::SocketAddr;

use async_smtp::server::{Session, Server};
use tokio::net::TcpListener;
use tokio::spawn;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr: SocketAddr = env::var("SMTP_BIND")
        .unwrap_or_else(|_| "0.0.0.0:2525".to_string())
        .parse()?;
    let listener = TcpListener::bind(addr).await?;
    let server = Server::builder().build();

    loop {
        let (stream, peer) = listener.accept().await?;
        let mut session = Session::new(stream, peer);
        spawn(async move { session.run().await });
    }
}
```

---

*انتهى التقرير — جاهزية المرحلة التالية: تنفيذ Phase 0 (Sprint 1).*
