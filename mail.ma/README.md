# Tira Mail

البريد السيادي المغربي — من **IFRU HOLDING** / **TIRA LABS**.

> الحالة: **P1 + قراءة HTTP + P2 كاملة، مع مصادقة مسارات القراءة ونسخ
> احتياطي مشفَّر مُثبت بالاسترجاع، على محرك Stalwart محلياً.** الرسالة الواردة
> تُحفظ في PostgreSQL وتُقرأ بعد إعادة التشغيل وتُعرض على `/mail/{id}`.
> وأُثبتت جلسة **IMAP كاملة** (STARTTLS → LOGIN → APPEND → FETCH)، وتسليم
> **SMTP** وارد وصادر مع AUTH PLAIN، و**JMAP** مُصادَق يقرأ الرسائل ويفكّ
> ترميز الموضوع العربي. وأُثبت **الجسر من طرف إلى طرف**: حساب في `tira_test`
> → `tira-provision` → حساب Stalwart → تسليم عابر للنطاقات
> (`youssef@uit.ac.ma` → `aicha@tira.test`) → قراءة عبر IMAP.
>
> كل قراءة مع قاعدة بيانات صارت **محمية برمز API**: الرمز عشوائي ومُجزّأ
> بـSHA-256 في القاعدة (لا يُخزَّن النص)، ومقيَّد بمستأجر واختيارياً بصندوق.
> بلا رمز `401`، وعبر المستأجرين `404` (لا `403`، حتى لا تُجرَّب المعرّفات)،
> وبلا قاعدة `503`. والنسخ الاحتياطي يغطي PostgreSQL + مخزن الـblobs، ويُشفَّر
> بصيغة `TIRAS2` (ChaCha20-Poly1305 + Argon2id لمرة واحدة لكل ملف، مع كشف
> الاقتطاع عبر إطار نهائي مُصادَق عليه؛ ويقرأ `TIRAS1` القديم)، ويمرّ
> بسكربت استرجاع يتحقق من كل blob بالـSHA-256.
>
> ما لم يُثبت بعد: شهادة حقيقية (ACME)، DNS، SPF/DKIM/DMARC، عزل RLS،
> وترحيل مؤسسة. راجع [`docs/CORRECTIONS.md`](docs/CORRECTIONS.md) قبل الاعتماد
> على أي رقم أو ادعاء قانوني في ملفات الاستراتيجية.

---

## الخريطة

- [`docs/DEEP_MAP.ar.md`](docs/DEEP_MAP.ar.md) — الخريطة العميقة: المعمارية،
  الفجوات، الطريق، المخاطر، الاقتصاد، الامتثال.
- [`docs/deep-map.html`](docs/deep-map.html) — نفس الخريطة بصرياً (افتحها في المتصفح).
- [`docs/ICONS.md`](docs/ICONS.md) — مصادر الأيقونات، رخصها، وقواعد استخدامها.
- [`docs/DECISIONS.md`](docs/DECISIONS.md) — سجل القرارات D1–D6: المحرك،
  الترخيص، التسعير، أول عميل، الاستضافة، SMS.

## ما يعمل الآن

| Crate | الوظيفة |
|---|---|
| `tira-smtp` | خادم SMTP داخل (tokio) + عميل إرسال (lettre) + تحليل MIME + تحقق المستلمين |
| `tira-store` | PostgreSQL 16 + blob store بمفاتيح sha256 + حصة + سجل تدقيق |
| `tira-vault` | تشفير at-rest: ChaCha20-Poly1305 + Argon2id |
| `tira-auth` | OTP برقم الهاتف + تجزئة كلمات المرور Argon2id |
| `tira-module` | نظام إضافات (trait + registry) للجامعات |
| `tira-provision` | جسر الهوية: `tira-store` → Stalwart عبر `stalwart-cli` idempotent + `--reconcile` |
| `tira-api` | HTTP API (Axum): health + version + واجهة الوارد، ويشغّل SMTP والقاعدة معاً |
| `tira-frontend` | HTML shell آمن (RTL، dark-first) + أيقونات Lucide/line-md مضمّنة |

> **8 crates في الـworkspace.** الواجهة الحالية HTML من الخادم (بلا JavaScript)
> وتُقدَّم على `GET /`. Leptos/WASM مرحلة لاحقة، بعد وجود API بريد كامل.

### المحرك: Stalwart 1.0.2 (مُثبت محلياً)

`tira-*` هي **control plane** (هوية، مستأجرون، تدقيق). تخزين البريد وبروتوكولات
المزامنة ملك Stalwart:

| المنفذ | البروتوكول | ما أُثبت |
|---|---|---|
| 25 | SMTP وارد + STARTTLS | `MAIL/RCPT/DATA` بلا مصادقة → `250 queued` → ظهور الرسالة في صندوق IMAP |
| 2526 | SMTP submission | يرفض `MAIL` قبل المصادقة (`503`)، و`AUTH PLAIN` → `235` |
| 1143 | IMAP + STARTTLS | `LOGINDISABLED` قبل TLS، ثم LOGIN، 5 مجلدات، `APPEND` 392 بايت، `FETCH` يعيد Message-ID والموضوع العربي |
| 1993 | IMAPS (TLS مباشر) | LOGIN ناجح، `AUTH=PLAIN LOGIN XOAUTH2 OAUTHBEARER` |
| 8443 | JMAP over HTTPS | جلسة مُصادَقة، 18 قدرة، 5 مجلدات، 3 رسائل، فكّ ترميز عربي |

سكربتات الإثبات (تعمل على الخادم المحلي فقط):

```powershell
.\scripts\imap-proof.ps1     # STARTTLS + LOGIN + APPEND + FETCH + IMAPS
.\scripts\smtp-proof.ps1     # وارد بلا مصادقة + AUTH PLAIN + تحقق عبر IMAP
.\scripts\jmap-proof.ps1     # session + Mailbox/get + Email/query + Email/get
.\scripts\bridge-proof.ps1   # submission عابر للنطاقات + قراءة عبر IMAP
```

إعداد حساب إدارة دائم للمحرك (مرة واحدة لكل تثبيت):

```powershell
$env:STALWART_RECOVERY_PASSWORD = "..."
$env:STALWART_ADMIN_PASSWORD    = "..."
.\scripts\stalwart-admin.ps1
```

> الدور الإداري الصحيح في Stalwart 1.0.2 هو `{"@type":"Admin"}` وليس
> `"Administrator"`. السكربت يكتشف `domainId` من المحرك، ولا يضع كلمة المرور
> في `argv`، وتدوير الكلمة يتم بإعادة تشغيله.

> **الشهادة الحالية ذاتية التوقيع** (ECDSA P-256، SANs: `mail.tira.test`,
> `tira.test`, `localhost`, `127.0.0.1`) ومولَّدة بـ `rcgen`. لا تصلح للإنترنت:
> تحتاج ACME/Let's Encrypt بعد ضبط DNS.
>
> **رخصة Stalwart: AGPL-3.0.** تشغيله كخدمة شبكة يفرض نشر الكود المعدَّل
> (بما فيه تعديلاتنا على Stalwart، لا على `tira-*` المنفصلة) لمن يتعامل معه
> عبر الشبكة. القرار التجاري (D2) ما زال مفتوحاً.

## البناء والتشغيل

```powershell
.\scripts\pg-dev.ps1        # PostgreSQL محمول على 127.0.0.1:55432
.\scripts\dev.ps1 check     # cargo check --workspace --all-targets
.\scripts\dev.ps1 test      # كل الاختبارات (اختبارات القاعدة تُتخطّى بلا اتصال)
.\scripts\dev.ps1 test-db   # 276 اختباراً، مع رفض التخطي
.\scripts\dev.ps1 clippy    # clippy مع -D warnings
.\scripts\dev.ps1 run       # tira-api على http://127.0.0.1:8080
```

الاختبارات الـ56 التي تتطلب PostgreSQL حقيقياً موزّعة على ملفات الاختبار هذه:
`tira-store/tests/persist.rs` (5)، `tira-store/tests/rls.rs` (7)،
`tira-store/tests/storage_safety.rs` (4)، `tira-store/tests/pool_timeouts.rs` (2)،
`tira-api/tests/read.rs` (9)، `tira-api/tests/proof.rs` (10)،
`tira-api/tests/session.rs` (3)، `tira-api/tests/outbound.rs` (8)،
و`tira-provision/tests/bridge.rs` (8).
الباقي اختبارات وحدة بلا شبكة ولا قاعدة بيانات.

### جسر الهوية

```powershell
$env:TIRA_DATABASE_URL = "postgres://postgres@127.0.0.1:55432/tira_test"
$env:STALWART_URL      = "http://127.0.0.1:8080"
$env:STALWART_USER     = "admin@tira.test"
$env:STALWART_PASSWORD = "..."

.\target\debug\tira-provision.exe --list-pending
.\target\debug\tira-provision.exe --mailbox <uuid> --dry-run
$env:TIRA_PROVISION_PASSWORD = "..."   # أو اكتبها عند الطلب
.\target\debug\tira-provision.exe --mailbox <uuid>
.\target\debug\tira-provision.exe --reconcile          # قراءة فقط، exit 2 عند الانحراف
.\target\debug\tira-provision.exe --reconcile --fix    # يحوّل المنحرف إلى failed
```

- كلمة المرور **لا تُخزَّن في control plane**، ولا تُقبل على سطر الأوامر (تظهر
  في قائمة العمليات)؛ تُقرأ من البيئة أو من stdin ثم تُسقط.
- اعتماد المسؤول (`STALWART_URL/USER/PASSWORD`) يُمرَّر إلى `stalwart-cli` عبر
  **بيئة** العملية الابنة لا عبر وسائطها، والخطة (وفيها كلمة مرور الصندوق لمرة
  واحدة) تُمرَّر عبر **stdin** (`apply --stdin`) فلا تلمس القرص إطلاقاً — لا ملف
  مؤقت يتسرّب أو يبقى بعد انهيار، على أي نظام تشغيل.
- `--reconcile` يقرأ المحرك **قراءتين فقط** (`Domain` ثم `Account`) مهما كان
  عدد الصناديق، ولا يُصلح تلقائياً: الحساب الغائب قد يعني حذفاً مقصوداً أو
  استرجاعاً فاشلاً أو عنوان إدارة خاطئاً.

تشغيل مع التخزين:

```powershell
$env:TIRA_DATABASE_URL = "postgres://postgres@127.0.0.1:55432/tira_test"
$env:TIRA_BLOB_ROOT    = ".\data\blobs"
$env:TIRA_SMTP_ADDR    = "127.0.0.1:2526"
.\target\debug\tira-api.exe
# ثم: http://127.0.0.1:8081/healthz  ->  {"status":"ok","database":"ok"}
```

### مسارات القراءة (محمية برمز API)

```
GET /                                                صفحة عرض عامة (بيانات تجريبية)
GET /mail/{mailbox_id}                              صفحة HTML عربية RTL — تتطلب رمزاً
GET /api/v1/mailboxes/{mailbox_id}/messages         JSON + limit/offset + total — رمز
GET /api/v1/messages/{message_id}/raw               بايتات RFC 5322 كما قُبلت — رمز
```

الرمز يُرسل في `Authorization: Bearer <token>` أو في كوكي `tira_token` لصفحات
HTML. **لا تُقبل الرموز في سلسلة الاستعلام** (تسريبها في السجلات و`Referer`).
التحقق من المستودع يقرأ `tenant_id` و`mailbox_scope` من صف الرمز، لا من الطلب:

| الحالة | الاستجابة |
|---|---|
| لا رمز، رمز مُبطَل، أو منتهٍ | `401` |
| صندوق/رسالة في مستأجر آخر، أو خارج `mailbox_scope` | `404` (لا `403`) |
| قاعدة البيانات غير مهيأة | `503` |
| رمز صالح ونطاق مطابق | `200` |

إصدار رمز لصندوق واحد (يُطبع النص مرة واحدة فقط؛ القاعدة تحفظ SHA-256):

```powershell
$env:TIRA_DATABASE_URL = "postgres://postgres@127.0.0.1:55432/tira_test"
.\target\debug\tira-token.exe --tenant 11111111-1111-4111-8111-111111111111 `
    --mailbox 44444444-4444-4444-8444-444444444444 --label "pilot-aicha" --ttl-days 90
.\target\debug\tira-token.exe --tenant 11111111-1111-4111-8111-111111111111 --list
.\target\debug\tira-token.exe --revoke <token-id>
```

> `GET /` نفسه يبقى عاماً ويعرض بيانات تجريبية ثابتة، تماماً كما يُختبر في
> `crates/tira-api/tests/health.rs`. لا تعتبره تسريباً: لا يقرأ القاعدة.
> الربط على `0.0.0.0` مسموح الآن من ناحية المصادقة، لكنه يبقى خلف TLS reverse
> proxy في الإنتاج.

الواجهة الحالية ليست صورة ولا mockup: `GET /` يبني نفس HTML الذي يُختبر في
`crates/tira-api/tests/health.rs`. الأيقونات مضمّنة في
`crates/tira-frontend/src/icons.rs` (Lucide ثابتة + line-md متحركة، بلا CDN)؛
القواعد والرخص في [`docs/ICONS.md`](docs/ICONS.md).

```powershell
$env:TIRA_BIND = "127.0.0.1:8090"   # 8080 مشغول بـ Stalwart محلياً
.\target\debug\tira-api.exe
# افتح http://127.0.0.1:8090/
```

> **Smart App Control على جهاز التطوير:** بعد تحوّله إلى وضع الفرض، يُرفض أي
> `.exe` حديث بـ `os error 4551`. التوقيع وحده لا يكفي: ملف موقَّع بـ
> `Status: Valid` رُفض فعلاً، وما نجح هو إعادة بناء الحزمة (تتغيّر التجزئة)
> ثم توقيعها. إن ظهر `could not execute process ... (never executed)` بعد
> بناء ناجح، أعد بناء الحزمة المعنية ثم شغّل `.\scripts\sign-dev.ps1`.
> هذا عرضي خاص بهذا الجهاز، وليس مطلباً للمنتج.

> **تحذير Windows مُختبَر:** تشغيل PostgreSQL بـ `PATH` الافتراضي يفشل بـ
> `0xC0000142` عند أول اتصال، لأن `jre-8\bin` يحتوي `msvcp140.dll` أقدم من
> نسخة `System32`. `scripts/pg-dev.ps1` ينظّف `PATH` ويستخدم runtime الصحيح.
> لا تحذف هذا السطر — كلّف تشخيصه ساعة.

### النسخ الاحتياطي والاسترجاع

```powershell
$env:TIRA_DATABASE_URL = "postgres://postgres@127.0.0.1:55432/tira_test"
$env:TIRA_BLOB_ROOT    = ".\data\blobs"

# 1) نسخة عادية (غير مشفرة — للفحص فقط)
.\scripts\backup.ps1 -OutDir D:\backups

# 2) نسخة مشفرة (هذا ما يُنقل خارج الجهاز)
$env:TIRA_SEAL_PASSPHRASE = "<عبارة مرور طويلة عشوائية>"
.\scripts\backup.ps1 -OutDir D:\backups -Seal -ChunkMiB 4

# 3) إثبات أن النسخة تُسترجَع فعلاً (قاعدة مؤقتة + blob مؤقت)
$env:TIRA_SEAL_PASSPHRASE = "<نفس العبارة>"
.\scripts\restore-drill.ps1 -BackupDir D:\backups\tira-20261106-120000
```

ما يشمله الأرشيف: `pg_dump` بصيغة custom لكل جداول control plane (مستأجرون،
نطاقات، صناديق، رسائل، مستلمون، رموز مُجزّأة، تدقيق)، و`blobs.tar` لبايتات
RFC 822، و`manifest.json` + `manifest.sha256`. وما **لا** يشمله: مخزن Stalwart
نفسه (لا توجد نسخة online موثّقة في 1.0.2؛ إيقاف المحرك أو أداة snapshot
خاصة به)، وكلمات المرور، ومفاتيح TLS.

- `backup.ps1` يفحص قبل الكتابة أن **كل** blob مذكور في القاعدة موجود فعلاً
  وبالحجم الصحيح؛ وإلا يفشل بدل أن يسلّم أرشيفاً ناقصاً (يتجاوزه
  `-AllowMissingBlobs` للفحص فقط، ويُسجَّل `blobs.state = "incomplete"`).
- يقارن العدّادات قبل وبعد الـdump: إن تساوت فالوضع `quiescent` (نقطة زمنية
  دقيقة)، وإلا `bounded` (نطاق يقبله سكربت الاسترجاع).
- التشفير مرحلتان: يُشفَّر ويُتحقق من كل ملف أولاً، ثم تُحذف النصوص الأصلية.
  أي فشل يزيل مجلد النسخة كاملاً عبر `trap`، فلا يبقى أرشيف نصف مشفَّر.
- `restore-drill.ps1` يستعيد إلى قاعدة `tira_restore_*` فقط (لا يلمس قاعدة
  حقيقية)، ويتحقق من: تجزئة manifest، حجم/تجزئة كل ملف، العدّادات، ترحيلات
  `schema_version`، تطابق `used_bytes` مع مجموع الرسائل، انتماء كل رسالة
  لمستأجر صندوقها، أن تجزئة الرمز 32 بايت، وأن كل blob موجود وبالتجزئة نفسها.
- جرّبت المسار الفاشل أيضاً: عبارة مرور خاطئة، غياب العبارة، وبتّ بايت واحد في
  `db.dump.tira` — كلها تنتهي بفشل صريح ورمز خروج `1`.

### P3 — مصادقة البريد وDNS

```powershell
$env:STALWART_URL      = "http://127.0.0.1:8080"
$env:STALWART_USER     = "admin@tira.test"
$env:STALWART_PASSWORD = "..."

# منطقة DNS + فحص صارم (SPF، MX، DMARC، MTA-STS، TLS-RPT، قوة مفاتيح DKIM)
.\scripts\dns-zone.ps1 -Strict
.\scripts\dns-zone.ps1 -Domain uit.ac.ma

# DMARC المرحلي: none → quarantine (25%) → quarantine (100%) → reject
.\scripts\dns-zone.ps1 -Domain uit.ac.ma -DmarcPolicy none
.\scripts\dns-zone.ps1 -Domain uit.ac.ma -DmarcPolicy quarantine -DmarcPct 25

# إثبات DKIM الرياضي (RSA + Ed25519، مع اختبار ذاتي لـRFC 8032)
.\scripts\dkim-proof.ps1 -Domain uit.ac.ma
.\scripts\dkim-proof.ps1 -Domain tira.test
```

`dkim-proof.ps1` لا «يثق» بالمحرك: يفك توقيع RSA، ويتحقق من Ed25519 بمُحقِّق
C# مضمّن يُختبر أولاً على متجهات RFC 8032 ويرفض توقيعاً مُعدَّلاً. وأهم ثابت
اكتُشف أثناء ذلك: RFC 6376 §3.7 يقتضي تجزئة حقل `DKIM-Signature` **بلا CRLF
نهائي**، بينما كل حقول `h=` تنتهي بـCRLF.

DNS والتحقق الخارجي غير مُثبتين لعدم وجود نطاق عام. التفاصيل الكاملة، وجدول
تدفئة IP، وقرار Rspamd الاختياري، في
[`docs/P3-MAIL-AUTH-PLAN.ar.md`](docs/P3-MAIL-AUTH-PLAN.ar.md).

### Rspamd (اختياري — غير مفعّل)

الافتراضي هو مصفّي Stalwart المدمج + SPF/DKIM/DMARC. طبقة Rspamd موجودة
كإعداد جاهز في `deploy/rspamd/` و`deploy/stalwart/milter.ndjson`، لكنها
**معطّلة** ولا تُشغَّل تلقائياً:

```powershell
.\scripts\milter-apply.ps1              # يطبّق الخطة معطّلة
.\scripts\milter-apply.ps1 -Enable      # يرفض إن لم يكن 11332 حياً
.\scripts\milter-apply.ps1 -Disable
```

`MtaMilter` مجموعة وليس singleton، وصيغتها مُثبتة بـ`snapshot` على 1.0.2:
`matchOn:["host"]`، والمفتاح `mtamilter-rspamd`. المراحل المشتركة
`connect, mail, rcpt, data` — لا `data` وحدها، وإلا فقد Rspamd عنوان العميل
والمُرسل الظرفي وصار فحص SPF بلا معنى.

### إعداد Windows (مرة واحدة، مُختبَر)

هذا الجهاز لا يحتوي على Visual Studio، أي لا يوجد `link.exe`، لذلك يُستخدم
toolchain GNU. لكن toolchain GNU وحده **لا يكفي للربط**: مكوّن `rust-mingw`
المرفق مع rustup لا يحتوي `as.exe`، و`dlltool.exe` المرفق يفشل بـ
`dlltool could not create import library`. الحل هو MinGW-w64 كامل:

```powershell
# 1) toolchain
rustup toolchain install stable-x86_64-pc-windows-gnu --profile minimal
rustup default stable-x86_64-pc-windows-gnu
rustup component add rustfmt clippy --toolchain stable-x86_64-pc-windows-gnu

# 2) MinGW-w64 (يوفر gcc + as + dlltool + ld)
winget install --id BrechtSanders.WinLibs.POSIX.UCRT -e
# إن فشل winget (يتوقف أحياناً عند التنزيل)، نزّل الأرشيف يدوياً من
# https://github.com/brechtsanders/winlibs_mingw/releases وافكّه في C:\tools\winlibs
```

`scripts/dev.ps1` يضيف مجلد `bin` تلقائياً إلى `PATH`. إن رغبت بالعمل يدوياً:

```powershell
$env:Path = "C:\tools\winlibs\bin;$env:Path"
cargo test --workspace
```

> **لماذا لا يوجد `rust-toolchain.toml`؟** لأن `channel = "stable"` داخل الملف
> يُجبر rustup على استخدام مضيف الجهاز (`x86_64-pc-windows-msvc`) ويتجاهل
> الافتراضي GNU، فيفشل البناء بـ `linker link.exe not found`. حُذف الملف عن
> قصد، والاعتماد على `rustup default` + `scripts/dev.ps1`.

## التحقق النهائي (15 نوفمبر 2026)

| البوابة | الأمر | النتيجة |
|---|---|---|
| التنسيق | `cargo fmt --all -- --check` | ✅ exit `0` |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 0 تحذير |
| البناء | `cargo check --workspace --all-targets` | ✅ exit `0` |
| الاختبارات | `.\scripts\dev.ps1 test-db` (`TIRA_REQUIRE_DATABASE=1`) | ✅ **250/250** (48 على PostgreSQL) |
| المحرك | `scripts/{imap,smtp,jmap,bridge}-proof.ps1` | ✅ **PASS ×4** |
| النسخ | `backup.ps1` ثم `restore-drill.ps1` (عادي + مشفَّر) | ✅ **DRILL PASSED ×2** |
| المصادقة | `tira-api` حي: `/` 200، بلا رمز 401، رمز صالح 200، مستأجر آخر 404، بعد الإبطال 401 | ✅ |
| DKIM | `scripts/dkim-proof.ps1` على `tira.test` و`uit.ac.ma` | ✅ 4 مفاتيح (RSA×2 + Ed25519×2) محقَّقة |
| DNS | `scripts/dns-zone.ps1 -Strict` | ✅ 9 فحوص و26 سجلاً لـ`tira.test`، و8 و18 لـ`uit.ac.ma` |
| Milter | `scripts/milter-apply.ps1 -Disable` ثم `-Enable -Force` | ✅ upsert + قراءة عكسية |

آخر إثبات للجسر: رسالة `4b44919a2400201` من `youssef@uit.ac.ma` إلى
`aicha@tira.test`، قُرئت في `INBOX`. الواردة على المنفذ 25 ما زالت تُصنَّف
`Junk Mail` قبل P3 — متوقع، وليس عطلاً.

> **درس SAC (تحديث):** التوقيع وحده لا يكفي. SAC يخزّن الحكم **بصمة الملف**؛
> ملف بتوقيع `Valid` رُفض فعلاً. الحل في `scripts/sign-dev.ps1 -Force`:
> يزيل جدول شهادة PE، يضيف 32 بايت overlay عشوائية (لا يقرأها المُحمّل)، ثم
> يوقّع — فتصبح البصمة جديدة ويُقبل الملف. استخدمه على الملف المرفوض فقط.

## التحقق النهائي (17 نوفمبر 2026)

هذا القسم يُضاف بعد إضافة **عقد الإرسال الآلي** (`POST /api/v1/outbound`،
الترحيل `0010_outbound_api.sql`). الجدول أعلاه (15 نوفمبر) يبقى سجلًا مؤرَّخًا.

| البوابة | الأمر | النتيجة |
|---|---|---|
| التنسيق | `cargo fmt --all -- --check` | ✅ exit `0` |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ 0 تحذير |
| البناء | `cargo check --workspace --all-targets` | ✅ exit `0` |
| الاختبارات | `.\scripts\dev.ps1 test-db` (`TIRA_REQUIRE_DATABASE=1`) | ✅ **276/276** (56 على PostgreSQL)، 0 failed، 0 ignored، 33 هدفًا، exit `0` |
| العقد | `cargo test -p tira-api --test outbound` | ✅ **8/8** على PostgreSQL حقيقي |

الفرق عن 15 نوفمبر: **+20 اختبارًا** (8 تحقق وحدات في `send.rs`، 4 ترويسات في
`client.rs`، 8 تكامل في `tests/outbound.rs`)، و**+1 ترحيل** (10 بدل 9)،
و**+1 مسار HTTP** (24 بدل 23)، و**575** سطر SQL غير فارغ (كان 540)،
و**18,843** سطر Rust غير فارغ في 56 ملفًا (كان 17,554 في 55).

> **تحديث (17 نوفمبر، بعد §9.43):** أُعيد قياس أرقام هذا القسم بعد إصلاح
> `MIME-Version`/`Content-Type`. الزيادة `+2` اختبارًا و`+99` سطر Rust هي اختبارا
> الترميز وحاجز الرؤوس المحجوزة في `crates/tira-smtp/src/client.rs`. جدول 15 نوفمبر
> في القسم السابق بقي كما هو؛ الجدول أعلاه هو الحالة المقيسة الحالية.

> **تحديث (17 نوفمبر، بعد §9.46):** أُعيد القياس بعد إصلاحات الواجهة الثلاثة،
> وتصحيح حاجز إدارة `PATCH /reports/{id}/status` في Jami3ati، وإصلاح
> `<textarea>` غير المُغلق في شاشة التحرير (كان يبتلع زر «إرسال»، فلا يمكن
> الإرسال من الواجهة أصلًا). الحالة المقيسة الآن: **276/276** اختبارًا في 33
> هدفًا (كان 271)، منها **56** على PostgreSQL حقيقي (بلا تغيير)، و**19,126**
> سطر Rust غير فارغ في 56 ملفًا (كان 18,843)، و**575** سطر SQL في 10 ترحيلات،
> و**24** مسار HTTP، و**8** crates. الاختبارات الخمسة الجديدة: 3 في
> `tira-frontend` (لافتة الإرسال، سطر التذييل الصادق، تسمية سجلات النطاق)، و2 في
> `tira-api` (ربط الإشعار بالصندوق، وسقف المخزن). الفقرة أعلاه تبقى سجلًّا مؤرَّخًا.

## ما لا يعمل بعد

SPF/DKIM/DMARC **مُولَّدة ومُتحقَّق منها رياضياً محلياً**، لكنها لم تُنشر في
DNS عام ولم يقيّمها مستقبل خارجي (Gmail/Outlook)، لذلك رسالة الاختبار قد
تُصنَّف Junk. Rspamd غير مُشغَّل؛ الافتراضي هو مصفّي Stalwart المدمج. العزل بين
المستأجرين **مفروض في PostgreSQL نفسه** لدور التطبيق `tira_app` (NOLOGIN،
`NOBYPASSRLS`): `ENABLE ROW LEVEL SECURITY` وسياسة لكل مستأجر على `tenants`
و`domains` و`mailboxes` و`messages` و`audit_log`، ومنع كامل على `api_tokens`
و`message_recipients`، والسياق GUC معاملاتي `tira.tenant_id`. يثبته **7**
اختبارات في `crates/tira-store/tests/rls.rs` وفحص `database.rls_isolation` في
`control-plane-proof`؛ ودور المالك (الترحيل والتسليم) يتجاوز RLS بحكم التصميم
ولا يُستخدم في مسار الطلبات. لا واجهة Leptos/WASM تفاعلية (الحالية HTML من
الخادم فقط). الشهادة ذاتية التوقيع، ولا ACME. النسخ الاحتياطي يغطي control
plane + الـblobs، **لا** مخزن Stalwart نفسه. جسر الهوية **مكتوب ومُثبت**، لكن
الترحيل الجماعي لمؤسسة حقيقية لم يُنفَّذ بعد، و`--reconcile` لم يُشغَّل ضد محرك
إنتاج. التفاصيل في `docs/CORRECTIONS.md` و`docs/P3-MAIL-AUTH-PLAN.ar.md`.

## الترخيص

AGPL-3.0-or-later (مؤقت). القرار الكامل — ترخيص Stalwart، ترخيص `tira-*`،
وما يجب نشره قبل أول عميل — في [`docs/DECISIONS.md`](docs/DECISIONS.md) (D2).
