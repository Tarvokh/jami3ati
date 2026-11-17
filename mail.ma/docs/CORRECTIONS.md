# Tira Mail — حالة البناء والتصحيحات

**آخر تحديث:** 6 نوفمبر 2026

هذا الملف يوثّق ما تم تصحيحه، وما لا يزال ناقصاً. أي تعارض بينه وبين
`BARID_STRATEGIC_REPORT.ar.md` أو `BARID_DEEP_PLAN.ar.md` يُحلّ **لصالح هذا الملف**.

---

## 1. حالة البناء

| العنصر | الحالة |
|---|---|
| `cargo check --workspace --all-targets` | ✅ ينجح، 0 تحذير |
| `cargo build --workspace --all-targets` | ✅ ينجح (ربط فعلي، لا check فقط) |
| `cargo test --workspace` | ✅ **87 اختباراً، 0 فشل، 0 متخطّى** — منها **20** على PostgreSQL حقيقي |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ ينجح |
| `cargo fmt --all -- --check` | ✅ نظيف |
| Toolchain | `stable-x86_64-pc-windows-gnu` 1.99.1 |
| الرابط | MinGW-w64 16.2.0 (WinLibs UCRT) — `gcc`/`as`/`dlltool`/`ld` |
| قاعدة البيانات | PostgreSQL **16.10** محمولة في `C:\tools\pgsql`، منفذ **55432** |

### قاعدة البيانات في التطوير

لا يوجد PostgreSQL ولا Docker مثبّت على الجهاز. لذلك تُستخدم نسخة محمولة
(بدون مثبّت، بدون خدمة، بدون صلاحيات مدير) عبر `scripts/pg-dev.ps1`.

**تحذير مقيس — لا تحذفه:** تشغيل PostgreSQL بـ `PATH` الافتراضي كان يفشل بـ
`0xC0000142` (فشل تهيئة DLL) في **أول اتصال عميل** فقط، بينما يبدو الخادم
سليماً. السبب: `C:\Program Files\Eclipse Adoptium\jre-8.0.504.1-hotspot\bin`
يحتوي `msvcp140.dll`/`vcruntime140.dll` (14.40) ويسبق `System32` (14.50) في
`PATH`. الحل المطبَّق: نسخ runtime من `System32` إلى `C:\tools\pgsql\bin`
(مجلد التطبيق يُبحث أولاً)، **و** تنظيف `PATH` داخل السكربت.

**تحذير ثانٍ:** `pg_ctl start | Out-Null` يعلّق إلى الأبد لأن الـ postmaster
الخفي يرث مقبض الأنبوب. لا تُمرّر مخرجات `pg_ctl` عبر pipe.

### لماذا GNU وليس MSVC؟

جهاز التطوير لا يحتوي على Visual Studio Build Tools، أي لا يوجد `link.exe`.
بديل MSVC يتطلب تنزيل عدة غيغابايت. تم اختيار `x86_64-pc-windows-gnu` لأنه
يعمل بدون تثبيت Visual Studio.

### لماذا فشل الربط أول مرة، وكيف أُصلح

هذه ليست تفاصيل ثانوية — كل خطوة منها كانت فشلاً حقيقياً أثناء البناء:

1. **`link.exe` غير موجود** → لا يمكن استخدام هدف MSVC إطلاقاً.
2. **`rust-toolchain.toml` بـ `channel = "stable"`** كان يُجبر rustup على
   اختيار مضيف الجهاز (`msvc`) متجاهلاً الافتراضي GNU. → حُذف الملف.
3. **`ring` (عبر `rustls`) يحتاج `gcc`** لبناء كود C. → استُبدل backend TLS
   إلى `native-tls` (SChannel على Windows)، فاختفى `ring` من شجرة الاعتماد.
4. **`dlltool` المرفق مع rustup يفشل**:
   `dlltool could not create import library ... CreateProcess`.
   السبب: `as.exe` غير موجود، و`dlltool` من MSYS2 يحتاج
   `libintl-8.dll` + `libzstd.dll` + `zlib1.dll` + `libiconv-2.dll`،
   وكلها كانت ناقصة (`STATUS_DLL_NOT_FOUND` = `-1073741515`).
5. **الحل النهائي:** MinGW-w64 كامل من WinLibs في `C:\tools\winlibs\bin`
   (gcc 16.2.0 + binutils 2.47)، ويُضاف إلى `PATH` عبر `scripts/dev.ps1`.

**ملاحظة إنتاجية:** خوادم Linux تستخدم `x86_64-unknown-linux-gnu` بشكل طبيعي،
ولا علاقة لهذا الاختيار بالنشر. كما أن `native-tls` على Linux يستخدم OpenSSL
وليس SChannel — يجب إعادة تقييم `rustls` قبل النشر على الخوادم.

---

## 2. تصحيحات تقنية (الكود السابق لم يكن يُترجم)

| المشكلة | التصحيح |
|---|---|
| `Cargo.toml` يعلن 6 أعضاء غير موجودين | أُنشئت كل الـ crates فعلياً |
| `anyhow` مستخدم وغير معلن | أُزيل؛ الأخطاء عبر `io::Result` و`thiserror` |
| `async-smtp = "0.1"` (2018) | أُزيلت. العميل يستخدم `lettre 0.11` |
| `mail-parser = "0.1"` | `0.11.9` |
| `age = "0.1"` / `chacha20poly1305 = "0.1"` | `chacha20poly1305 0.10.1`؛ `age` أُزيل (لم يُستخدم) |
| `async_smtp::client::Client::new()` | غير موجود أصلاً؛ كود مُتخيَّل |
| `async_smtp::server::{Session, Server}` | غير موجود؛ `async-smtp` مكتبة عميل فقط |
| `Session::new(stream, peer)` | غير موجود |

الخادم الآن مكتوب يدوياً فوق `tokio` (EHLO/MAIL/RCPT/DATA/RSET/NOOP/QUIT)،
والعميل فوق `lettre`.

---

## 3. تصحيحات قانونية ووقائعية

| الادعاء الخاطئ | الصحيح |
|---|---|
| "CNPD" | **CNDP** — Commission Nationale de contrôle de la protection des Données à caractère Personnel |
| "القانون 09-08 (2008/2011)" | صدر في **18 فبراير 2009**؛ المرسوم التطبيقي **2-09-165** |
| "09-08 يتطلب تخزيناً داخل المغرب" | **خطأ.** 09-08 ينظّم المعالجة والنقل خارج المغرب بشروط وترخيص، ولا يفرض الاستضافة المحلية |
| "Gmail غير قانوني للشركات المغربية" | **خطأ قانوني.** لا تبنِ عرضاً تجارياً على هذا الادعاء |
| "UIT = Ibn Taisir — مكنة" | **UIT = جامعة ابن طفيل — القنيطرة**؛ مكناس = جامعة مولاي إسماعيل (UMI) |
| "ANRT: 40 مليون مشترك" | عدد مشتركي الهاتف المحمول؛ لا يصلح كحجم سوق للبريد |
| "Google Workspace = 370–420 درهم/seat" | مشتق من سعر باليورو بدون TVA 20% وبدون هوامش الموزّع. يجب التحقق من فاتورة فعلية |

**التوصية:** أي ادعاء قانوني أو تسعيري في العرض التجاري يجب أن يمر على محام
مغربي متخصص في حماية المعطيات قبل النشر.

---

## 4. تناقض التسعير (لم يُحلّ بعد)

الوثائق تحتوي ثلاثة أسعار مختلفة:

- `BARID_STRATEGIC_REPORT`: **30 درهم/seat**
- `BARID_DEEP_PLAN`: **200 درهم/seat**
- ملخص المحادثة: **50 للـ SME و200 للـ Business**

لا يمكن بناء نموذج مالي على ثلاثة أسعار. هذا قرار مفتوح، وليس قراراً هندسياً.

---

## 5. ما تغطيه الاختبارات فعلياً

| Crate | العدد | ما يُختبر |
|---|---|---|
| `tira-smtp` | 11 | جلسة SMTP كاملة عبر TCP حقيقي (EHLO→MAIL→RCPT→DATA→QUIT)، رفض DATA قبل RCPT، رفض الأوامر غير المدعومة (STARTTLS)، dot-unstuffing، تحليل MIME، بناء رسالة صادرة، رفض عناوين غير صالحة، **رفض مستلم غير موجود عند RCPT** |
| `tira-store` | 8 | 5 على blob store (تجزئة، إزالة تكرار، كشف تلف، مفاتيح خبيثة) + **3 على PostgreSQL حقيقي**: الرسالة تبقى بعد إعادة التشغيل، الحصة تُفرض بـ 552، المستلم المجهول يُرفض بـ 550 |
| `tira-vault` | 5 | roundtrip، عشوائية salt/nonce، رفض كلمة مرور خاطئة، كشف التلاعب، رفض blob مبتور |
| `tira-auth` | 8 | OTP صحيح/خاطئ، استنفاد المحاولات، الانتهاء الزمني، عدم تخزين الكود صريحاً، Argon2id للتجزئة |
| `tira-module` | 3 | التسجيل، رفض التكرار، ترتيب التنفيذ، عزل module يسبّب panic |
| `tira-api` | 8 | `/healthz` (مع `database: disabled`)، `/api/v1/version`، 404، و**5 اختبارات قراءة على قاعدة حقيقية**: قائمة JSON، رسالة خام مطابقة بايتاً ببايت، تهريب موضوع خبيث في HTML، 404/400، و503 بلا قاعدة |
| `tira-frontend` | 2 | escaping لـ HTML، shell بـ RTL |
| **المجموع** | **45** | كلها تمر على Windows/GNU |

الاختبارات ليست شاملة، لكنها **حقيقية**: جلسة SMTP تمر عبر socket TCP فعلي،
والـ vault يُختبر ضد تلاعب بايت فعلي، والـ OTP ضد استنفاد المحاولات، واختبارات
التخزين تعمل على PostgreSQL 16 حقيقي مع إعادة اتصال فعلية.

**اختبارات المحرك (خارج `cargo test`، سكربتات إثبات):**

| السكربت | ما يثبته |
|---|---|
| `scripts/imap-proof.ps1` | STARTTLS → LOGIN → LIST → SELECT → APPEND → FETCH، ثم IMAPS 1993 |
| `scripts/smtp-proof.ps1` | تسليم وارد بلا مصادقة، رفض MAIL قبل AUTH، AUTH PLAIN، تحقق IMAP |
| `scripts/jmap-proof.ps1` | جلسة مُصادَقة، Mailbox/get، Email/query، Email/get، فكّ عربي |

هذه ليست بديلاً عن اختبارات آلية في CI: تعتمد على خادم Stalwart يعمل محلياً
وعلى شهادة ذاتية التوقيع. تُوثّق القدرة، ولا تحرس الانحدار بعد.

**ثغرة أُغلقت:** كانت اختبارات `persist.rs` تُطبع `SKIP` وتبدو **خضراء**.
الآن `TIRA_REQUIRE_DATABASE=1` يجعل غياب قاعدة البيانات فشلاً صريحاً، و
`scripts/dev.ps1 test-db` يضبطه تلقائياً.

---

## 6. ما هو غير موجود — بصراحة

هذه ليست قائمة مهام، بل **حدود المنتج الحالي**. هذا القسم يوثّق حالة ما قبل
P2/P3؛ ما أُغلق بعده موسوم صراحةً، والمرجع الملزم هو §9.6 و§9.7.

1. **IMAP/JMAP موجودان، لكن في المحرك لا في كودنا.** أُثبتا محلياً على
   Stalwart 1.0.2 (تفاصيل §8). ما ينقص ليس البروتوكول، بل **جسر الهوية**:
   الحساب أُنشئ بخطة `stalwart-cli`، وليس من `tira-auth`/`tira-store`، ولا
   توجد مزامنة بين قاعدة `tira_test` ومخزن Stalwart. — **أُغلق في P2:**
   `tira-provision` ينشئ الحساب من `tira-store`، و`--reconcile` يكشف الانحراف.
2. **التخزين موجود للمسار الوارد فقط:** PostgreSQL 16 + blob store على
   القرص. لا S3/MinIO، ولا واجهة لإنشاء الصناديق (تُنشأ بالكود أو SQL).
   القراءة موجودة عبر HTTP/HTML، لكنها **بلا مصادقة ولا صلاحيات**. —
   **أُغلق جزئياً:** المصادقة على كل قراءة نُفِّذت (Bearer/كوكي، 401/404/503)،
   والنسخ/الاسترجاع أُثبتا. يبقى S3 وواجهة إدارة الصناديق.
3. **لا فلترة سبام.** بلا Rspamd/SpamAssassin، أي IP سيُدرج في blocklists خلال أسابيع.
   أثرها ظاهر أصلاً: رسالة اختبار محلية واحدة ذهبت إلى `Junk Mail` لأن
   `tira.test` بلا SPF/DKIM/DMARC. — **أُغلق جزئياً في P3:** DKIM (RSA-2048
   + Ed25519) مولَّد ومُتحقَّق منه رياضياً، ومنطقة DNS تُصدَّر وتُفحص. يبقى
   النشر العام وتشغيل Rspamd؛ الافتراضي هو مصفّي Stalwart المدمج.
4. **STARTTLS/AUTH موجودان في المحرك، لا في `tira-smtp` اليدوي.** على منفذ 25
   أُثبت STARTTLS وقبول بريد وارد بلا مصادقة؛ وعلى 2526 أُثبت رفض `MAIL` قبل
   المصادقة (`503`) ثم `AUTH PLAIN` → `235`. `tira-smtp` نفسه ما زال بلا TLS،
   وهو مرشح للاستبدال لا للتطوير.
5. **لا نسخ احتياطي ولا استعادة.** — **أُغلق:** `backup.ps1` +
   `restore-drill.ps1` نجحا مرتين (عادي ومشفَّر `TIRAS1`). يبقى جدولة cron
   ونسخة خارج الموقع ونسخة متسقة لمخزن المحرك.
6. **لا واجهة Leptos.** الواجهة الحالية HTML مُصيَّر على الخادم فقط.
7. **لا اختبار حمل، ولا مراقبة، ولا نشر.**
8. **تشفير at-rest فقط** بكلمة مرور — ليس E2E، ويتعارض مع البحث من جهة الخادم
   والـ AI وفلترة السبام. يجب حسم هذا التعارض قبل التنفيذ.
9. **modules داخل نفس العملية.** أي panic في module معزول اليوم عبر `catch_unwind`،
   لكن module ثالثاً يجب أن يعمل خارج العملية (out-of-process) قبل السماح به.
10. **عزل المستأجرين غير مفروض في القاعدة.** كل جدول يحمل `tenant_id` تمهيداً
    لـ `ROW LEVEL SECURITY`، لكن السياسات نفسها غير مكتوبة. — **ما زال مفتوحاً؛**
    هو الخطوة التالية الفعلية (§9.7.6 و`DEEP_MAP` §15).

---

## 7. الخطوة التالية المقترحة

> هذا القسم كُتب قبل P2/P3. ما أُنجز منه موسوم أدناه؛ الترتيب الحالي في
> `DEEP_MAP.ar.md` §15.

1. ~~قرار المحرك~~ — **حُسم: Stalwart** (D1 = B). أُثبتت IMAP/SMTP/JMAP محلياً.
2. ~~تخزين حقيقي~~ — **أُنجز للمسار الوارد** (P1).
   ~~قراءة عبر HTTP~~ — **أُنجزت 4 نوفمبر 2026** (JSON + raw + HTML).
   ~~مصادقة مسارات القراءة~~ — **أُنجزت 6 نوفمبر 2026**؛ ~~النسخ الاحتياطي~~ —
   **أُثبت عادي ومشفَّر**. التالي: S3/MinIO وواجهة إدارة الصناديق.
3. ~~جسر الهوية والاستئجار~~ — **أُنجز في P2:** الحساب يُنشأ من `tira-store`
   بخطة idempotent، والحذف/التعطيل ينعكس، وكلمة المرور يملكها Stalwart وحده،
   و`--reconcile` يكشف الانحراف. يبقى الترحيل الجماعي لمؤسسة حقيقية.
4. شهادة حقيقية + DNS عام + Rspamd وقت التشغيل + تدفئة IP.
   **توليد SPF/DKIM/DMARC والتحقق الرياضي منهما أُنجز في P3**؛ الناقص هو
   النشر الخارجي وتشغيل Rspamd.
5. **RLS + تدقيق CNDP** — غير محجوب، وهو الخطوة التالية الفعلية.
6. بعد ذلك فقط: واجهة Leptos، ثم الـ AI.

---

## 8. Stalwart 1.0.2 — ما أُثبت، وما كلّف، وما يقيّدك

### 8.1 الإصدار والرخصة

| العنصر | القيمة |
|---|---|
| `stalwart.exe` | 1.0.2 · 164,546,560 بايت · Windows MSVC |
| `stalwart-cli.exe` | 1.0.14 · 7,052,288 بايت |
| الرخصة | **AGPL-3.0-only OR LicenseRef-SEL** (النسخة المجتمعية AGPL) |
| النشر | `C:\tools\stalwart\` — **خارج المستودع** |

**أثر AGPL (D2):** تقديم Stalwart كخدمة شبكة يفرض إتاحة الكود المصدري
**لنسخة Stalwart المعدَّلة** لمستخدمي الخدمة عبر الشبكة. `tira-*` المنفصلة
تبقى تحت رخصتنا، لكن أي تعديل نُدخله على Stalwart نفسه (patch، module مدمج،
تغيير سلوك) يدخل نطاق الالتزام. هذا قرار تجاري قبل أن يكون هندسياً.

### 8.2 نموذج الإعداد الحقيقي (ليس ما تقوله معظم الأدلة القديمة)

- `config.json` ملف **JSON** وليس TOML، ويحتوي **كائن `DataStore` فقط**:
  `{"@type":"RocksDb","path":"C:/tools/stalwart/run/data"}`.
- كل ما تبقّى (نطاقات، حسابات، مستمعون، شهادات) يُحفظ في **registry داخل
  المخزن**، ويُدار بـ `stalwart-cli apply` عبر خطة NDJSON.
- إن غاب `config.json` يبدأ الخادم في **bootstrap mode**: مخزن مؤقت، مدير
  مؤقت، ومعالج `/admin` على 8080 — لا تصلح للإنتاج.
- **recovery mode** هو المسار الآمن لتعديل الإعداد:
  `STALWART_RECOVERY_MODE=1` + `STALWART_RECOVERY_ADMIN=admin:<pass>`
  → HTTP إداري على 8080 بلا خدمات بريد. بعده يُعاد التشغيل طبيعياً.
- مدير recovery **لا يُنشأ كحساب دائم**؛ لذلك `stalwart-cli` يفشل بـ
  `401` في الوضع الطبيعي. لا تعتمد عليه كمدير إنتاج — أنشئ حساباً بدور
  إداري في الخطة.

### 8.3 الخطة المطبَّقة (5 عمليات)

```json
{"@type":"upsert","object":"Domain","matchOn":["name"],"value":{"dom-tira":{"name":"tira.test",...}}}
{"@type":"upsert","object":"Account","matchOn":["name"],"value":{"usr-yassine":{"@type":"User","name":"yassine","domainId":"#dom-tira","credentials":{"0":{"@type":"Password","secret":"..."}}}}}
{"@type":"create","object":"Certificate","value":{"cert-tira":{"certificate":{"@type":"File","filePath":".../cert.pem"},"privateKey":{"@type":"File","filePath":".../key.pem"}}}}
{"@type":"upsert","object":"NetworkListener","matchOn":["name"],"value":{...}}
{"@type":"update","object":"SystemSettings","value":{"defaultHostname":"mail.tira.test","defaultDomainId":"#dom-tira"}}
```

قواعد الترميز التي كلّفت وقتاً: `Map<T>` يُكتب `{"<item>": true}` لا مصفوفة؛
`List` يُكتب `{"0": {...}}`؛ البروتوكول نص lowercase (`"imap"`, `"smtp"`)؛
`Account` موسوم `@type: User|Group`؛ `Credential` موسوم `@type: Password`.
و`Certificate` **بلا حقل `name`** — يُنشأ بـ `create` بمعرّف يختاره العميل،
ولا يُعاد تشغيل نفس سطر `create` (سيصطدم بمفتاح أساسي).

### 8.4 النتائج المقيسة (5 نوفمبر 2026)

| الاختبار | النتيجة |
|---|---|
| IMAP 1143 | `STARTTLS` مقبول؛ TLS 1.3 / AES-256 / SHA-384؛ LOGIN ناجح؛ 5 مجلدات |
| APPEND | 392 بايت → `0 EXISTS` أصبح `1 EXISTS`؛ `FETCH` أعاد Message-ID والموضوع `=?UTF-8?B?...?=` |
| IMAPS 1993 | TLS مباشر؛ LOGIN ناجح؛ `AUTH=PLAIN LOGIN XOAUTH2 OAUTHBEARER` |
| SMTP 25 | `MAIL/RCPT/DATA` بلا مصادقة → `250 2.0.0 Message queued` |
| SMTP 2526 | `503 5.5.1 You must authenticate first` قبل AUTH، ثم `235` بعد `AUTH PLAIN` |
| التسليم | الرسالتان ظهرتا عبر IMAP؛ إحداهما في `INBOX` والأخرى في `Junk Mail` |
| JMAP 8443 | جلسة مُصادَقة `yassine@tira.test`، 18 قدرة، 5 مجلدات، 3 رسائل، عربي مفكوك |

**الخلل المتوقع:** تصنيف إحدى الرسالتين كـ Junk سببه غياب SPF/DKIM/DMARC
لـ `tira.test`، لا خلل في التسليم. لن يزول قبل P3.

### 8.5 فخّان تقنيان وُجدا بالتجربة

1. **`/.well-known/jmap` يعيد `307` إلى `/jmap/session`.** و`.NET` (ومنه
   `Invoke-RestMethod`) **يحذف ترويسة `Authorization` عند إعادة التوجيه**،
   فتحصل على جلسة مجهولة (`username: ""`, `accounts: {}`) و`200 OK` خادع.
   اطلب `/jmap/session` مباشرة.
2. **`Get-NetTCPConnection` على هذا الجهاز يعطي نتائج كاذبة** لمنافذ Stalwart
   (يقول NOT listening بينما TCP وHTTP يستجيبان). لا تبنِ فحص صحة على هذا
   الـ cmdlet؛ استخدم اتصال TCP/HTTP فعلياً.
3. **`LOGIN` على 1143 قبل TLS مرفوض بـ `LOGINDISABLED`** — هذا سلوك أمني
   صحيح، لا عطل. لا تُضعفه؛ أضف TLS.
4. **PowerShell 5.1:** استدعاء `scriptblock` كـ
   `RemoteCertificateValidationCallback` يفشل على خيط TLS
   (`There is no Runspace available`). الحل: `ICertificatePolicy` مُصرَّف
   بـ `Add-Type`، أو `SslStream` يدوي.

---

## 9. ما أُضيف بعد إثبات الجسر (5 نوفمبر 2026)

### 9.1 حساب الإدارة الدائم

مدير recovery مؤقت. لكي يعمل `stalwart-cli` في الوضع الطبيعي يجب إنشاء حساب
بدور إداري:

- الدور الصحيح: `"roles":{"@type":"Admin"}`.
- `"Administrator"` يفشل بـ `invalidPatch ... Properties: roles`.
- الخطة محفوظة في `deploy/stalwart/admin.ndjson`، والتطبيق في
  `scripts/stalwart-admin.ps1` (يقرأ كلمة المرور من البيئة أو من إدخال آمن،
  ويكتشف `domainId` من المحرك بدل تثبيته).
- **إعادة تشغيل السكربت تدوّر كلمة المرور** ولا تغيّر معرّف الحساب.

الترتيب الكامل لأي تثبيت جديد:

```text
bootstrap mode  ->  recovery mode (8080)  ->  admin.ndjson  ->  normal mode
```

### 9.2 المطابقة (reconcile)

`tira-provision --reconcile` يقارن كل صندوق `active` بلقطة من المحرك:

| الحالة | المعنى | الإجراء مع `--fix` |
|---|---|---|
| `in-sync` | المعرّف في PostgreSQL = معرّف المحرك | تحديث `engine_synced_at` فقط |
| `missing-from-engine` | الصندوق في قاعدة البيانات بلا حساب | تحويل إلى `failed` |
| `engine-id-mismatch` | حساب موجود بمعرّف مختلف | تحويل إلى `failed` مع الاحتفاظ بالمعرّف القديم |
| `domain-missing` | النطاق نفسه غير موجود | تحويل إلى `failed` |

- قراءتان فقط من المحرك (`Domain` ثم `Account`) مهما كان عدد الصناديق، بدل
  عمليتَي CLI لكل صندوق.
- **لا إصلاح تلقائي.** الحساب الغائب قد يعني حذفاً مقصوداً، أو استرجاعاً فاشلاً،
  أو `STALWART_URL` خاطئاً. إعادة الإنشاء بلا قرار بشري تخفي الثلاثة.
- رمز الخروج `2` عند وجود انحراف، ليتمكن cron من التنبيه بينما يبقى `1` للأعطال
  الحقيقية.

### 9.3 Smart App Control يمنع تشغيل أي ملف جديد

بعد تحوّل SAC من وضع التقييم إلى **الفرض** على جهاز التطوير، صار كل ملف `.exe`
مُصرَّف حديثاً يُرفض:

```text
Une stratégie de contrôle d'application a bloqué ce fichier. (os error 4551)
```

الأعراض المضللة: البناء ينجح، ثم `cargo test` يقول
`could not execute process ... (never executed)`، وأحياناً يعمل ملف قديم
ويُرفض ملف جديد. الحل المؤقت: `scripts/sign-dev.ps1` ينشئ شهادة توقيع محلية
(CurrentUser) ويوقّع ملفات `target\debug`.

**تصحيح مُقاس (5 نوفمبر 2026):** التوقيع وحده **لا يكفي**. الملف
`tira_vault-49a6469834f0d3e5.exe` كان `Status: Valid` و`StatusMessage:
Signature vérifiée`، ومع ذلك رُفض بـ `os error 4551` من PowerShell ومن
`cmd` ومن مسار آخر بعد نسخه. ما نجح حينها هو **إعادة البناء فتغيّر تجزئة الملف**،
ثم التوقيع: `tira_vault-b552ee76a284d8f0.exe` مرّ فوراً. أي أن SAC يقرر
بالسمعة/التجزئة، لا بمجرد وجود توقيع.

**التصحيح النهائي (6 نوفمبر 2026):** إعادة البناء ليست مناورة موثوقة، لأن
الربط الحتمي (deterministic linking) قد يُنتج **نفس البصمة**، فيبقى الحكم
المخزَّن كما هو. الحل المُثبت في `scripts/sign-dev.ps1 -Force` يقوم بثلاث
خطوات على الملف المرفوض نفسه:

1. **إزالة جدول شهادة PE** (data directory entry 4): يقرأ `e_lfanew`، يتحقق من
   `PE\0\0`، يحدد `IMAGE_OPTIONAL_HEADER` (PE32 `0x10B` أو PE32+ `0x20B`)،
   يصفّر مدخل الشهادة ثم يقصّ الملف عند `certOffset`. لا يمكن التعديل في ملف
   موقَّع دون هذه الخطوة: `Set-AuthenticodeSignature` يفشل بـ `UnknownError`.
2. **إضافة 32 بايت overlay عشوائية** بعد نهاية الصورة. مُحمّل Windows يتجاهلها
   تماماً، لكن تجزئة الملف تتغيّر — وهذا ما يقرأه SAC.
3. **إعادة التوقيع** بالشهادة المحلية.

أُثبت على `blob-probe.exe` (توقيع `Valid` ثم رُفض)، وعلى
`tira_token-35cf505361ebebe1.exe`، وعلى `blob-63ab5d9a01fbc698.exe`: بعد
`-Force` أصبحت الحالة `Valid` ونفّذت الملفات اختباراتها بنجاح.

**تحذير تشغيلي مُقاس:** `-File` في PowerShell يربط الوسيط الثاني بـ`-Since`
فيفشل بخطأ تحويل تاريخ. لتمرير أكثر من مسار استخدم `-Command`:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command `
  "& '.\scripts\sign-dev.ps1' -Path @('C:\...\a.exe','C:\...\b.exe') -Force"
```

هذا **ليس** حلاً للمنتج: على خادم Linux أو CI عادي لا وجود لـ SAC، ولا حاجة
للتوقيع. إن أُعيد البناء تجب إعادة التوقيع قبل التشغيل، و`-Force` يُستخدم فقط
على الملف الذي رفضه SAC صراحةً.

### 9.4 الأيقونات

انظر [`ICONS.md`](ICONS.md): Lucide ثابتة + line-md متحركة، كلتاهما SVG مضمّن
بلا CDN. مكتبات React/Vue التي اقترحها المستخدم لا يمكن تركيبها في Leptos.

### 9.5 التحقق النهائي (5 نوفمبر 2026) — تاريخي، تجاوزه §9.6

> هذا القسم محفوظ للسياق فقط. الأرقام أدناه كانت صحيحة عند إغلاق P2، لكنها
> **لم تعد الحالة الحالية**. المرجع الملزم هو §9.6. لا تقتبس 69/69 أو 16
> اختباراً قاعدةً من هنا.

خمس بوابات، نُفِّذت بهذا الترتيب على `main` المحلي (لا git بعد):

| البوابة | الأمر | النتيجة |
|---|---|---|
| التنسيق | `cargo fmt --all -- --check` | exit `0`، بلا فروق |
| التحليل الساكن | `cargo clippy --workspace --all-targets -- -D warnings` | exit `0`، 0 تحذير |
| البناء | `cargo check --workspace --all-targets` | exit `0` |
| الاختبارات | `cargo test --workspace` | **69/69**، exit `0` |
| المحرك | `scripts/{imap,smtp,jmap,bridge}-proof.ps1` | **PASS** ×4 |

توزيع الـ69: `health` 4 · `read` 5 (قاعدة) · `tira-auth` 8 · `tira-frontend` 5 ·
`tira-module` 3 · `tira-provision` وحدة 9 · `bridge` 8 (قاعدة) · `tira-smtp` 6 ·
`session` 5 · `tira-store` وحدة 3 · `blob` 5 · `persist` 3 (قاعدة) · `tira-vault` 5.
**16 اختباراً على PostgreSQL حقيقي** (`persist` 3 + `read` 5 + `bridge` 8).

جسر الهوية أُعيد تشغيله في نفس اليوم: `4b44919a2400201`، من
`youssef@uit.ac.ma` إلى `aicha@tira.test`، ثم قراءة `INBOX` عبر IMAP.
رسالة SMTP الواردة ما زالت تُصنَّف `Junk Mail` — هذا متوقع قبل P3، وليس عطلاً.

القرارات التجارية المفتوحة (D2 ترخيص AGPL، D3 التسعير، D5 الاستضافة، D6 SMS)
مسجّلة في [`DECISIONS.md`](DECISIONS.md). لا تُخلط مع حالة الكود أعلاه.

### 9.6 التحقق النهائي (6 نوفمبر 2026) — تاريخي، تجاوزه §9.8

أُعيدت البوابات الخمس كلها بعد إضافة مصادقة القراءة والنسخ الاحتياطي:

| البوابة | الأمر | النتيجة |
|---|---|---|
| التنسيق | `cargo fmt --all -- --check` | exit `0`، بلا فروق |
| التحليل الساكن | `cargo clippy --workspace --all-targets -- -D warnings` | exit `0`، 0 تحذير |
| البناء | `cargo check --workspace --all-targets` | exit `0` |
| الاختبارات | `cargo test --workspace` | **87/87**، exit `0` |
| المحرك | `scripts/{imap,smtp,jmap,bridge}-proof.ps1` | **PASS** ×4 |
| DKIM | `scripts/dkim-proof.ps1 -Domain {uit.ac.ma,tira.test}` | **PASSED** ×2 (RSA + Ed25519) |
| DNS | `scripts/dns-zone.ps1 -Strict` | **PASSED** (7 فحوص/نطاق) |
| Milter | `scripts/milter-apply.ps1` (تعطيل/تفعيل/تعطيل) | **PASS** (upsert + `get`) |

توزيع الـ87: `tira-api` وحدة 4 · `health` 4 · `read` 9 (قاعدة) ·
`tira-auth` 8 · `tira-frontend` 5 · `tira-module` 3 · `tira-provision` وحدة 9 ·
`bridge` 8 (قاعدة) · `tira-smtp` 6 · `session` 5 · `tira-store` وحدة 6 ·
`blob` 5 · `persist` 3 (قاعدة) · `tira-vault` 12.
**20 اختباراً على PostgreSQL حقيقي** (`persist` 3 + `read` 9 + `bridge` 8).

#### 9.6.1 مصادقة مسارات القراءة — إثبات حي

`tira-store/src/token.rs` يصدر رمزاً عشوائياً بصيغة `tira_` + 64 خانة ست عشرية،
ويحفظ **SHA-256 فقط** مع `tenant_id` و`mailbox_scope` اختياري و`expires_at`
اختياري. `tira-api/src/auth.rs` يقبل `Authorization: Bearer` أو كوكي
`tira_token`، ويرفض أي رمز في سلسلة الاستعلام.

نُفِّذت الجلسة على خادم يعمل فعلاً، والنتائج:

| الطلب | الاستجابة |
|---|---|
| `GET /` (الصفحة العامة) | `200` |
| `GET /mail/{id}` بلا رمز | `401` |
| `GET /api/v1/mailboxes/{id}/messages` برمز صالح | `200` |
| نفس الطلب برمز مستأجر آخر | `404` (لا `403`، حتى لا تُجرَّب المعرّفات) |
| نفس الطلب بعد `--revoke` | `401` |

الرمز يُطبع مرة واحدة عند الإصدار؛ القاعدة لا تحتوي النص أبداً. و
`GET /` يبقى demo عاماً ببيانات ثابتة لا يقرأ القاعدة.

#### 9.6.2 النسخ الاحتياطي والاسترجاع — إثبات مزدوج

`tira-vault` صار يحتوي صيغة بثّ ثانية `TIRAS1` بجانب `TIRAV1`:

```text
TIRAS1 | salt[16] | frame_nonce[12] | ChaCha20-Poly1305(frame)
       | frame_index as AAD | ... | EOF
```

- مفتاح Argon2id يُشتق **مرة واحدة لكل ملف**، وكل إطار له nonce جديد.
- ترتيب الإطار يُربط كـAAD، فإعادة ترتيب الإطارات أو حذفها يُكتشف.
- قطع الإطار الأخير لا يُكشف بالبثّ وحده، لذلك يحمل `manifest.json` تجزئة
  SHA-256 للنص الأصلي لكل ملف، ويتحقق منها `restore-drill.ps1`.

نتائج التشغيل الفعلي على `tira_test` (95 رسالة، 4 blobs، 20,090 بايت):

| المسار | النتيجة |
|---|---|
| نسخة عادية `tira-20261106-004605` ← استرجاع | **RESTORE DRILL PASSED** (95 رسالة، 4 blobs، 3 ترحيلات، قاعدة مؤقتة أُسقطت) |
| نسخة مشفَّرة `tira-20261106-004729` ← استرجاع | **RESTORE DRILL PASSED** |
| استبعاد `tmp/` | 0 مدخل `tmp/` في `tar -tf` |
| `-AllowMissingBlobs` مع blob ناقص | تحذير، `blobs.state = "incomplete"`، والدرل يفشل بـ `missing blob in archive: 7f/fc/...` |
| المسار الصارم بلا المفتاح | فشل `backup would be incomplete (missing=1, ...)` وحذف المجلد الناقص |
| فشل قاعدة أثناء التشغيل | `trap` يحذف المجلد، exit `1`، ويحفظ **نص الخطأ الأصلي** بعد إصلاح `throw` ← `throw $failure` |

#### 9.6.3 تصحيح SAC

انظر §9.3: `scripts/sign-dev.ps1 -Force` (إزالة جدول شهادة PE + 32 بايت overlay
عشوائية + إعادة توقيع) هو الحل المُثبت، لا إعادة البناء وحدها.

#### 9.6.4 ما لم يُثبت بعد

- شهادة ACME حقيقية وDNS عام (لا وصول للنادى/المزود من هنا).
- تقييم SPF/DKIM/DMARC من مستقبل خارجي (Gmail/Outlook) وسمعة IP؛ التوليد
  والتحقق الرياضي فقط هما المُثبتان (§9.7).
- Rspamd وقت التشغيل: لا حزمة ولا Redis هنا، والإعدادات مراجعة وثائق فقط.
- عزل RLS على مستوى القاعدة وتدقيق كل قراءة (CNDP). **أُغلق في §9.8**:
  `tira_app` بلا `BYPASSRLS`، 4 اختبارات كسر + probe حيّ داخل الـendpoint.
- نسخة online متسقة من مخزن Stalwart نفسه؛ الأرشيف الحالي يسجّل
  `engine.included = false` ولا يدّعي غير ذلك.
- جدولة النسخ خارج الموقع: السكربتات تعمل يدوياً، بلا cron/systemd timer.

---

### 9.7 P3 — مصادقة البريد (6 نوفمبر 2026)

#### 9.7.1 الثابت الحرج في RFC 6376 §3.7

`dkim-proof.ps1` كان يفشل على توقيع RSA رغم صحة التوقيع. السبب: الخطوة 2 من
§3.7 تتطلب تجزئة حقل `DKIM-Signature` **بلا CRLF نهائي**، بينما كل حقول `h=`
تُجزَّأ **مع** CRLF. السكربت كان يعيد استخدام دالة الرأس العامة فتُضيف CRLF
إلى سطر التوقيع أيضاً.

أُثبت تجريبياً على الرسالة نفسها:

```
بلا CRLF على سطر DKIM-Signature  →  verify = True
مع CRLF على سطر DKIM-Signature   →  verify = False
```

الإصلاح: `TrimEnd("`r","`n")` على سطر التوقيع فقط، لا على بقية الحقول. أي
إعادة كتابة لاحقة يجب أن تحافظ على هذا الفرق.

#### 9.7.2 Ed25519 بدون مكتبة خارجية

PowerShell 5.1/.NET Framework لا يوفّر Ed25519 (Windows CNG يُرجع
`-1073741275`). أُضيف مُحقِّق C# مضمّن `TiraEd25519` (BigInteger + SHA-512)،
ويُشغّل قبل أي حكم اختباراً ذاتياً:

- متجها RFC 8032 رقم 1 و2 → يجب أن ينجحا.
- توقيع مُعدَّل → يجب أن يُرفض.

إن فشل الاختبار الذاتي لا يُصدر السكربت نتيجة. و`ed25519-sha256` (RFC 8463)
يوقّع **تجزئة SHA-256 للنص القانوني** بـPureEdDSA، لا النص الخام.

#### 9.7.3 النتائج الحية

| النطاق | RSA | Ed25519 | المحدِّدات |
|---|---|---|---|
| `uit.ac.ma` | 2048-bit ✅ | 32-byte ✅ | `v1-rsa-20261105` / `v1-ed25519-20261105` |
| `tira.test` | 2048-bit ✅ | 32-byte ✅ | `v1-rsa-20261104` / `v1-ed25519-20261104` |

```
DKIM PROOF PASSED: 1 RSA signature(s) verified, 1 Ed25519 signature(s) verified
```

`c=relaxed/relaxed`، `h=Message-ID:Date:Subject:To:From`، و`p=` في DNS هو
بايتات SPKI نفسها المخزّنة في المحرك. **لكن** التحقق استخدم المفتاح المخزّن،
لا مفتاحاً مسحوباً من DNS عام؛ هذا دليل توقيع لا دليل تسليم خارجي.

#### 9.7.4 منطقة DNS وDMARC المرحلي

`scripts/dns-zone.ps1` يصدّر 24 سجلاً لكل نطاق ويفحص 7 فحوص، منها فك SPKI
وحساب طول معامل RSA فعلياً (1024-bit يُرفض)، والتحقق من أن مفتاح Ed25519
32 بايت.

أُضيف `-DmarcPolicy engine|none|quarantine|reject` مع `-DmarcPct` و
`-DmarcSubdomainPolicy`. الافتراضي `engine` لا يمسّ سجل المحرك. الطرح
المرحلي: `p=none` (14 يوماً) → `quarantine pct=25` (7 أيام) →
`quarantine pct=100` (7 أيام) → `reject`. سبب عدم البدء بـ`reject`: نطاق
جامعي فيه أنظمة قديمة ترسل نيابة عنه، و`reject` قبل حصرها يحذف بريداً مشروعاً.

#### 9.7.5 شكل `MtaMilter` — مُثبت بـsnapshot

`MtaMilter` مجموعة (object) وليس singleton. الشكل الفعلي على 1.0.2:

```json
{"@type":"upsert","object":"MtaMilter","matchOn":["host"],
 "value":{"mtamilter-rspamd":{"host":"127.0.0.1","port":11332,
 "protocolVersion":"v6","stages":{"connect":true,"mail":true,"rcpt":true,"data":true},
 "tempFailOnError":true,"connectTimeout":5000,"commandTimeout":10000,
 "dataTimeout":60000,"maxResponseSize":52428800,"useTls":false,
 "allowInvalidCerts":false,"enable":{"match":{},"else":"false"}}}}
```

`matchOn` هي `host` لأن الكائن لا يحمل `name`. تغيير المضيف يُنشئ كائناً
جديداً ويترك القديم؛ الحذف يدوي. المراحل ليست `data` وحدها: بدون `connect`
لا يُعرف IP العميل، وبدون `mail` لا يُعرف المُرسل الظرفي، فيصبح SPF بلا معنى.

`scripts/milter-apply.ps1` يتحقق من الصيغة بـ`--dry-run`، ويرفض التفعيل إن
لم يكن `127.0.0.1:11332` يقبل TCP، ثم يقرأ الكائن بـ`get` بعد التطبيق:

```
ok  id=jngnhlzgaaqb host=127.0.0.1 port=11332 enable=false tempFailOnError=True
MILTER APPLY: PASS
```

#### 9.7.6 ما لم يُثبت في P3

- DNS عام، وسحب `p=` بـ`dig`، وتقييم Gmail/Outlook، وMTA-STS fetch.
- Rspamd وقت التشغيل، وسلوك Stalwart عند موته في منتصف `DATA`.
- تدفئة IP: لا IP عام ولا سمعة؛ الجدول في
  [`P3-MAIL-AUTH-PLAN.ar.md`](P3-MAIL-AUTH-PLAN.ar.md) خطة لا نتيجة.

---

### 9.8 RLS + endpoint الأدلة الحيّة (7 نوفمبر 2026) — المرجع الملزم

هذا القسم يُلغي ما سبقه حيث يتعارض: §9.6 كان آخر تحقق بخمس بوابات و87 اختباراً،
و§9.6.4 عدّ RLS فجوة. كليهما تجاوزهما هذا القسم.

#### 9.8.1 RLS: من «طبقة استعلام» إلى ضمان في القاعدة

`0004_rls.sql` (191 سطراً) يفرض العزل على قاعدة البيانات نفسها، لا على انضباط
الاستعلام:

- **دوران:** دور الاتصال (المالك/`postgres` في التطوير) يرحّل ويُنشئ؛ ودور
  القراءة `tira_app` بـ`NOLOGIN` و`NOBYPASSRLS`. لا يوجد مسار قراءة يتجاوز
  السياسات.
- **GUC معاملاتي:** `set_config('tira.tenant_id', …, true)` ثم `SET LOCAL ROLE
  tira_app` داخل المعاملة نفسها؛ لا GUC على مستوى الجلسة يتسرّب بين الطلبات.
- **دالتا `SECURITY DEFINER`:** `tira_resolve_token` و`tira_resolve_recipient`
  تحلّان الهوية **قبل** وجود مستأجر، ثم تُعاد السيطرة إلى `tira_app`.
- **جواهر التاج:** `api_tokens` و`message_recipients` ممنوعتان تماماً على
  `tira_app` (لا SELECT ولا INSERT)؛ الوصول إليهما فقط عبر الدوال المحدّدة.

`crates/tira-store/tests/rls.rs` (334 سطراً، 4 اختبارات) لا يختبر «أن الشيء
يعمل» فقط، بل **يكسره عمداً** ليتأكد أن الاختبار يفشل فعلاً عند تعطيل RLS:
صفر ثقة، عزل أفقي بين مستأجرين، تزوير تدقيق، وجواهر التاج. الأربعة خضراء.

الـprobe الحيّ داخل الـendpoint (`crates/tira-store/src/proof.rs`، 154 سطراً)
يعيد نفس الإثبات على القاعدة الحقيقية: معاملة تنزل إلى `tira_app`، تضبط GUC
مستأجراً **عشوائياً لا وجود له**، وتعدّ الصناديق. أي صف ≠ 0 يعني أن RLS لا
يفلتر. كل شيء يُلغى (`ROLLBACK`)؛ الـendpoint **لا يكتب شيئاً ولا يدقّق نفسه**.

#### 9.8.2 عقد `GET /api/v1/control-plane-proof`

الاسم القانوني هو `control-plane-proof`؛ والاسم التوافقي
`/api/v1/sovereignty-proof` يخدم **نفس** الحمولة الحيّة، لا نسخة ثانية. كلاهما
عام بلا مصادقة (لجنة التحكيم)، و`Cache-Control: no-store`.

| الحقل | كيف يُقاس | متى يفشل |
|---|---|---|
| `tenant_isolation` | معاملة حقيقية → `tira_app` + GUC مستأجر عشوائي + `count(*)` | صف ≠ 0، أو الدور ليس `tira_app`، أو `BYPASSRLS=true` → `200 status=fail` |
| `crown_jewels` | `has_table_privilege` على `api_tokens` و`message_recipients` | أي SELECT/INSERT → `fail` |
| `audit.insert_probe` | INSERT لصف تدقيق داخل معاملة **تُلغى** | رفض → `fail`؛ لا مستأجر → `warn` |
| `dkim` | فك SPKI DER وحساب بتات معامل RSA؛ طول مفتاح Ed25519 بالبايت | RSA < 2048 أو Ed25519 ≠ 32 → `fail` |
| `dmarc` | تحليل سجل `_dmarc` من ملف المنطقة، لا من `artifact.dmarcPolicy` | `p=none` → `warn` (مراقبة، لا إنفاذ) |
| `spf` / `mx` / `mta_sts` / `tls_rpt` | قراءة السجلات من المنطقة نفسها | غياب SPF/MX → `fail`؛ غياب STS/TLS-RPT → `warn` |
| `no_external_assets` | فحص HTML المُصيَّر؛ `xmlns` يُستثنى لأنه اسم لا طلب | أي `src/href/url/@import` خارجي → `fail` |
| `artifact` | `sha256` للبايتات المخدومة + `generatedAt` + `age_seconds` | أقدم من 7 أيام → `warn` (`stale`) |
| `control_plane.database` | `host_class` مُنقّى (loopback/private_network/named_host/non_loopback) | غير متصل → `503` |
| `scope.does_not_prove` | قائمة صريحة بما لا يثبته الـendpoint | تُقرأ دائماً؛ ليست فحصاً |

**قاعدة الفشل مقصودة:** `503` **فقط** عندما تكون الأدلة غير متاحة (قاعدة غير
متصلة، ملف مفقود، JSON تالف، منطقة غير قابلة للتحليل). إذا وُجدت الأدلة وكان
الضمان مكسوراً فالجواب `200` و`status:"fail"` — لأن إعادة المحاولة لا تصلح
خطأً أمنياً. ولا تُعرض مسارات مطلقة ولا منافذ ولا `DATABASE_URL` ولا اعتمادات؛
الأثر يُعرَّف بـ`sha256` لا بمساره.

`scope.does_not_prove` (8 عناصر): الاختصاص القضائي للاستضافة، الإقامة
الفيزيائية للبيانات، نشر DNS العام، منفذ 25 الصادر، صلاحية شهادة ACME، Rspamd
وقت التشغيل، التسليم لدى المزوّدين الكبار، وتوقيع الـartifact نفسه.

#### 9.8.3 القياس الحيّ (7 نوفمبر 2026)

تشغيل فعلي على `127.0.0.1:8099` مقابل PostgreSQL 16.10 و`deploy/dns` الحقيقي:

```
status=pass   checks=16   fails=0   warns=0
environment=demo-local   database.host_class=loopback   connected=true
tenant_isolation: method=live_probe role=tira_app bypassrls=false random_tenant_rows=0
crown_jewels.deny_all_verified=true
audit.insert_probe=verified   last_event_at=2026-11-07T00:47:59Z   events_last_7d=390
dmarc=reject/final   spf=v=spf1 mx -all   mx=10 mail.tira.test.
dkim: v1-ed25519-20261104 [ed25519, bytes=32, verified] · v1-rsa-20261104 [rsa, bits=2048, verified]
artifact.sha256=5a66c0259b7009d1…   age_seconds=26168   stale=false
no_external_assets: bytes=15955   namespace_uris_ignored=16   external_references=0   verified=true
```

هذه الأرقام من الاستجابة نفسها، لا من اختبار وحدة. ملاحظة مهمة: `dmarcPolicy`
في ملف الـartifact هو **معامل السكربت**، وليس السياسة الفعلية؛ الـendpoint
يقرأ `p=` من سجل `_dmarc` في المنطقة. الفرق مقصود ومُختبَر.

#### 9.8.4 الخطأ الجذري: BOM من PowerShell 5.1

أول تشغيل حيّ أعاد `503` مع `DNS artifact is not valid JSON` رغم أن الملف يبدو
سليماً. السبب: `Set-Content -Encoding UTF8` في PowerShell 5.1 يكتب **UTF-8
with BOM** (`EF BB BF`)، و`serde_json` يعتبر البايتات الثلاثة بيانات لا
مسافات، فيرفض الملف. الأمر نفسه أخفى أول سجل في المنطقة عند التحليل النصي.

الإصلاح من الجهتين:

1. **القارئ:** `strip_bom()` + `trim_start_matches('\u{feff}')` قبل التحليل.
   الـhash ما زال يغطي **البايتات الخام** كما هي على القرص (بالـBOM)، لأن
   الـhash يشهد على الملف لا على تفسيره.
2. **المولّد:** `scripts/dns-zone.ps1` صار يكتب
   `System.Text.UTF8Encoding($false)` مع `[IO.File]::WriteAllText`، فلا يُنتج
   BOM من الأصل.

الدرس: أدوات Windows تُنتج ترميزات صحيحة لكن غير محمولة. أي قارئ إنتاجي يجب
أن يتسامح مع BOM، وأي كاتب يجب ألا يُنتجه — والاثنان معاً، لا أحدهما.

#### 9.8.5 خطأ ثانٍ: عدّ المرجع الخارجي مرتين

`@import url("https://…")` كان يُطابق نمطي `@import` و`url("http`، فيُعدّ
مرجعاً واحداً مرتين. الإصلاح: كل تطابق يُطبَّع إلى **موضع بداية `http`** الذي
يشير إليه، ثم تُزال التكرارات بموضع البداية. النتيجة تقيس المراجع الفعلية لا
عدد أنماط المطابقة. اختبار الفحص الخارجي يثبت الآن 3 مراجع لا 4.

#### 9.8.6 البوابات الخمس — 109/109

| البوابة | الأمر | النتيجة |
|---|---|---|
| التنسيق | `cargo fmt --all -- --check` | exit `0`، بلا فروق |
| التحليل الساكن | `cargo clippy --workspace --all-targets -- -D warnings` | exit `0`، 0 تحذير |
| البناء | `cargo check --workspace --all-targets` | exit `0` |
| بناء الاختبارات | `cargo test --workspace --all-targets --no-run` | exit `0` |
| الاختبارات | `cargo test --workspace --no-fail-fast` مع DB | **109/109**، exit `0` |

التوزيع: `tira-api` وحدة 16 (منها 12 للأدلة) · `health` 4 · `read` 9 (قاعدة) ·
`bridge` 8 (قاعدة) · `tira-auth` 8 · `session` 5 · `tira-frontend` 5 ·
`tira-store` وحدة 6 · `blob` 5 · `persist` 3 (قاعدة) · `rls` 4 (قاعدة) ·
`proof` تكامل 6 (قاعدة) · `tira-vault` 12 · `tira-module` 3 ·
`tira-provision` 9 · `tira-smtp` 6 · `tira-seal` 3 · إلخ.
**30 اختباراً على PostgreSQL حقيقي**: `persist` 3 + `read` 9 + `bridge` 8 +
`rls` 4 + `proof` 6.

#### 9.8.7 ملاحظة تشغيلية عن SAC (تكملة §9.3)

التوقيع الذاتي **لم يعد كافياً**: شهادة `CN=Tira Dev Builds` صالحة وموثوقة في
`TrustedPublisher` و`Root`، مع Timestamp من DigiCert، ومع ذلك حجب SAC الملف
(`os error 4551`). ما يعمل فعلاً هو تغيير hash الملف: **إزالة جدول شهادة PE**
أو إلحاق 32 بايت overlay عشوائية. الملفات غير الموقّعة تمرّ بعد ذلك. عملياً:
`sign-dev.ps1 -Force` ثم، إن استمر الحجب، إزالة الجدول/إلحاق overlay وإعادة
المحاولة. لا تُفسّر نجاح ملف قديم على أنه دليل على أن التوقيع يعمل.

#### 9.8.8 ما لم يُثبت بعد §9.8

- **DNS عام وشهادة ACME**: لا وصول للنطاق من هذه العقدة؛ كل فحص DNS محلي
  على منطقة مولَّدة، لا على استعلام `dig` خارجي.
- **التسليم الخارجي**: لم تُرسل رسالة عبر منفذ 25 إلى Gmail/Outlook، ولا قياس
  سمعة IP أو تدفئة.
- **Rspamd وقت التشغيل**: الإعدادات مراجعة وثائق فقط، والمحرك كان متوقفاً.
- **Stalwart**: سكربتات `imap/smtp/jmap/bridge` لم تُعَد في هذا التشغيل؛ آخر
  نجاح لها 6 نوفمبر، ولا تُقرأ كنجاح جديد.
- **نسخة مخزن المحرك**: `engine.included = false`؛ لا أمر snapshot online
  موثّق.
- **دين تقني معروف:** اختبار SQL مباشر لسياسات `domains`/`messages`، و
  `has_function_privilege('public', …)` على الدوال، وربط
  `ConnectInfo<SocketAddr>` كي لا يكون `ip` فارغاً في التدقيق، وإعادة
  `x-request-id` في الاستجابة. لا شيء منها يغيّر نتيجة §9.8.
- **Bus Factor = 1**، ولا git repo: نقطة الرجوع الوحيدة نسخة zip محلية.

### 9.9 M1 — إصلاحات بعد الفحص البصري (7 نوفمبر 2026)

أربعة عيوب ظهرت بقياس آلي (Chrome headless + `getBoundingClientRect`)، لا بالعين:

1. **تجاوز أفقي عند `≤1000px`.** السبب مركّب: مسار `.workspace` كان `1fr`
   (حدّه الأدنى `auto`)، و`.folder` يحمل `width:100%` من تخطيط العمود ثم صار
   صفاً أفقياً، فبلغ `scrollWidth` للقائمة 3944px ودفع المسار إلى 676px داخل
   حاوية 497px (RTL يفيض يساراً). الإصلاح: `minmax(0, 1fr)` + `width:auto`
   و`min-width:0` للـsidebar وfolders. عند 512px: `docW == vw` و`overflowX=false`.
2. **تباين 3.49:1.** `--faint` (`#5c6f88`) يُستخدم لنص حقيقي (عناوين الأقسام،
   `.row-addr`، التواقيت) فوق `--surface-2`. رُفع إلى `#788ba2` → **≥4.6:1**،
   وأعاد فحص النص المرئي **0 مخالفة** على الصفحات الثلاث (desktop + 512px).
3. **أزرار «قريبًا» كانت `aria-disabled` فقط.** أُضيف `disabled` الفعلي إلى زر
   الإنشاء وثلاثة أزرار قراءة (ردّ/إعادة توجيه/مرفقات)؛ لا زر غير منفّذ قابل للنقر.
4. **ETag للـCSS كان مشتقاً من إصدار الحزمة فقط** مع `max-age=300`، فتعديل CSS
   لا يغيّر الـvalidator. صار FNV-1a على محتوى `APP_CSS`
   (`"app-css-93bc2f8b9dcacd90"`)، فأي تعديل يبطل الكاش.

البوابات بعد الإصلاح: `fmt` نظيف، `clippy -D warnings` = 0، `cargo check` = 0،
**119/119 اختباراً** (منها 15 للـ`tira-frontend`) على قاعدة حقيقية. حيّاً:
`/assets/app.css` = 23,287 بايت، صفحة الوارد 200 بصفّين حقيقيين، وقراءة الرسالة
200. أُلغي توكن التحقق `7f7d2ae6…` وأعاد 303 إلى `/login`.

**لم يُثبت:** المشاهدة البصرية البشرية. النموذج لا يقبل صوراً؛ اللقطات في
`%TEMP%\opencode\preview\gallery.png` بانتظار مراجعة المالك. أيضاً التوكن
`demo-live` (`f5df76ff…`، بلا انتهاء) ما زال حياً ويجب إلغاؤه أو استبداله
بتوكن قصير العمر.

### 9.10 الخطوة 1 من إعادة التصميم — اتجاه «هدوء» (7 نوفمبر 2026)

أُعيد بناء طبقة العرض بالكامل مع الحفاظ على نفس العقود (Maud + HTMX، عرض من
الخادم، لا CDN، لا توكن في JavaScript):

1. **الخطوط.** ثمانية ملفات `woff2` من IBM Plex Sans Arabic 4.0
   (عربي + لاتيني، أوزان 400/500/600/700، رخصة OFL 1.1) مُضمّنة في الملف
   التنفيذي عبر `include_bytes!`، وتُقدَّم من `GET /assets/fonts/{file}`.
   المطابقة بالاسم الدقيق فقط؛ `../` و`OFL.txt` يعيدان 404. الاستجابة
   `font/woff2` + `Cache-Control: immutable` + ETag مشتق من البايتات.
   `layout` يسبق تحميل وزنين 400 (عربي + لاتيني) بـ`crossorigin`.
2. **الرموز اللونية.** فاتح أولاً (`#F7F8FA` / `#FFFFFF` / `#0B1F33` /
   `#5A6B7D` / `#E2E7EE` / `#007C7C`) وداكن تلقائي عبر
   `prefers-color-scheme` (`#0B1220` / `#121C2B` / `#E9F0FA` / `#5FD3D3`).
   `color-scheme: light dark` + `theme-color` مزدوج.
3. **التخطيط.** شريط علوي 56px، جانبي 232px، صفوف 64px كحد أدنى، عمود قراءة
   `68ch`، أهداف لمس 44px، كسر عند 1100px و760px. حُذف البحث والساعة من
   الشريط العلوي (لم يكنّا يعملان)، وصار للوارد مجلد واحد نشط مع
   `aria-current="page"`، وكل ما لا يدعمه المخطط جُمع في `<details>` مطوي
   بعنوان «غير متاح بعد» مع سبب لكل بند.
4. **الصدق البصري.** «ردّ/إعادة توجيه/مرفقات» لم تعد ثلاثة أزرار متقطّعة، بل
   قسم مطوي بسبب مكتوب. تنزيل الرسالة صار `download="message.eml"`، والخادم
   يرسل `Content-Disposition: attachment; filename="message.eml"`.
5. **شاشة الدخول.** بطاقة واحدة 440px، شرح «أين أجد التوكن؟» قابل للفتح،
   وسطر سفلي يذكر HttpOnly وSHA-256 حرفياً بدل ثلاثة ادعاءات منفصلة.

القياسات الآلية (Chrome headless، DOM حقيقي، بيانات PostgreSQL حقيقية):

| البند | فاتح | داكن | الحد |
|---|---|---|---|
| النص الأساسي | 15.71:1 | 16.32:1 | ≥4.5 |
| النص الثانوي (meta) | 5.15:1 | 7.99:1 | ≥4.5 |
| الموضوع/المقتطف/العنوان في القائمة | 9.58 / 5.48 / 5.25 | — | ≥4.5 |
| الشريط العلوي / الجانبي | 56 / 232px | 56 / 232px | كما هو |
| عمود القراءة | 612px = 68ch | 612px | ≤68ch |
| تمرير أفقي عند 1440/760/390 | لا | لا | لا |
| طلبات خارجية / روابط `#` | 0 / 0 | 0 / 0 | 0 |

البوابات: `fmt` = 0، `check` = 0، `clippy -D warnings` = 0، **176/176 اختباراً**
(+3 لاختبارات الخطوط والـETag) على قاعدة حقيقية. حيّاً: CSS =
**46,441 بايت** ببصمة `"app-css-d7cd0dfb47440eb6"`، الخطوط 200 `font/woff2`،
الوارد 200 بصفّين، الرسالة 200، توكن باطل ← 303 إلى `/login`، وCSP بلا
`unsafe-inline`. اللقطات في `docs/qa-redesign/`.

**لم يُبنَ بعد (معلن، لا مخفي):** العرض الثلاثي مع روابط عميقة (الخطوة 2)،
مبدّل السمة اليدوي، والرد/البحث/المجلدات/النجوم (بانتظار المخطط أو Stalwart).
توكن الفحص المؤقت `qa-redesign-20261107` (`9d7bfd6e…`، يوم واحد) يجب إلغاؤه
بعد اعتماد المالك.

### 9.11 عرض الرسالة — تخطيط قراءة احترافي ونسخة طباعة (8 نوفمبر 2026)

كانت صفحة الرسالة عموداً واحداً مع `<details class="headers">` مكرّر. صارت
تخطيط قراءة من عمودين، مع مسار طباعة مستقل، دون تغيير أي عقد أمني:

1. **التخطيط.** `.reader-grid` = عمود رئيسي 728px + فجوة 16px + بطاقة جانبية
   280px داخل مساحة 1024px. جسم الرسالة 612px = **68ch** بالضبط، وارتفاع
   السطر 28.5px. `.reader-bar` شريط أدوات ثابت (`sticky`) ارتفاعه 62px تحت
   الشريط العلوي، وفيه الرجوع، سياق الموضوع، «نسخة للطباعة»، و«تنزيل .eml».
2. **البطاقة الجانبية.** المرسل، شارة النطاق (`format::domain_part`)، التاريخ
   داخل `<time datetime>`، الحجم، `Message-ID`، معرّف الرسالة، وملاحظة تدقيق.
   حُذف `<details class="headers">` لأن البطاقة أصبحت المصدر الوحيد للبيانات
   الوصفية — لا تكرار ولا مصدران يتناقضان.
3. **نسخة الطباعة.** مسار مستقل
   `GET /mail/{mailbox_id}/message/{message_id}/print` يرسم صفحة كاملة
   (`.print-sheet`) بترويسات كاملة وجسم نصي فقط، ويعمل بلا JavaScript. والطباعة
   من صفحة العرض نفسها تعمل أيضاً عبر `@media print` + `@page`: تُخفي الشريط
   العلوي والجانبي وشريط القراءة والبطاقة والتنبيهات، وتُبقي النص `#111` على
   ورق أبيض.
4. **نفس الصلاحيات حرفياً.** `reading_model()` مشترك بين الصفحتين: RLS + نطاق
   التوكن + فحص الصندوق الشقيق + سطر تدقيق `message.read`. أخطاء التحميل في
   `ReadingError` تُرسم بنفس الردود، والمسار الجديد لا يستطيع توسيع الوصول.
   (هذا الاختبار كشف `clippy::result_large_err` على `Result<_, Response>`؛
   حُلّ بتعداد صغير بدل `Box<Response>` في مسار النجاح.)
5. **الوصول.** أُضيف `Icon::Printer`، وحقل `received_iso` للتاريخ المعياري،
   وأزرار الرد/المرفقات بقيت معطّلة ظاهراً مع سببها.

القياسات الآلية (Chrome headless، DOM حقيقي):

| البند | 1440px | 760px | 390px |
|---|---|---|---|
| عمود القراءة | 612px = 68ch | 612px | 336px |
| البطاقة الجانبية | 280px · sticky | أسفل الرسالة · static | أسفل الرسالة |
| شريط القراءة | 62px · sticky | 116.1px (سطران) | يلتف، بلا قصّ |
| تمرير أفقي | لا | لا | لا |
| طلبات خارجية / روابط `#` | 0 / 0 | 0 / 0 | 0 / 0 |

التباين: نص `15.71:1` فاتح / `14.93:1` داكن؛ بيانات البطاقة `9.58:1` /
`10.96:1`. وفحص `@media print` عبر CDP: `topbar`/`sidebar`/`reader-bar`/
`reader-rail`/التنبيهات = `display:none`، خلفية الصفحة بيضاء، وحدود
`.reader-main` = 0، ونص الجسم `#111`.

البوابات: `fmt` = 0، `check` = 0، `clippy -D warnings` = 0، **176/176 اختباراً**
على PostgreSQL حقيقي. حيّاً: صفحة الرسالة 200 (**10,355 بايت**)، صفحة الطباعة
200 (**2,353 بايت**)، CSS = **46,441 بايت** ببصمة
`"app-css-d7cd0dfb47440eb6"`، و`details class="headers"` غائب. اللقطات الجديدة:
`message-light-1440.png`، `message-dark-1440.png`، `message-light-760.png`،
`message-light-390.png`، `message-print-1440.png`.

نقطة رجوع قبل هذه الوحدة: `TiraMail_Snapshot_Reader_20261108.zip`
(813,316 بايت، SHA-256 `FD54158D1DF77F4650A05E386A1EFA725A9F98D0B0F955ADE9E900BA42E3B669`).

**لم يُبنَ بعد (معلن):** العرض الثلاثي (split view) مع روابط عميقة، وتحليل
ترويسات RFC الخام والمرفقات — قرار نطاق لا نقص. الاعتماد البصري البشري ما زال
معلّقاً؛ النموذج لا يقبل صوراً.

### 9.12 الهوية — دخول بكلمة المرور وملف شخصي بجلسات قابلة للإلغاء (8 نوفمبر 2026)

كان الدخول يعتمد على توكن يُلصق يدوياً، ولا توجد صفحة ملف شخصي. صار الدخول
تقليدياً (بريد + كلمة مرور) مع إبقاء التوكن باباً ثانياً للتهيئة والاستعادة:

1. **بيانات الاعتماد.** جدول `mailbox_credentials(mailbox_id PK, password_hash,
   updated_at)` بسياسة **deny-all** على دور التطبيق `tira_app`؛ لا يقرأ التجزئات
   إلا مسار الخادم عبر مالك الجدول. التجزئة `Argon2id` بمعاملات
   `m=19456 KiB, t=2, p=1` (وهي `argon2::Argon2::default()` الموصى بها في OWASP)،
   وتُنفَّذ داخل `spawn_blocking` كي لا تحجب حلقة `tokio`. العنوان المجهول
   يُتحقَّق ضد تجزئة وهمية لتسوية الزمن (timing equalization).
2. **نموذج الدخول.** `POST /login` بحقل `mode` (`password` افتراضاً أو `token`).
   كلمة المرور المحلية **ليست** كلمة مرور Stalwart/IMAP؛ واجهة الدخول والملف
   الشخصي تقول ذلك صراحةً. سياسة كلمة المرور الجديدة: 12–128 حرفاً، ورفض
   كلمة مرور تساوي العنوان أو الجزء المحلي منه. الدخول نفسه يرفض فقط الفارغ.
3. **الجلسة.** تُنشأ في `api_tokens` بوسم `session:web`، نطاقها صندوق واحد،
   عمرها 7 أيام، وتُسلَّم في كوكي `tira_token` بخصائص `HttpOnly` و
   `SameSite=Strict` (و`Secure` خارج التطوير). لا توكن في JavaScript ولا في
   `localStorage`. إلغاء الجلسة أو انتهاؤها يعيد إلى `/login`.
4. **الملف الشخصي.** `GET /profile` يعرض العنوان الحقيقي، الاسم الظاهر، حالة
   كلمة المرور، والجلسات النشطة مع شارة «الجلسة الحالية». المسارات:
   `POST /profile/display-name`، `POST /profile/password`،
   `POST /profile/sessions/revoke`، `POST /profile/sessions/revoke-others`.
   تغيير كلمة المرور يتطلب كلمة المرور الحالية، ويُلغي **كل** الجلسات الأخرى
   ويُبقي الجلسة الحالية.
5. **الحدود.** 8 إخفاقات / 10 دقائق لكل `email|IP` عند الدخول، ولكل
   `mailbox_id|IP` عند تغيير كلمة المرور. الاستجابة `429` مع
   `Retry-After: 600`. وكل POST مغيّر للحالة يمر عبر فحص
   `Origin` / `Sec-Fetch-Site` إضافةً إلى `SameSite=Strict`.
6. **التدقيق.** أحداث `auth.login` (بـ `method` و`reason`)،
   `profile.display_name`، `profile.password.set`، `profile.password.change`،
   `profile.session.revoke`، `profile.sessions.revoke_others` — تشمل المحاولات
   المرفوضة والمحدودة.

التحقق الحيّ (خادم `127.0.0.1:8099`، PostgreSQL حقيقي، بيانات حقيقية):

| الخطوة | الطلب | النتيجة |
|---|---|---|
| ملف شخصي بتوكن QA | `GET /profile` | `200` |
| ضبط كلمة المرور أول مرة | `POST /profile/password` | `303 → /profile?status=password-set` |
| دخول بكلمة مرور خاطئة | `POST /login` | `401` |
| دخول بكلمة مرور صحيحة | `POST /login` | `303 → /mail/3333…` + كوكي `tira_token` |
| حفظ الاسم الظاهر | `POST /profile/display-name` | `303 → status=name-saved`، مطابقة بايتية في PostgreSQL |
| كلمة قصيرة / تأكيد مختلف / حالية خاطئة | `POST /profile/password` | `303` مع `password-short` / `password-mismatch` / `current-wrong` |
| إلغاء جلسة ثانية | `POST /profile/sessions/revoke` | `303 → session-revoked`، والكوكي الملغى → `303 /login` |
| تغيير كلمة المرور | `POST /profile/password` | `303 → password-changed`؛ التوكن القديم `303 /login`، والقديمة `401`، والجديدة `303` |
| تجاوز الحد | 9 محاولات لعنوان مجهول | 8×`401` ثم `429` + `Retry-After: 600` |
| إلغاء كل الجلسات الأخرى | `POST /profile/sessions/revoke-others` | `303 → others-revoked` |
| POST من أصل آخر | `Origin: https://evil.test` أو `Sec-Fetch-Site: cross-site` | `403` |
| POST من نفس الأصل | `Origin: http://127.0.0.1:8099` | `303` |

القياسات الحيّة: `/login` = **4,237 بايت**، `/profile` = **12,122 بايت**،
`app.css` = **46,441 بايت** ببصمة `"app-css-d7cd0dfb47440eb6"`. تباين النص
`15.71:1` فاتح / `16.32:1` داكن، والنص الثانوي `5.15:1` / `7.99:1`؛ ولا تمرير
أفقي عند 1440 أو 390، و0 طلبات خارجية و0 روابط `#`. اللقطات:
`login-password-light.png`، `login-password-dark.png`، `profile-light.png`،
`profile-dark.png`، `profile-mobile-390.png`.

البوابات: `fmt` = 0، `check --workspace` = 0، `clippy --all-targets -D warnings`
= 0، **176/176 اختباراً** على PostgreSQL حقيقي، وبناء `tira-api` ناجح
(`target\debug\tira-api.exe`).

حالة العرض بعد التحقق: كلمة المرور التجريبية مضبوطة ومُدوَّرة بنجاح (لا تُطبع
في الوثائق)؛ الاسم الظاهر `ياسين الإدريسي` مخزّن بايتياً؛ والجلسة النشطة الوحيدة
هي `7cc064d9-165a-4919-bd7b-626c4151ca56` بوسم `session:web`. توكن الفحص
`qa-redesign-20261107` أُلغي تلقائياً عند تدوير كلمة المرور.

**لم يُبنَ بعد (معلن):** مصادقة Stalwart/IMAP بنفس كلمة المرور (Stalwart متوقف)،
استعادة كلمة المرور عبر البريد (تحتاج مسار إرسال)، توقيع البريد والصورة الرمزية
وتفضيلات العرض (P2). دخول التوكن باقٍ عمداً كباب تهيئة/استعادة، لا كبديل مخفي.

نقطة الرجوع قبل هذه الوحدة: `TiraMail_Snapshot_IdentityCode_20261108.zip`
(1,018,369 بايت، SHA-256 `AFCC6F7C2A6FA3ED4BC0E7E6A81703379DBDAB51E0AA88030880AA19D34474B0`).

### 9.13 تصليب التشغيل — رفض CSRF مرئي ومصمّم، وتنظيف التوكنات الميتة (8 نوفمبر 2026)

كشفت الجولة الحيّة على الواجهة ثغرتين تشغيليتين لا تمسّان الحدود الأمنية لكنهما
تمسّان الصدق والصيانة: (أ) حارس CSRF كان يرفض **صامتاً** ويعيد `403` نصّياً
خارج التصميم، فلا يرى المشغّل سبباً في السجل ولا يرى المستخدم صفحة مفهومة؛
(ب) التوكنات والجلسات المنتهية/الملغاة كانت تتراكم إلى الأبد بلا أي تنظيف.
أُصلحت الاثنتان دون إرخاء أي حدّ أمني — نفس الرفض، نفس `403`، فقط عرض صادق
وصيانة دورية:

1. **تسجيل الرفض — لا صمت بعد اليوم.** كل فرع رفض في `is_same_origin` صار يبثّ
   `tracing::warn!` بالسبب: `sec-fetch-site=cross-site`، `missing Host header`،
   `unparseable Origin`، `Origin without authority`، `origin/host mismatch`.
   الحارس لا يغيّر حالة؛ فقط يجعل الرفض مرئياً للمشغّل. **دقّة:** `Origin: null`
   يُحلَّل عبر `http::Uri` كسلطة (authority) اسمها `null`، فيُسجَّل عبر فرع
   `origin/host mismatch` (مُتحقَّق حيّاً)، لا عبر فرع «بلا سلطة»؛ الهدف (ألا
   يكون الرفض صامتاً أبداً) محقَّق في الحالتين.
2. **صفحة 403 مصمّمة بالتفاوض على `Accept`.** `same_origin_guard` صار يفحص ترويسة
   `Accept`: إن احتوت `text/html` (متصفح) أعاد الصفحة العربية المصمّمة
   `tira_frontend::pages::cross_origin_rejected()` (تخطيط `auth`، عنوان فرعي
   «حماية CSRF»، عنوان «أصل الطلب غير موثوق.»، إشعار بأن لا شيء تغيّر، وزر
   «العودة إلى تسجيل الدخول»). ولغير المتصفح (curl/سكربتات) يبقى النص المضغوط
   `text/plain; charset=utf-8` بجسم "cross-origin form submission rejected"
   القابل للـ grep. طبقة `security_headers` هي الخارجية، فتغلّف الـ`403` بسياسة
   CSP الصارمة أيضاً. لا إرخاء أمنياً: نفس الرفض، نفس `403`، فقط عرض صادق.
3. **اختبارات CSRF.** `crates/tira-api/tests/csrf.rs` (4 اختبارات، `tower::oneshot`،
   بلا قاعدة بيانات): أصل غامض ← `403` `text/html` مع CSP وRTL ونسخة الصفحة
   المصمّمة ودون تسريب أي صندوق؛ أصل غامض بلا `Accept: text/html` ← `403`
   `text/plain` بالنص الحرفي؛ `cross-site` ← `403` مهما كان `Accept`؛ وPOST من
   نفس الأصل ← لا يحجبه الحارس (ليس `403`).
4. **تنظيف التوكنات الميتة.** `PgStore::purge_dead_tokens(grace: chrono::Duration)`
   يحذف الصفوف حيث `expires_at < now-grace` **أو** `revoked_at < now-grace`.
   التوكنات الحيّة لا تُمَسّ أبداً. نافذة السماح (grace) تُبقي الميت الحديث فلا
   يُمحى أثر ارتباط/تدقيق فوراً؛ و`audit_log.actor` نصّ (ليس مفتاحاً أجنبياً)،
   فحذف توكن لا يمحو أي دليل تدقيق. يعيد عدد المحذوف.
5. **التنظيف الدوري.** في `tira-api/src/main.rs`، بعد «storage ready»، حلقة
   `tokio::spawn` تنبض كل ساعة (`interval(3600s)`، أول نبضة فورية) وتستدعي
   `purge_dead_tokens(Duration::days(7))`: `Ok(0)` صامت، `Ok(n)` ←
   `info!(removed=n, "dead api tokens purged")`، `Err` ← `warn!`. تعمل فقط عند
   ضبط قاعدة بيانات.
6. **التنظيف اليدوي (CLI).** `tira-token --purge-dead [--grace-days <n>]`
   (افتراض 7). يرفض السماح السالب؛ و`--grace-days` بلا `--purge-dead` يخرج بالرمز 2
   مع الاستعمال. يطبع "purged {n} dead token(s) (grace {d} day(s))".
   و`TIRA_DATABASE_URL` من البيئة فقط.
7. **اختبار التنظيف.** `crates/tira-store/tests/persist.rs::purge_removes_only_dead_tokens_past_the_grace_window`
   (PostgreSQL حقيقي): يصدر توكن حيّاً (+1 يوم)، وميتاً قديماً (−10 أيام)، وميتاً
   حديثاً (−1 ثانية)؛ تنظيف بسماح 7 ← القائمة بنطاق المستأجر تُبقي الحيّ والحديث
   وتُسقط القديم؛ تنظيف بسماح 0 ← الحديث يختفي والحيّ يبقى؛ `resolve_token(الحي)`
   = `Some` والميت = `None`. يستعمل `list_tokens` بنطاق المستأجر للحتمية لأن
   التنظيف **عام** (global).

الملفات المتغيّرة في هذه الوحدة: `crates/tira-api/src/lib.rs` (الحارس + تحذيرات
`is_same_origin` + استيراد `Html/IntoResponse/Response`)،
`crates/tira-frontend/src/pages.rs` (`cross_origin_rejected`)،
`crates/tira-api/tests/csrf.rs` (جديد)، `crates/tira-store/src/token.rs`
(`purge_dead_tokens`)، `crates/tira-api/src/main.rs` (الحلقة الساعية)،
`crates/tira-store/src/bin/tira-token.rs` (الـCLI)، و`crates/tira-store/tests/persist.rs`
(اختبار التنظيف).

التحقق الحيّ (خادم `127.0.0.1:8080`، PostgreSQL حقيقي، سجلّات `data/fix-verify.*`):

| الخطوة | الطلب | النتيجة |
|---|---|---|
| POST بمتصفح مضمّن (`Origin: null`) | `POST /login` بـ `Accept: text/html` | `403` صفحة مصمّمة + CSP، و`WARN … origin/host mismatch origin=null host=127.0.0.1:8080` في السجل |
| POST من عميل غير متصفح | `POST /login` بلا `Accept: text/html` | `403` `text/plain` جسمه "cross-origin form submission rejected" |
| تنظيف يدوي بسماح 0 | `tira-token --purge-dead --grace-days 0` | "purged 1 dead token(s) (grace 0 day(s))" (صف ميت مُدرَج عمداً، ثم نُظّف) |
| تنظيف يدوي بسماح 7 | `tira-token --purge-dead --grace-days 7` | أبقى الميت الحديث ضمن النافذة؛ حذف فقط ما تجاوزها |

رفض `cross-site` ونجاح POST من نفس الأصل مغطّيان في `tests/csrf.rs`؛ والحلقة
الساعية تستدعي نفس `purge_dead_tokens` المُتحقَّق منها حيّاً عبر الـCLI.

البوابات: `fmt` = 0، `check --workspace` = 0، `clippy --all-targets -D warnings`
= 0، **143/143 اختباراً** على PostgreSQL حقيقي (منها 5 جديدة في هذه الوحدة:
4 في `csrf.rs` + 1 تنظيف في `persist.rs`)، وبناء `tira-api` ناجح
(`target\debug\tira-api.exe`).

ملاحظات صادقة: (أ) `Origin: null` يُسجَّل عبر فرع `origin/host mismatch` لا فرع
«بلا سلطة» (دقّة التحليل أعلاه). (ب) اختبار التنظيف في `persist.rs` ينفّذ تنظيفاً
**عاماً** بسماح 0، فمحى أثناء تشغيل الاختبارات تراكم QA الميت في قاعدة التطوير
المشتركة (593 ← 199 صفاً)؛ هذا أثر جانبي على قاعدة التطوير لا على الإنتاج،
و199 المتبقية توكنات حيّة. التحقق اللاحق عبر الـCLI استعمل صفوفاً ميتة مُدرَجة
عمداً ونُظّفت بعده. (ج) الجلسات الصالحة-غير-المستعملة تنتهي طبيعياً خلال 7 أيام؛
التنظيف يزيل فقط ما مات بالفعل وتجاوز السماح، لا الحيّ أبداً. (د) توكن الفحص
`1ef14efc-3876-4d86-9e28-999080d79afd` أُلغي.

**لم يُبنَ بعد (معلن):** مستودع git حقيقي (ما زلنا على لقطات zip، وعامل الحافلة 1)؛
جعل فترة التنظيف والسماح قابلين للضبط عبر البيئة (مضمّنان حالياً: ساعة/7 أيام)؛
مزامنة كلمة مرور Stalwart/IMAP (معلنة من §9.12، وStalwart متوقف).

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_IdentityCode_20261108.zip`
(1,018,369 بايت، SHA-256 `AFCC6F7C2A6FA3ED4BC0E7E6A81703379DBDAB51E0AA88030880AA19D34474B0`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح، مصدر فقط بلا `target/` ولا `data/`):
`TiraMail_Snapshot_OpsHardening_20261108.zip` (1,350,041 بايت، SHA-256
`F535C441857E162C6215C72A8A4066C5F7BC825B844B667DC24B8ED5F793DB19`).

### 9.14 تصليب الأسرار — اعتماد المسؤول عبر البيئة، والخطة عبر stdin (8 نوفمبر 2026)

كشف تدقيق المحوّል (`crates/tira-provision/src/engine.rs`) ثغرتين خفيّتين في
معالجة الأسرار، لا تمسّان حدود المصادقة لكنهما تسرّبان سرّين إلى سطحين مقروءين:
(أ) كلمة مرور **المسؤول** كانت تُمرَّر على وسائط سطر الأوامر (`--password`) فتظهر
في قائمة العمليات لأي مستخدم محلّي؛ (ب) الخطة وفيها كلمة مرور **الصندوق** لمرة
واحدة كانت تُكتب في ملف مؤقّت، وعلى Windows بصلاحيات موروثة لا «للمالك فقط»،
خلافاً لما كانت وثيقة الوحدة تدّعيه («owner-only permissions on Unix» وسكت عن
Windows). أُصلحت الاثنتان **بالإزالة لا بالترقيع**، دون أي تغيير وظيفي: نفس ترميز
NDJSON، نفس تحليل الملخّص، نفس آلة الحالة في `Provisioner`.

1. **الاعتماد عبر البيئة لا argv.** `base_command()` صار يمرّر
   `STALWART_URL/USER/PASSWORD` عبر `.env()` إلى العملية الابنة، ويقرأها
   `stalwart-cli` رسمياً (`--password … [env: STALWART_PASSWORD]`)؛ فأُزيلت
   `--url`/`--user`/`--password` من argv. قائمة الوسائط مقروءة عالمياً
   (`/proc/<pid>/cmdline`، `ps`، عمود Command line في Task Manager، WMI)، أمّا
   بيئة الابن فلا تتعرّض هناك. هذا يطابق مبدأ المشروع نفسه الذي يرفض `--password`
   على argv في `tira-provision.rs` («it leaks in the process list») — كان المحوّል
   يكسر ما يحميه الـCLI، ويطابق نمط `scripts/milter-apply.ps1` (env-auth بلا argv).

2. **الخطة عبر stdin لا ملف.** `apply()` صار يبني الاستدعاء عبر `apply_command()`
   (`apply --stdin --json`)، ويكتب الخطة في أنبوب stdin: `spawn` ← أخذ stdin ←
   `write_all` ← `flush` ← `drop(stdin)` (إشارة EOF ضرورية وإلا انتظر الـCLI
   إلى الأبد) ← `wait_with_output`. لا ملف ⇒ لا صلاحيات خاطئة، لا أثر قرص
   (forensics/swap)، لا نافذة انهيار تترك سرّاً، لا تنظيف best-effort. السلوك
   موحّد على كل المنصّات (لا `#[cfg]` أمني متفرّق). الإزالة أقوى من ترقيع
   Windows ACL الذي يُبقي أثر القرص قائماً.

3. **إزالة سطح القرص بالكامل.** حُذفت `write_private` (فرعا Unix/Windows)، وحقل
   `plan_dir` من `StalwartConfig`، وقراءة `STALWART_PLAN_DIR` من `from_env`،
   واستيراد `Path`/`Uuid` من engine.rs؛ أُضيف `tokio::io::AsyncWriteExt`. ضبط
   clippy (`io_other_error`): `std::io::Error::other` بدل
   `Error::new(ErrorKind::Other, …)`. `to_ndjson`/`parse_summary`/`summarise`
   بلا تغيير، فالسلوك الوظيفي مطابق؛ تغيّر فقط نقل الخطة والمصادقة.

4. **وثيقة صادقة.** استُبدل الادّعاء المضلّل بوثيقة تصف الضمان الفعلي: لا سرّ في
   argv ولا على القرص، على أي نظام تشغيل.

5. **اختباران أمنيان حتميان** (بلا شبكة ولا قاعدة بيانات، عبر `Command::as_std()`
   في tokio 1.54): `admin_credential_never_appears_in_argv` (السرّ ليس في أي وسيط،
   و`--password`/`--user`/`--url` غائبة من argv، و`STALWART_PASSWORD/USER/URL` في
   بيئة الابنة بالقيمة الصحيحة)، و`apply_reads_the_plan_from_stdin_not_a_disk_file`
   (`--stdin` موجود، `--file` غائب، ولا تسريب للسرّ في argv). أي انحدار يعيد السرّ
   إلى argv أو ملفاً إلى apply يكسر الاختبار فوراً — invariant مُختبَر لا نيّة.

الملفات المتغيّرة في هذه الوحدة: `crates/tira-provision/src/engine.rs` (المحوّل +
الاختباران)، `crates/tira-provision/src/bin/tira-provision.rs` (سلسلة `--help`:
إزالة `STALWART_PLAN_DIR`)، `README.md` (إزالة سطر البيئة + ملاحظة env+stdin)،
`docs/SECRET-HARDENING-REPORT.ar.md` (تقرير كامل جديد: ماذا/لماذا/كيف)، وهذا القسم.

التحقق الحيّ (القناة الرسمية، `stalwart-cli 1.0.14`، المحرك متوقّف):

| الخطوة | الأمر/الفحص | النتيجة |
|---|---|---|
| واجهة رسمية | `stalwart-cli apply --help` | `<--file <PATH>\|--stdin>`، `--stdin Read the JSON plan from stdin`، `--password … [env: STALWART_PASSWORD]` («prompted if absent and stdin is a TTY») |
| استدعاء المحوّል بالضبط | `apply --stdin --json` + اعتماد البيئة، خطة domain عبر الأنبوب | قرأ الخطة من stdin ووصل إلى HTTP؛ فحص آلي: `NO_PASSWORD_PROMPT` (اعتماد البيئة مقبول)، `NO_SECRET_IN_OUTPUT` (لا تسريب) |
| سبب الفشل | `HTTP 404` (refresh schema ثم apply)، `EXIT=1` | المحرك لا يخدم واجهة الإدارة على نقطة الفحص (متوقّف) — فشل شبكة/خادم، لا عيب stdin/مصادقة |
| `--help` بعد الإصلاح | `tira-provision.exe --help` | لم يعد يذكر `STALWART_PLAN_DIR` |

البوابات: `fmt` = 0، `check --workspace --all-targets` = 0،
`clippy --workspace --all-targets -D warnings` = 0، **145/145 اختباراً** على
PostgreSQL حقيقي (0 فشل، 0 تخطّي؛ `TIRA_REQUIRE_DATABASE=1` و
`TIRA_TEST_DATABASE_URL=postgres://postgres@127.0.0.1:55432/tira_test`)، منها
11 وحدة في `tira-provision` (بالاثنين الجديدَين) و8 جسر تكاملية في `bridge.rs`
(FakeEngine + PostgreSQL حقيقي، فلم تتأثر بتغيير المحوّል الحقيقي)؛ وبناء
`tira-provision` ناجح (`target\debug\tira-provision.exe`). 145 = 143 السابقة + 2.

ملاحظات صادقة: (أ) **حدّ التحقق:** التطبيق الشامل ضدّ محرك Stalwart **حيّ**
(إنشاء حساب فعلي وملخّص `created/updated`) لم يُختبر لأن المحرك متوقّف ولم تُطلب
تشغيله؛ المُتحقَّق حيّاً هو ابتلاع stdin + قبول اعتماد البيئة + عدم التسريب/الطلب،
والخاصّيات الأمنية مُثبتة حتمياً بالاختبارين، والمنطق الوظيفي (ترميز/تحليل) لم
يتغيّر وقناة `--stdin` رسمية موثّقة. (ب) لا git (عامل الحافلة 1، لقطات zip).
(ج) `uuid` بقي اعتماداً سليماً (مستعمل في service/plan/bin، فلم يصبح ميتاً بإزالة
استيراده من engine.rs). (د) grep أكّد لا مرجع متبقٍّ لـ`plan_dir`/`write_private`/
`STALWART_PLAN_DIR` في الكود؛ و`lib.rs` يعيد تصدير `StalwartConfig` بلا إنشاء
حرفي خارج `from_env` فإزالة الحقل آمنة.

**لم يُبنَ بعد (معلن):** تشغيل محرك Stalwart للتحقق الشامل حيّاً؛ مزامنة كلمة مرور
Stalwart/IMAP (معلنة من §9.12، وStalwart متوقّف)؛ مستودع git حقيقي.

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_OpsHardening_20261108.zip`
(1,350,041 بايت، SHA-256 `F535C441857E162C6215C72A8A4066C5F7BC825B844B667DC24B8ED5F793DB19`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح + التقرير، مصدر فقط بلا `target/` ولا
`data/` ولا `proof_try_*.exe`): `TiraMail_Snapshot_SecretHandling_20261108.zip`
(1,363,175 بايت، SHA-256 `EBB8FF4C119970D83AB7BF5C2610C6FF778BAA2E9CB927501EB470B6504987F7`).

### 9.15 الحركة والتغذية الراجعة والوصولية — إزالة SMIL، مؤشر حيّ، منطقة حيّة (8 نوفمبر 2026)

أغلق اتجاه «هدوء» (§9.10) الشكل الثابت، لكن الفحص البصري السريع لا يكشف ثلاث
فجوات صامتة في مسار العرض: (أ) حركة **لا يحكمها** `prefers-reduced-motion` لأن
عائلة `AnimatedIcon` كانت تستخدم SMIL (`<animate>`/`<animateTransform>`) وهو خارج
ولاية CSS؛ (ب) مؤشر تحميل **ميت** (قواعد `.htmx-indicator` بلا عنصر يحملها، و
`includeIndicatorStyles:false` لأن حقن HTMX لـ`<style>` تكسره CSP)؛ (ج) **لا تغذية
راجعة لقارئ الشاشة** (CSS وحده لا يحرّك منطقة حيّة، فتبديل `display` ليس تغيير
DOM يُعلَن). أُصلحت الثلاث **بالإزالة من المنبع لا بالترقيع**، مع نظام حركة قابل
للتطور. التفاصيل الكاملة في [`MOTION-ACCESSIBILITY-REPORT.ar.md`](MOTION-ACCESSIBILITY-REPORT.ar.md).

1. **إزالة SMIL بالكامل.** حُذفت `AnimatedIcon` (enum + impl + `animated()`) وثوابت
   `LINE_MD_*` السبعة من `icons.rs`؛ بقيت `Icon` (27 شكلاً) و`LUCIDE_*` بايتاً ببايت.
   علامتا التجارية (`components.rs`، `pages.rs`) → `Icon::Mail.svg()`. كل الحركة
   الآن في `app.css` حيث يحكمها تفضيل المستخدم فعلاً. اختباران يقفلان invariant:
   كل أيقونة SVG زخرفية مكتفية (`aria-hidden`، بلا `xlink`)، و**لا SMIL** (لا
   `<animate`/`<animateTransform`/`<set`/`repeatCount`/`<script`). أي انحدار يكسر
   الاختبار فوراً.

2. **نظام حركة بـtokens.** `--ease-out/in-out`، `--dur-1..4` (120/200/320/480ms)،
   `--reveal-y:8px`، `--stagger:55ms` بدل الأرقام السحرية. دخول منسّق
   (`.content > *`، `.auth__inner > *`) بـ`reveal-up` وإزاحة **رأسية** (محايدة في
   RTL) و`fill-mode: backwards` — مقصود ألا يترك transform دائم يكسر `sticky`
   لأبناء `.reader-bar`/`.rail`. تفاعلات دقيقة (`.btn:active`، انتقالات الحقول
   والروابط) و`.row-link:focus-visible` بمساواة hover.

3. **مؤشر حقيقي + منطقة حيّة.** `layout.rs` شحن `div#nav-indicator.htmx-indicator`
   (`aria-hidden=true`، حبّة `fixed` فيها `.spinner` + «جارٍ التحميل…`)، و
   `p#nav-status.sr-only[role=status][aria-live=polite]`، و`body hx-indicator`
   موروث. المؤشر يستريح `opacity:0/visibility:hidden` (لا `display:none`، ليبقى
   الانتقال ناعماً ومحكوماً بتقليل الحركة) ويظهر عند `.htmx-request`.

4. **محسّن وصولي مستضاف ذاتياً (جديد).** `assets/nav-a11y.js` (59 سطراً، IIFE،
   `"use strict"`) يستمع على `document` لأحداث HTMX ويعيد استعلام العناصر كل حدث
   (لأن `body.innerHTML` يُبدَّل عند التعزيز): `beforeRequest`→`aria-busy=true` على
   `#main` + «جارٍ التحميل…»، `afterSettle`→busy=false + `document.title`،
   `responseError`/`sendError`→busy=false + رسالة خطأ. **تحسين تدريجي:** بلا JS
   الروابط تنقّل كامل والصفحات من الخادم؛ المنطقة الحيّة تحسين لا اعتماد. مستضاف
   ذاتياً (لا CDN/مورد خارجي)، وCSP `script-src 'self'` يسمح به بلا `unsafe-inline`.
   ضُمّن في `assets.rs` (`NAV_A11Y_JS` + `NAV_A11Y_ETAG` مشتقّ من المحتوى) وخُدم عبر
   `pages::asset_nav_a11y` (content-type + `max-age=300` + ETag) ومسار في `lib.rs`.

5. **تقليل الحركة يحكم كل شيء.** كتلة `@media (prefers-reduced-motion: reduce)`:
   مدد animation/transition ~0ms، تأخير 0، iteration 1، `scroll-behavior:auto`،
   و`.spinner{animation:none}` (حلقة ثابتة؛ النص والإعلان الحيّ ما زالا يحملان
   الحالة). لا حركة تفلت من التفضيل بعد الآن.

الملفات المتغيّرة (9): `tira-frontend/src/icons.rs`، `components.rs`، `pages.rs`،
`assets/app.css`، `src/layout.rs`، `assets/nav-a11y.js` (جديد)، `src/assets.rs`؛
`tira-api/src/lib.rs` (مسار + وثيقة CSP)، `src/pages.rs` (معالج الأصل)؛ وهذا القسم
و`docs/MOTION-ACCESSIBILITY-REPORT.ar.md` (تقرير كامل جديد).

التحقق الحيّ (خادم `tira-api` على `127.0.0.1:8080`، PostgreSQL حقيقي، متصفح):

| الفحص | النتيجة |
|---|---|
| `/healthz` · `/login` · `/assets/nav-a11y.js` | 200؛ الأخير `application/javascript; charset=utf-8`، `max-age=300`، ETag `"nav-a11y-…"` |
| CSP | `default-src 'self'` … `script-src 'self'` … بلا `unsafe-inline` (لم تتغيّر) |
| console | **لا رسائل** (لا خرق CSP، لا مورد خارجي) |
| السكريبتات | `htmx.min.js` + `nav-a11y.js` فقط (مستضافان ذاتياً) |
| SMIL (`animate/animateTransform/set`) · `[style]` inline | **0 · 0** |
| `#nav-indicator` | `fixed`، يستريح opacity:0/hidden، `aria-hidden=true`؛ عند `.htmx-request` (بعد قمع الانتقال) opacity:1/visible/translateY(0) |
| `#nav-status` | `role=status`، `aria-live=polite`، `sr-only` |
| `.spinner` · دخول المحتوى | `spin 0.7s` · `reveal-up 0.32s backwards` |
| wiring المحسّن | الأحداث الأربعة تبدّل `aria-busy` والنص صحيحاً |
| نموذج الدخول | `method=post hx-boost=false` (يعمل بلا JS) |

البوابات: `fmt --check` = 0، `clippy --workspace --all-targets -D warnings` = 0،
**147/147 اختباراً** على PostgreSQL حقيقي (`127.0.0.1:55432/tira_test`، 0 فشل،
0 تخطّي). 147 = 145 السابقة + 1 (icons) + 1 (assets).

ملاحظات صادقة: (أ) **حدّ التحقق:** سطح المتصفح داخل التطبيق كان مخفياً
(`visibilityState=hidden`) فعلّق rendering الانتقالات؛ القراءة اللحظية لـopacity
بعد إضافة الصنف أعادت 0 (إطار بداية الانتقال)، فتُحقّق من القيمة المستهدفة بقمع
الانتقال عبر CSSOM (`element.style.transition='none'`، مسموح تحت CSP) ومن قواعد
الـstylesheet المطابِقة — فالقواعد صحيحة والصفر أثر توقيت لا عيب. لم تُختبر الحركة
بالعين (لا viewport مرئي للّقطة). (ب) لا git (عامل الحافلة 1، لقطات zip).
(ج) الوحدة **مسار عرض فقط**: لا تمسّ SMTP/IMAP/التخزين/المصادقة؛ فجوات الإطلاق
الخارجية (شهادة إنتاج، DNS عام، سمعة IP، Rspamd) خارج نطاقها ومعلنة في
`DEEP_MAP.ar.md` §5/§14.

**لم يُبنَ بعد (معلن):** لقطة شاشة بصرية للحركة (سطح متصفح مخفي)؛ تشغيل محرك
Stalwart للتحقق الشامل حيّاً؛ مزامنة كلمة مرور Stalwart/IMAP (معلنة من §9.12)؛
مستودع git حقيقي.

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_SecretHandling_20261108.zip`
(1,363,175 بايت، SHA-256 `EBB8FF4C119970D83AB7BF5C2610C6FF778BAA2E9CB927501EB470B6504987F7`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح + التقرير، مصدر فقط بلا `target/` ولا
`data/` ولا `proof_try_*.exe`): `TiraMail_Snapshot_MotionA11y_20261108.zip`
(1,374,275 بايت، SHA-256 `1E719663D3FB3F82D25401D69D00AE465256C452ECB17EAB210A48569E81D122`).

### 9.16 الموجة 1 / وحدة A — تصليب الجلسة والكوكي ورؤوس الأمان في tira-api (8 نوفمبر 2026)

أول وحدة تنفيذية من «إصلاح كل Tira Mail» بعد تفويض المستخدم (موجات مرتّبة
بالخطورة). سبقها تدقيق أمني شامل قراءة-فقط للـcrates الثمانية والبنية التحتية،
سُجّل بدليل `file:line` في [`SECURITY-AUDIT-2026-11-08.ar.md`](SECURITY-AUDIT-2026-11-08.ar.md)
(10 High، ~22 Medium، ~15 Low، وقائمة «أُكّد سليماً لا تُصلح»). هذه الوحدة تغلق
أربعة عيوب مؤكّدة في مسار استجابة HTTP/الجلسة، دون أي تغيير معماري:

1. **H-01 — الكوكي Secure افتراضياً.** كان `Secure` اختياريّاً عبر
   `TIRA_COOKIE_SECURE=1`؛ فبلا تذكّر المشغّل، كوكي 7 أيام يُرسل over plain-HTTP
   ويسرّب اعتماداً طويل الأمد عند أي MITM/مورد http محقون. صار `cookie_secure()`
   يعيد true افتراضياً، والاستثناء صريح للتطوير المحلي فقط عبر
   `TIRA_COOKIE_INSECURE=1`. Builders صارت نقية (`session_cookie(secret, secure)`،
   `clearing_cookie(secure)`) فتُختبر حتمياً. كوكي المسح يحمل نفس الصفات (وإلا
   أبقى المتصفح الأصلي) و`Secure` افتراضياً. لم تُستخدم بادئة `__Host-` عمداً:
   تمنع opt-out التطويري وتفرض تغيير الاسم عبر auth extractor.

2. **H-02 — logout يبطل الجلسة فعلاً.** كان `logout()` يمسح الكوكي فقط؛ صفّ
   `api_tokens` يبقى حيّاً 7 أيام (`SESSION_TTL_DAYS`)، فسرّ مُلتقَط قبل الخروج
   يظل يعمل بينما الواجهة توحي بالانتهاء. صار يأخذ `State + HeaderMap +
   RequestMeta`، يحلّ السرّ عبر `secret_from_headers`، وإن وجده حيّاً يبطله بـ
   `revoke_token` ويسجّل `auth.logout`. يبقى idempotent: بلا store/سرّ/أو سرّ ميت
   يمسح الكوكي ويعيد التوجيه إلى `/login` بلا حلقة ولا panic. فشل التدقيق لا
   يحوّل خروجاً ناجحاً إلى خطأ تخزين (السرّ أُبطل بالفعل).

3. **M-02 — HSTS على كل استجابة.** أُضيف
   `Strict-Transport-Security: max-age=31536000; includeSubDomains` في
   `security_headers`. يُرسل بلا شرط عمداً: per RFC 6797 المتصفح **يجب** أن
   يتجاهله over non-secure transport، فالتطوير المحلي plain-HTTP لا يتأثر،
   والإنتاج behind TLS يثبّت HTTPS لسنة ويوقف SSL-strip قبل أن يتسرّب الكوكي
   (الآن Secure). `preload` محذوف عمداً (التزام منفصل صريح).

4. **M-03 — الصفحات المصادَقة لا تُخزَّن.** `html()` صار يضبط
   `Cache-Control: no-store` + `Vary: Cookie` على كل صفحة SSR (وارد، رسالة،
   ملف شخصي، دخول، أخطاء). `no-store` هو الحدّ؛ `Vary: Cookie` حزام إضافي لأي
   cache يتجاهله، فلا يخدم جهاز مشترك وارد شخص للتالي. الأصول (css/js/fonts)
   وendpoint الأدلة يبنيان رؤوسهما الخاصة فلم تتأثر.

الملفات المتغيّرة: `tira-api/src/lib.rs` (HSTS)، `tira-api/src/pages.rs`
(logout + كوكي + html + 4 اختبارات وحدة)، `tira-api/tests/health.rs` (اختبار
رؤوس)، `tira-api/tests/session.rs` (جديد: 3 اختبارات logout على PostgreSQL
حقيقي)، وهذا القسم. (M-06 endpoint الأدلة وM-07 I/O الحاجب نُقلا إلى موجة 3.)

التحقق الحيّ (خادم `tira-api` على `127.0.0.1:8080` + curl):

| الفحص | النتيجة |
|---|---|
| `GET /login` رؤوس | `strict-transport-security: max-age=31536000; includeSubDomains`، `cache-control: no-store`، `vary: Cookie`، CSP بلا `unsafe-inline`، `x-frame-options: DENY`، `nosniff`، `no-referrer` |
| `POST /logout` (بلا كوكي) | `303 See Other`، `location: /login`، `set-cookie: tira_token=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0; Secure` |
| logout بسرّ حيّ (اختبار DB) | `resolve_token` قبل=Some، بعد=None (الصفّ أُبطل لا الكوكي فقط) |
| logout idempotent | بلا جلسة/بسرّ ميت ⇒ 303 + مسح، بلا panic |

البوابات: `fmt --check` = 0، `clippy --workspace --all-targets -D warnings` = 0،
**155/155 اختباراً** على PostgreSQL حقيقي (`TIRA_REQUIRE_DATABASE=1`،
`127.0.0.1:55432/tira_test`، 0 فشل، 0 تخطّي). 155 = 147 السابقة + 4 (وحدة pages)
+ 1 (رؤوس health) + 3 (session). بناء `tira-api` ناجح.

ملاحظات صادقة: (أ) `TIRA_COOKIE_SECURE` لم يعد مقروءاً؛ لا مرجع خارجي له
(README/سكربتات) فكان قلبه آمناً. (ب) HSTS over HTTP يتجاهله المتصفح، فلا
كسر للتطوير المحلي؛ الحماية الفعلية تتطلب TLS إنتاجيّاً (حدّ خارجي معلن).
(ج) لا git (لقطات zip). (د) M-06/M-07 (الأدلة/الحاجب) لم يُصلحا هنا عمداً —
موجة 3 — لإبقاء الوحدة متماسكة (جلسة/كوكي/رؤوس فقط).

**لم يُبنَ بعد (معلن):** تقييد endpoint الأدلة (M-06)؛ spawn_blocking للI/O
الحاجب (M-07)؛ بقيّة الموجة 1 (وحدات B–E)؛ TLS إنتاجي/DNS عام (خارجي).

نقطة الرجوع قبل هذه الوحدة: `TiraMail_Snapshot_MotionA11y_20261108.zip`
(1,374,275 بايت، SHA-256 `1E719663D3FB3F82D25401D69D00AE465256C452ECB17EAB210A48569E81D122`).
لقطة حالة هذه الوحدة (مصدر فقط بلا `target/` ولا `data/` ولا `proof_try_*.exe`):
`TiraMail_Snapshot_SessionHardening_20261108.zip`
(1,388,919 بايت، SHA-256 `4AED385804EC7BD31636E3F39162DB0CB563D6D88F7C78280F33472502C23A7F`).

### 9.17 الموجة 1 / وحدة B — سلامة التخزين في tira-store (8 نوفمبر 2026)

ثاني وحدة تنفيذية من «إصلاح كل Tira Mail» (موجات مرتّبة بالخطورة). تغلق خمسة
عيوب مؤكّدة في طبقة التخزين، بدليل `file:line` في
[`SECURITY-AUDIT-2026-11-08.ar.md`](SECURITY-AUDIT-2026-11-08.ar.md). لا تغيير
معماريّاً: نفس `PgStore`/`FsBlobStore`، ونفس حدّ RLS، مع ترحيل جديد `0007`
يُضاف ولا يُعدّل ترحيل مطبّق (قاعدة `db.rs`).

1. **H-03 — استنزاف القرص بال blobs يتيمة.** كان `append_message` يكتب الـblob
   (`store.rs:269`) **قبل** فحص الحصة تحت `FOR UPDATE`؛ فكل تسليم لصندوق ممتلئ
   يترك blob على القرص ثم يتراجع الصفّ، ويتكرّر ذلك بلا حدّ حتى يمتلئ الحجم.
   الإصلاح على ثلاث طبقات: (أ) **فحص قبلي** للحصة من `used_bytes/quota_bytes`
   المحسومة قبل `blobs.put`، فيُرفض التسليم قبل كتابة أي بايت (يسدّ متجه الهجوم
   المتكرّر من منبعه). (ب) **كنس mark-and-sweep** دوريّ
   (`purge_unreferenced_blobs`): يعلّم كل `blob_key` مرجعيّ في `messages`، ثم
   يحذف ما على القرص ممّا تعدّى نافذة سماح ولم يُعلَّم. (ج) **تحديث mtime عند
   كل `put`** (حتى المكرّر dedup)، فلا يكون blob تلمسه معاملة قيد الالتزام
   مرشّحاً للكنس. الحذف الفوري عند التراجع **مرفوض عمداً**: بسبب dedup قد تكون
   معاملة أخرى تشير لنفس البايتات، فالكنس المؤجّل بنافذة سماح (24 ساعة في
   `main.rs`) هو الآلية الصحيحة. الفحص الموثوق تحت `FOR UPDATE` باقٍ (الأرقام
   المحسومة قد تكون قديمة تحت التزامن)، والكنس يلتقط أي يتيم متبقٍّ من تسابق.

2. **M-09 — سلامة الـblob وديمومته.** كان `put` يثق بـ`path.exists()` ويعيد
   «مخزّن» دون تحقّق، ويكتب بـ`fs::write` + rename بلا fsync. صار: عند وجود
   الملف يعيد تجزئته، فإن طابق حدّث mtime وأعاده، وإن خالف (bit-rot أو ملف
   فارغ من انهيار قديم) **أعاد كتابة البايتات الصحيحة** بدل تسليم للقمامة؛
   والكتابة صارت `File::create + write_all + sync_all` على الملف المؤقت **قبل**
   rename (فالبايتات ديمومة قبل أن يصير العنوان مرئيّاً)، ثم mزامنة الدليل
   الأب best-effort (تفتح Windows الدليل كملف تفشل، فـNTFS يسجّل النقل أصلاً).

3. **M-08 — مفتاح DKIM ممنوع على دور الويب.** منح `0004` كان
   `SELECT ON domains` على كامل الجدول، بما فيه `dkim_private_enc` (مفتاح
   التوقيع المشفّر). لا مسار قراءة HTTP يختار هذا العمود اليوم (كلّها تضم
   `domains` لأجل `d.name` فقط)، لكن منحاً على كامل الجدول يعني أنّ `SELECT *`
   واحداً مستقبلاً أو معالجاً مخترقاً يسرّب مفتاح توقيع المستأجر. ترحيل `0007`
   يسحب المنح العام ويمنح `SELECT (id, tenant_id, name, created_at)` فقط؛
   `dkim_private_enc` يبقى لدور المالك (تجهيز المحرك)، نفس حدّ الثقة الذي يحمي
   `api_tokens` و`mailbox_credentials`. RLS يرشّح الصفوف، وهذا يرشّح الأعمدة.

4. **M-10 — تدقيق متعدد المستأجرين.** كان التسليم يسجّل صفّ تدقيق واحداً بـ
   `targets[0].tenant_id`؛ فظرف واحد يوزّع على صناديق في مستأجرين مختلفين يترك
   تسليم الآخر بلا أثر (يخالف CNDP 09-08). صار يجمع المستأجرين المتمايزين
   ويسجّل صفّ `message.delivered` **لكل مستأجر**، يشتركون في المرسل ومفتاح
   الـblob لكن كل صفّ موسوم لمستأجره.

5. **M-11 — سلامة الرسالة عبر المستأجرين + اختبار RLS.** كان
   `messages.mailbox_id` و`messages.tenant_id` يشيران باستقلال، فلا شيء في
   المخطّط يمنع صفّاً صندوقه لمستأجر A بينما `tenant_id` يقول B؛ وسياسة RLS على
   `messages` ترشّح بـ`tenant_id`، فصفّ مفكوك التزامن يظهر للمستأجر الخطأ.
   ترحيل `0007` يضيف `UNIQUE (id, tenant_id)` على `mailboxes` ثم مفتاحاً أجنبيّاً
   مركّباً `messages (mailbox_id, tenant_id) → mailboxes (id, tenant_id)`، فتفرض
   قاعدة البيانات التطابق. وأُضيفت اختبارات RLS بلا مُسنَد لـ`domains`
   و`messages` (كان `0004` يختبر `mailboxes/tenants` فقط).

الملفات المتغيّرة: `tira-store/src/blob.rs` (تحقّق-أو-أعد-كتابة + fsync +
mtime + امتداد الواجهة بـ`remove`/`remove_if_modified_before`/
`keys_modified_before`)، `tira-store/src/store.rs` (فحص قبلي + تدقيق لكل
مستأجر + `purge_unreferenced_blobs`)، `tira-store/src/db.rs` (تسجيل `0007`)،
`crates/tira-store/migrations/0007_column_grants.sql` (جديد)،
`tira-api/src/main.rs` (كنس الـblobs في الحلقة الساعيّة إلى جانب كنس التوكنات)،
`tira-store/tests/blob.rs` (+5)، `tira-store/tests/rls.rs` (+3)،
`tira-store/tests/storage_safety.rs` (جديد: +3)، وهذا القسم.

التحقق الحيّ (PostgreSQL حقيقي `127.0.0.1:55432/tira_test`، `psql`):

| الفحص | النتيجة |
|---|---|
| `schema_version` | `0007_column_grants` مطبّق (بعد 0001–0006) |
| `has_column_privilege('tira_app','domains','dkim_private_enc','SELECT')` | **false** (المفتاح ممنوع) |
| `has_column_privilege(... 'name' / 'id' ...)` | **true** (أعمدة القراءة مسموحة) |
| `pg_constraint` | `mailboxes_id_tenant_key` = unique، `messages_mailbox_tenant_fk` = foreign key |
| كنس الـblobs (اختبار DB) | يتيم aged يُحذف، مرجعيّ يبقى ومقروء، يتيم حديث يُستبقى داخل السماح، ثم يُحذف بعد aging؛ كنس ثالث = 0 (idempotent) |
| رفض الحصة (اختبار DB) | `QuotaExceeded` و**0 ملفات محتوى** على القرص (الفحص القبلي منع الكتابة) |
| توزيع multi-tenant (اختبار DB) | صفّ `message.delivered` لكل مستأجر، نفس مفتاح الـblob، `actor` = المرسل |
| RLS بلا مُسنَد (اختبار DB) | `domains`/`messages` لمستأجر A فقط؛ `SELECT dkim_private_enc` و`SELECT *` ⇒ 42501؛ FK مركّب يرفض رسالة cross-tenant ⇒ 23503 |

البوابات: `fmt --all` = 0، `clippy --workspace --all-targets -D warnings` = 0،
**166/166 اختباراً** على PostgreSQL حقيقي (`TIRA_REQUIRE_DATABASE=1`، 0 فشل،
0 تخطّي). 166 = 155 السابقة + 11 جديدة (5 blob + 3 rls + 3 storage_safety).
بناء `tira-api`/`tira-store` ناجح.

ملاحظات صادقة: (أ) نافذة السماح (24 ساعة) **يجب** أن تتجاوز أطول معاملة تسليم؛
`remove_if_modified_before` يعيد فحص mtime فوراً قبل الحذف فيضيّق تسابق الكنس
مع تسليم dedup متأخّر إلى نافذة ميكروثانية، و`get` يعيد التجزئة عند القراءة
فأي فساد متبقٍّ يُكتشف (فشل صريح لا صامت). (ب) `touch` وmزامنة الدليل
best-effort: فشلها لا يُسقط التسليم، وعلى نظام ملفات للقراءة فقط يفشل الحذف
أيضاً فتتوافق حالات الفشل. (ج) ترحيل FK المركّب يفشل بصوت عالٍ إن وُجدت بيانات
قديمة مفكوكة التزامن (سلوك صحيح: يُظهر التعارض بدل إخفائه). (د) لا git
(عامل الحافلة 1، لقطات zip). (هـ) الوحدة **طبقة تخزين فقط**: لا تمسّ
SMTP/المصادقة/العرض؛ فجوات الإطلاق الخارجية (شهادة إنتاج، DNS عام، سمعة IP،
Rspamd) خارج نطاقها ومعلنة في `DEEP_MAP.ar.md` §5/§14.

**لم يُبنَ بعد (معلن):** بقيّة الموجة 1 (وحدات C–E: crypto/المصادقة،
تجهيز المحرك، CI/السكربتات)؛ موجات 2–4؛ TLS إنتاجي/DNS عام (خارجي).

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_SessionHardening_20261108.zip`
(1,388,919 بايت، SHA-256 `4AED385804EC7BD31636E3F39162DB0CB563D6D88F7C78280F33472502C23A7F`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح + الترحيل + الاختبارات، مصدر فقط بلا
`target/` ولا `data/` ولا `proof_try_*.exe`): `TiraMail_Snapshot_StorageSafety_20261108.zip`
(1,402,118 بايت، SHA-256 `BDA19654052321C593A024D9A3CEC68BAA3D5E3850D8C2C421B93764FB94B5F4`).

### 9.18 الموجة 1 / وحدة C — تصليب crypto في tira-auth (8 نوفمبر 2026)

**النطاق:** طبقة المصادقة النقية (`tira-auth`) فقط: تجزئة كلمات المرور
(Argon2id) ورمز الدخول لمرة واحدة (OTP). لا HTTP ولا قاعدة بيانات ولا مزوّد
SMS هنا — سياسة خالصة تُختبر بلا بنية تحتية.

**ما أُصلح ولماذا:**

1. **H-05 — رمز OTP قابل لإعادة الاستخدام داخل مدة صلاحيته.** كان `verify`
   ينقص `attempts_left` عند كل محاولة، لكن عند النجاح يبقى `code_hash`
   سليماً و`attempts_left` موجباً، فيمكن لمن يلتقط الرمز (أو يسابق المستخدم)
   إعادة تقديمه ونجاح التحقق مراراً حتى انتهاء TTL. الإصلاح: عند أول نجاح
   نُصفّر `attempts_left = 0` **ونمسح** `code_hash.clear()`، فيصبح الرمز
   أحادي الاستخدام فعلاً: أي محاولة لاحقة (حتى بالرمز الصحيح) تُرفض فوراً
   لأن المحاولات نفدت والمادة المخزونة أُزيلت. المسح يزيل أيضاً مادة حسّاسة
   من الذاكرة بدل بقائها حتى انتهاء الكائن. الفحص المبكر (`attempts_left == 0`
   أو منتهٍ) يُرجع `false` قبل أي اشتقاق، فلا يُحرق رمزٌ ميّتٌ مواردَ الخادم.

2. **M-01 — عوامل عمل Argon2 غير محدودة عند التحقق (DoS).** مكتبة
   `password-hash` تُنفّذ `PasswordVerifier` لكل `PasswordHasher` عبر
   `verify_password` الذي **يعيد الاشتقاق باستخدام العوامل المضمّنة في
   التجزئة المخزونة** (`Params::try_from(hash)`) لا افتراضيات المتحقّق. أي
   تجزئة خبيثة تحمل `m`/`t`/`p` فلكية (مثلاً m=4294967295) تُجبر الخادم على
   حجز ذاكرة هائلة أو تعليق الخيط ⇒ إنهاك خدمة. الإصلاح: وحدة داخلية جديدة
   `work_factor.rs` تفحص `Params::try_from(&parsed)` وترفض أي قيمة تتجاوز
   السقوف **قبل** الاشتقاق: `m ≤ 65536` KiB، `t ≤ 4`، `p ≤ 4`. تُستدعى في
   `verify_password` وفي `otp::verify` معاً. السقوف تترك هامشاً فوق
   الافتراضيات (m=19456، t=2، p=1) فتبقى التجزئات الشرعية (بل والأقوى ضمن
   الحد) قابلة للتحقق، بينما تُرفض الفلكية بصمت (`false`) دون تسريب سبب.
   الأساس موثّق من مصدر المكتبة فعلاً: `password-hash-0.5.0/src/traits.rs`
   (التنفيذ الشامل يعيد الاشتقاق بعوامل التجزئة) و`argon2-0.5.3/src/params.rs`
   (`TryFrom<&PasswordHash>` + الدوال العامة `m_cost/t_cost/p_cost`).

**الملفات المتغيّرة:**

- `crates/tira-auth/src/work_factor.rs` (جديد): السقوف + `params_are_bounded`
  + 4 اختبارات (الافتراضيات ضمن الحد، القيم عند السقف تُقبل، أي قيمة فوق
  السقف بواحد تُرفض، العوامل الفلكية تُرفض).
- `crates/tira-auth/src/lib.rs`: `mod work_factor;`.
- `crates/tira-auth/src/password.rs`: فحص السقوف قبل الاشتقاق في
  `verify_password` + اختباران (تجزئة مخزونة بعوامل فلكية تُرفض قبل
  الاشتقاق، تجزئة أقوى لكن محدودة تبقى قابلة للتحقق).
- `crates/tira-auth/src/otp.rs`: حرق أحادي الاستخدام عند النجاح + فحص
  السقوف + 4 اختبارات (النجاح أحادي الاستخدام، النجاح يمسح المادة، الرمز
  الخاطئ يحرق محاولة واحدة لا التحدّي كله، رفض العوامل الفلكية).

**البوابات:** `fmt --all` = 0، `clippy --workspace --all-targets -D warnings`
= 0، **176/176 اختباراً** على مستوى الـworkspace مع PostgreSQL حقيقي
(`TIRA_REQUIRE_DATABASE=1`، 0 فشل، 0 تخطّي). 176 = 166 السابقة + 10 جديدة
(4 work_factor + 2 password + 4 otp)؛ العشرة الجديدة اختبارات crypto نقية
بلا قاعدة بيانات، والـ166 السابقة تشمل اختبارات RLS/التخزين الحيّة. بناء
`tira-auth` ناجح.

**ملاحظات صادقة:**

- (أ) `otp` ما زال **كامناً**: لا مستدعٍ له في مسار HTTP بعد (تحقّق حيّ:
  `grep` على `tira-api`/`tira-smtp` = صفر). ربط تسجيل الدخول بالهاتف = M-05،
  وحدة لاحقة منفصلة. الإصلاح يسبق الربط حتى لا يُفعّل مسارٌ معيباً.
  الاختبارات تزرع الحقول الخاصة مباشرة (child module) للتحقق من الحرق
  والمسح دون مزوّد SMS.
- (ب) السقوف (m≤65536/t≤4/p≤4) **اختيار سياسة** لا حدّ المكتبة؛ تترك
  هامشاً معقولاً فوق افتراضيات OWASP وتقبل التجزئات الأقوى ضمنها. لو رُفعت
  الافتراضيات مستقبلاً فوق السقف وجب رفع السقف معها (معلَن هنا).
- (ج) المتحقّق يُرجع `false` للتجزئة المشوّهة وللعوامل الفلكية على حدّ
  سواء، فلا يستطيع المهاجم تمييز الحالتين (لا رسالة ولا توقيت مختلف).
- (د) الاشتقاق ثقيل (Argon2id)؛ المستدعون في `pages.rs` يلفّونه في
  `spawn_blocking` فعلاً (تحقّق حيّ: `verify_password_async` سطر 560،
  `hash_password_async` سطر 566؛ يُستدعيان من الدخول سطر 333 وتغيير كلمة
  المرور 1063/1104/1109) كي لا يحجز خيط tokio. هذه الوحدة لا تغيّر ذلك،
  لكنه قيد تشغيلي معلَن.
- (هـ) لا git (عامل الحافلة 1، لقطات zip). الوحدة **طبقة مصادقة فقط**: لا
  تمسّ SMTP/التخزين/العرض؛ فجوات الإطلاق الخارجية (شهادة إنتاج، DNS عام،
  سمعة IP، Rspamd) خارج نطاقها ومعلنة في `DEEP_MAP.ar.md` §5/§14.

**لم يُبنَ بعد (معلن):** بقيّة الموجة 1 (وحدات D–E: تجهيز المحرك،
CI/السكربتات)؛ موجات 2–4؛ ربط OTP بالهاتف (M-05)؛ TLS إنتاجي/DNS عام (خارجي).

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_StorageSafety_20261108.zip`
(1,402,118 بايت، SHA-256 `BDA19654052321C593A024D9A3CEC68BAA3D5E3850D8C2C421B93764FB94B5F4`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح + الاختبارات، مصدر فقط بلا `target/`
ولا `data/` ولا `proof_try_*.exe`): `TiraMail_Snapshot_AuthCrypto_20261108.zip`
(1,408,568 بايت، SHA-256 `566131D092CFE34CC45964DF1AE661518607D7B369010034943DE7642F4FEACF`).

### 9.19 الموجة 1 / وحدة D — تصليب الأسرار والأخطاء في tira-provision (9 نوفمبر 2026)

**النطاق:** جسر الهوية (`tira-provision`) فقط: ترميز خطة Stalwart (`plan`)،
وتطبيقها وقراءة النتيجة عبر `stalwart-cli` (`engine`)، وسطر الأوامر
(`bin/tira-provision`). لا يمسّ SMTP ولا العرض ولا التخزين؛ `tira-store` يبقى
مصدر الحقيقة لهوية الصندوق، وStalwart لمكان البريد.

**ما أُصلح ولماذا:**

1. **H-06 — تسريب كلمة المرور لمرة واحدة عبر stderr المحرك.** كان `engine.rs`
   يلتقط stderr الطفل الخام (حتى 2000 محرف) ويمرّره كما هو إلى
   `EngineError::Apply/Query{stderr}`، ثم `service.rs` يحوّله إلى نصّ ويخزّنه في
   عمود `engine_error` (PostgreSQL) ويعرضه في الطرفية. لو أعاد المحرك صدى خطةٍ
   مشوّهة (NDJSON يحوي `credentials.0.secret`) لتسرّبت كلمة المرور لمرة واحدة
   إلى السجلّ الدائم. الإصلاح طبقتان: (أ) `redact_stderr` يستبدل كل سرّ معروف
   (من `plan::secret_values` وكلمة مرور المسؤول) استبدالاً حرفياً الأطول أولاً
   بـ `[REDACTED]`، ثم يُسقط أي سطر يحوي مفتاحاً حسّاساً
   (`secret`/`password`/`credentials`/`secret=`/`password=`) إلى `[REDACTED LINE]`
   حتى لو كان السرّ مجهولاً، ثم يحدّ النص إلى 2000 محرف على حدّ محرف سليم.
   (ب) أصناف أخطاء M-12 الجديدة لا تحمل stderr أصلاً (أرقام فقط)، فلا قناة
   تسريب فيها.

2. **M-12 — نجاح صامت مزيف عند غياب الملخّص أو فشل عمليات.** كان `parse_summary`
   يعيد تقريراً مصفّراً عند غياب سطر `summary`، ويتجاهل `failed > 0` إذا كان
   رمز الخروج 0، فيُسجَّل الصندوق `active` وهو لم يُنشأ فعلاً. الإصلاح: صار
   `parse_summary(stdout, expected_lines) -> Result<ApplyReport, EngineError>`
   يفرض وجود `done` وإلا `NoSummary`، ويرفض `failed > 0` بـ `ApplyFailed{failed}`،
   ويتحقق أن `created + updated + destroyed ≥ expected_lines` وإلا
   `IncompleteApply{expected, got}`. كل الأصناف بلا نصّ خام (أرقام فقط).

3. **M-13 — حقن مرشّح عبر `--where`.** كان `lookup_account` يحشر
   `local_part`/`domain` مباشرة في `--where name={…}`، فقيمة خبيثة (مسافة/فاصلة/
   عامل تصفية) تكسر المرشّح أو تغيّر دلالته. الإصلاح: `validate_filter_token`
   يفرض أبجدية آمنة (أرقام وحروف لاتينية صغرى، و`.`/`_`/`+`/`-`، والكبرى مسموحة
   فقط لمعرّف المحرك) وسقوف طول (local ≤ 64، domain ≤ 253، id ≤ 256) **قبل**
   إطلاق الطفل، ويُعاد التحقق من `domain_id` بعد استخراجه من نتيجة المرشّح
   (دفاع عميق). الرفض `InvalidFilterValue` يحدث قبل الإطلاق فلا تصل القيمة
   الخطرة إلى argv المحرك.

4. **M-14 — كلمة مرور قاعدة البيانات في argv.** كان `--database-url` مقبولاً
   على سطر الأوامر فتظهر كلمة المرور في قائمة العمليات. الإصلاح: رُفض
   `--database-url` صراحةً (و`--password` كذلك) برسالة توجيه، وصار المصدر الوحيد
   `TIRA_DATABASE_URL` من البيئة؛ إن غاب فالخطأ `TIRA_DATABASE_URL is not set`.

5. **M-15 — نقطة نهاية غير آمنة للمحرك.** كان `STALWART_INSECURE=1` يعطّل التحقق
   من الشهادة والافتراضي `http://127.0.0.1:8080`؛ على مضيف بعيد يعني ذلك مرور
   كلمة مرور المسؤول وكلمة المرور لمرة واحدة بلا تشفير. الإصلاح:
   `StalwartConfig::validate` و`url_host` يرفضان أي نقطة غير loopback ما لم تكن
   `https://` والتحقق مفعّل (`InsecureEndpoint`)، مع استثناء loopback فقط
   (`localhost`/`127.*`/`::1`) للاستعادة المحلية. يُستدعى `validate` في `from_env`
   وفي بداية `apply`/`query` (دفاع عميق).

**الملفات المتغيّرة:**

- `crates/tira-provision/src/engine.rs` (31125 بايت): خمسة أصناف `EngineError`
  جديدة، و`validate`/`url_host`/`parse_summary`/`validate_filter_token`/
  `redact_stderr`، واستدعاءاتها في `apply`/`query`/`lookup_account`/`from_env`.
  21 اختباراً (منها 16 جديداً).
- `crates/tira-provision/src/plan.rs` (9084 بايت): `secret_values` و
  `collect_secret_values` (يمشيان مشية `mask_value` نفسها). 8 اختبارات (منها
  2 جديدان).
- `crates/tira-provision/src/bin/tira-provision.rs` (9584 بايت): رفض
  `--database-url`/`--password`، وقصر المصدر على البيئة، وتحديث نص المساعدة
  لـ `STALWART_INSECURE`.

**البوابات:** `fmt --all` = 0، `clippy --workspace --all-targets -D warnings`
= 0، **195/195 اختباراً** على مستوى الـworkspace مع PostgreSQL حقيقي
(`TIRA_REQUIRE_DATABASE=1`، 0 فشل، 0 تخطّي). أضافت وحدة D 18 اختباراً في
tira-provision (16 في engine.rs و2 في plan.rs) ليصبح engine.rs 21 وplan.rs 8.
تحقّق حيّ من سطر الأوامر: `--database-url` و`--password` مرفوضان برسالة توجيه.
أُعيدت البوابات من بناء نظيف بعد استعادة المصدر (انظر الملاحظة هـ).

**ملاحظات صادقة:**

- (أ) `redact_stderr` يلتقط الصدى الحرفي للسرّ (المتجه المدقّق: صدى خطة NDJSON)،
  ويُسقط أي سطر يحمل مفتاحاً حسّاساً حتى لو تحوّل السرّ. سرّ محوّل على سطر بلا
  مفتاح قد ينجو نظرياً؛ ليس هذا المتجه المدقّق، لكنه حدّ معلَن.
- (ب) فحص الاكتمال في M-12 يفترض أن كل سطر خطة = عملية upsert واحدة محسوبة.
  المحرك (Stalwart) متوقف فلم يُختبر `apply` حياً ضدّه؛ يُعلَم هذا لأول
  provisioning حيّ.
- (ج) `url_host` مستخرج أدنى؛ عنوان مشوّه يسقط إلى معاملة «بعيد» (fail-safe:
  يرفض إلا https).
- (د) **حادثة عرضية خارج نطاق الوحدة:** أثناء بوابة fmt اكتُشف أن
  `crates/tira-frontend/src/pages.rs` أُتلف خارجياً (دمج خاطئ: تعريف `login`
  مكرّر، محارف مشوّهة، نهايات `\r?`، وافتتاح `mod tests` مفقود، فلا يُترجَم).
  حُفظت النسخة التالفة في `../TiraMail_pages.rs.corrupt-backup-20261109`
  واستُعيد آخر أصل سليم من لقطة AuthCrypto (عودة إلى دخول §9.15 العامل؛ نيّة
  إعادة التصميم محفوظة في النسخة التالفة).
- (هـ) **حادثة حذف المصدر واستعادته (10 نوفمبر 2026):** وُجد `mail.ma/` وقد فقد
  كل مصدره ولم يتبقَّ إلا `target/` (8.4GB). كل اللقطات كانت سليمة في
  `screenshot mail.ma/`. استُعيدت أحدث نسخة `ProvisionHardening` (SHA مطابق
  وفحص تكامل ناجح) إلى `mail.ma/`: 147 ملفاً و8 crates بأحجام مطابقة، ثم حُذف
  `target/` القديم المصرّح به وأُعيد تشغيل PostgreSQL، وأُعيدت البوابات من بناء
  نظيف: fmt=0 و**195/195**. نسخة `mail.ma.rar` لم تُمَس. لا git (عامل الحافلة 1،
  لقطات zip)؛ الحادثة تؤكد قيمة اللقطات المتعددة.
- (و) الوحدة **جسر هوية فقط**: لا تمسّ SMTP/التخزين/العرض؛ فجوات الإطلاق
  الخارجية (شهادة إنتاج، DNS عام، سمعة IP، Rspamd) خارج نطاقها ومعلنة في
  `DEEP_MAP.ar.md` §5/§14.

**لم يُبنَ بعد (معلن):** وحدة E (CI/السكربتات)؛ موجات 2–4؛ ربط OTP بالهاتف
(M-05)؛ TLS إنتاجي/DNS عام (خارجي)؛ أول provisioning حيّ ضد Stalwart.

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_AuthCrypto_20261108.zip`
(1,408,568 بايت، SHA-256 `566131D092CFE34CC45964DF1AE661518607D7B369010034943DE7642F4FEACF`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح والاختبارات، مصدر فقط بلا `target/`
ولا `data/` ولا `proof_try_*.exe`): `TiraMail_Snapshot_ProvisionHardening_20261109.zip`
(1,419,550 بايت، SHA-256 `46681C7DEE3D477EA96AEFEBC563A46469A0C0701B5BE8B6F8D87173BC465B14`).

### 9.20 الموجة 1 / وحدة E — CI وسكريبتات البنية التحتية (10 نوفمبر 2026)

**النطاق:** مسار التكامل المستمر (`.github/workflows/ci.yml`) وسكريبتات PowerShell
التشغيلية (`backup`/`backup-common`/`milter-apply`/`sign-dev`) وقطع DNS المولّدة
(`deploy/dns`). لا يمسّ أي حزمة Rust؛ المصدر البرمجي والسلوك الاختباري بلا تغيير،
والبوابات تؤكد ذلك (195/195 كما هي).

**ما أُصلح ولماذا:**

1. **H-07 — CI أخضر بلا تغطية تخزين.** كان `ci.yml` يشغّل `cargo test --workspace`
   بلا خدمة PostgreSQL وبلا `TIRA_REQUIRE_DATABASE=1`، فكانت كل اختبارات
   التخزين/RLS/provision تُتخطّى بصمت (`SKIP`)، ومنها
   `unknown_recipient_is_refused_at_rcpt` — الإثبات الوحيد أن المعالج ليس
   `open relay`. الإصلاح: خدمة `postgres:16` (`POSTGRES_USER`/`POSTGRES_PASSWORD`
   بقيمة postgres، و`POSTGRES_DB=tira_test`، والمنفذ 5432، وفحص صحة `pg_isready`)،
   و`env` على مستوى المهمة يضبط `TIRA_TEST_DATABASE_URL` و`TIRA_REQUIRE_DATABASE="1"`،
   و`--locked` على clippy/test. صار غياب اتصال القاعدة فشلاً صريحاً لا تخطّياً.
   الهجرة 0004 تُنشئ الدور `tira_app` تلقائياً عند الاتصال (تحتاج `superuser`؛
   وpostgres كذلك).

2. **M-020 — BOM في قطع DNS.** كانت أربعة ملفات (`tira.test`/`uit.ac.ma` بامتداد
   `.zone`/`.dns.json`) تبدأ بـ UTF-8 BOM (`EF BB BF`)، وهو يفسد أول اسم DKIM في
   الzone ويكسر محلّلات JSON الصارمة. الإصلاح: جُرّد الBOM بايتياً بدقّة
   (2093→2090، و1530→1527، و2162→2159 مرتين). السبب الجذري: ملفات قديمة؛ فالمولّد
   `dns-zone.ps1` يكتب أصلاً بلا BOM. أُضيفت خطوة CI باسم «DNS artifacts must be
   BOM-free» ترفض أي BOM وترفض غياب القطع (لا نجاح صوري).

3. **H-09 — كلمة مرور القاعدة في argv.** كان `backup.ps1` و`backup-common.ps1`
   يمرّران `--dbname=$DatabaseUrl` (فيها كلمة المرور) إلى psql/pg_dump، فأي مستخدم
   محلّي يقرأ سطر أوامر العملية. الإصلاح: `ConvertTo-SafeDatabaseUrl` ينقل كلمة
   المرور إلى `PGPASSWORD` (مفكوكة `percent-encoding`) ويعيد URL بلا كلمة مرور؛
   كل مواضع argv الستة في `backup.ps1` و`Get-TiraCounts` تستعمل `$SafeDatabaseUrl`.
   الدالة `idempotent` (URL بلا كلمة مرور يعود كما هو، و`PGPASSWORD` يبقى كما هو
   لـ`restore-drill`). الURL الكامل الآن يظهر فقط في المعامل والتحقق والتحويل
   والعرض وال`manifest` المنقّح. كان `milter-apply.ps1:29-31` أصلاً المثال الجيّد
   (env لا argv).

4. **M-022 — milter يبلغ PASS بلا مقارنة.** كان `milter-apply.ps1` يقرأ الكائن من
   المحرك ويطبع `PASS` بلا مقارنة بالخطة المطبّقة، فmilter معطّل أو `fail-open`
   يُعلَن نجاحاً. الإصلاح: توكيدات تقارن قراءة المحرك (`$detail`) بالخطة
   (`$milter`): المضيف/المنفذ، و`enable.else`، و`tempFailOnError`؛ أي اختلاف يرفع
   خطأ (مرشّح معطّل يجب ألا يبلغ `PASS`، و`fail-open` يجب ألا يُجاز كـ`fail-closed`).

5. **H-10 — إضعاف دائم لمخزن الثقة في سكريبت تطوير.** كان `sign-dev.ps1` ينشئ
   شهادة توقيع كود قابلة للتصدير (5 سنوات) ويثقها في `CurrentUser\Root` و
   `TrustedPublisher`، مع `-Force` الذي يجرد التواقيع ويضيف `overlay` عشوائياً
   لتجاوز Smart App Control، وبلا مسار تنظيف. الإصلاح: (أ) حارس CI يرفض العمل على
   أي وكيل بناء (`CI`/`GITHUB_ACTIONS`/`TF_BUILD`/`BUILD_BUILDID`/`JENKINS_URL`)؛
   (ب) إنشاء الشهادة صار يتطلب `-AllowTrustStore` صراحةً (لا إضعاف ضمني للثقة)؛
   (ج) `-RemoveTrust` يزيل شهادة التطوير على نحو قابل للعكس من
   My/Root/TrustedPublisher.

**الملفات المتغيّرة:**

- `.github/workflows/ci.yml`: خدمة `postgres:16` + العلمان + `--locked` + خطوة فحص BOM.
- `scripts/backup-common.ps1`: `ConvertTo-SafeDatabaseUrl`؛ و`Get-TiraCounts` يستعملها.
- `scripts/backup.ps1`: `$SafeDatabaseUrl` في كل argv (6 مواضع)؛ لا كلمة مرور في argv.
- `scripts/milter-apply.ps1`: توكيدات المطابقة بعد القراءة (M-022).
- `scripts/sign-dev.ps1`: حارس CI + `-AllowTrustStore` + `-RemoveTrust` (H-10).
- `deploy/dns/{tira.test,uit.ac.ma}.{zone,dns.json}`: تجريد BOM (4 ملفات).

**البوابات:** `fmt --all` = 0، و`clippy --workspace --all-targets --locked -D warnings`
= 0 (نظيف)، و**195/195 اختباراً** على PostgreSQL حقيقي (`TIRA_REQUIRE_DATABASE=1`،
0 فشل، 0 تخطّي) — بلا تغيير لأن وحدة E لم تمسّ Rust. تحقّق إضافي على مستوى
السكربتات: فحص AST parse لأربعة سكريبتات PowerShell = 0 أخطاء؛
و`ConvertTo-SafeDatabaseUrl` = 6/6 اختبارات وظيفية (تجريد الكلمة، وضبط `PGPASSWORD`،
والأمان عند التكرار، وفك `percent-encoding`)؛ وفحص وظيفي شامل لـ`backup.ps1`
(كلمة المرور عبر env ← كل مراحل القاعدة + pg_dump تنجح، وال`manifest` منقّح
`postgres://postgres:***@...`، وdb.dump/blobs.tar/manifest.sha256 موجودة،
وconsistency=quiescent)؛ ورفض حارس CI لـsign-dev مُتحقّق؛ وفحص `ci.yml` البنيوي
= 0 مشاكل + كل المفاتيح المطلوبة موجودة.

**ملاحظات صادقة:**

- (أ) M-022 وH-07 لا يُختبران حياً بالكامل: محرك Stalwart متوقف (لا milter apply
  حيّ)، وGitHub Actions لا يعمل محلياً. `ci.yml` تُحقّق بنيوياً (لا محارف tab،
  وإزاحة منتظمة بمضاعفات 2، والمفاتيح المطلوبة) لكن لا محلّل YAML كامل متاح
  محلياً (لا powershell-yaml ولا python ولا node)؛ سلوك الworkflow وقت التشغيل
  غير مُتحقّق حتى أول push. خدمة postgres والعلمان يعكسان الإعداد المحلي الذي
  يجتاز 195/195.
- (ب) مسارا `-RemoveTrust` و`-AllowTrustStore` في H-10 تُحقّقا عبر AST parse ومراجعة
  الكود، **لم يُنفّذا**، تجنّباً تعديل مخزن ثقة هذا الجهاز أو حذف شهادة توقيع
  التطوير الحقيقية للمستخدم (SAC مفروض هنا). نُفّذ فقط رفض حارس CI (يرمي الخطأ
  قبل أي منطق شهادة).
- (ج) الفحص الوظيفي لـ`backup.ps1` استعمل `-AllowMissingBlobs` ضد `tira_test`
  (814 رسالة اختبار متبقية، و7 مفاتيح blob مفقودة على القرص) في مجلد مؤقت حُذف
  بعده؛ يثبت مسار اتصال/تنقيح H-09 لا أرشيف إنتاج. الأرشيف الحقيقي يحتاج جذر
  الblob الفعلي.
- (د) أثر بيئي لوحظ (ليس انحداراً في H-09، وتُرك خارج النطاق): إطلاق PowerShell
  من Git Bash يرث `/usr/bin/tar` (GNU tar) الذي يسيء قراءة `C:\...` كـ`host:file`
  بعيد فيفشل tar الblob. مع bsdtar الخاص بـSystem32 (المضيف الطبيعي) يكتمل
  النسخ. `backup.ps1` يختار أول `tar.exe` في PATH؛ تصليب هذا الاختيار (فرض
  System32 أو `--force-local`) بند متانة منفصل خارج نطاق تدقيق هذه الوحدة.
- (هـ) وحدة E هي CI/سكريبتات/DNS فقط: لم تُمَسّ أي حزمة، فخط الأساس 195 اختباراً
  بلا تغيير. نسخ المحرك الأصلي (H-08)، وthrottle الدخول (M-04)، ونقطة الأدلة
  (M-06)، وتثبيت Rspamd (M-19)، وسلامة/نسخ offsite (M-21) تبقى لموجات لاحقة.

**لم يُبنَ بعد (معلن):** موجات 2–4 (SMTP، وVault TIRAS2، والتشغيل/throttle، وLow+CI)؛
وأول push يشغّل `ci.yml` فعلياً؛ وأول milter apply حيّ ضد Stalwart.

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_RestoreVerified_20261110.zip`
(1,422,435 بايت، SHA-256 `0605e28df20efa17e4e4de20cd47efdc7c2d8124bc4ec7c787a0e446e691400a`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح، مصدر فقط بلا `target/` ولا `data/`):
`TiraMail_Snapshot_InfraHardening_20261110.zip` (1,424,709 بايت، SHA-256
`b246deeddf00cd463ef5b13ab1f4a95b86f59b106e697e38ce17b3186a337270`).

### 9.21 الموجة 2 / وحدة SMTP — تعطيل الخادم اليدوي افتراضياً وتصليب عيوبه (10 نوفمبر 2026)

**النطاق:** حزمة `tira-smtp` (الخادم اليدوي inbound ESMTP) ونقطة الإقلاع
`crates/tira-api/src/main.rs` فقط. لا يمسّ العرض ولا التخزين ولا المحرك.
Stalwart يبقى محرك الإنتاج (SMTP/IMAP/JMAP)؛ هذا الخادم مسار تطوير/احتياط
وسطح هجوم إضافي، لذلك عُدِّل قراره المعماري.

**القرار المعماري (من المستخدم):** «عطّله افتراضياً + أصلح عيوبه». فالخادم
اليدوي لا يعمل إلا بتفعيل صريح، وحتى حينها يكون مُصلَحاً لا معطوباً.

**ما أُصلح ولماذا:**

1. **تعطيل افتراضي (قرار المستخدم + DEEP_MAP «مرشح للإيقاف»).** كان `main.rs`
   يربط منفذ SMTP ويشغّل الخادم دائماً متى وُجدت قاعدة البيانات. الإصلاح: بوابة
   `TIRA_SMTP_ENABLE` (افتراضي `false`؛ تقبل `"1"` أو `"true"`). عند التعطيل: لا
   ربط ولا `spawn`، و`AppState` يأخذ `smtp_bound=None`، وسطر `info` يوضح كيفية
   التفعيل. عند التفعيل: يربط `TIRA_SMTP_ADDR` (افتراضي `127.0.0.1:2525`) ويشغّل
   الخادم المُصلَح. هذا يزيل سطح DoS الإضافي من التشغيل الافتراضي.

2. **H-04 — DoS عبر تخزين سطر غير محدود + لا مهلة DATA + لا حدّ جلسات.** ثلاثة
   متجهات في `server.rs`:
   - (أ) كان `read_line`/`read_until` ينموان بلا حد حتى وصول `\n`؛ سطر بحجم
     gigabyte بلا newline يتجاوز حدّ SIZE ويستنزف الذاكرة. الإصلاح:
     `read_line_capped` يقرأ عبر `fill_buf`/`consume` ويفرض سقفاً
     (`MAX_COMMAND_LINE=2048`، `MAX_DATA_LINE=2048`؛ RFC 5321 §4.5.3.1 يحدد 512
     للأوامر و1000 لنص الرسالة، وتُرك هامش للتوسيع). تجاوز الحدّ يعيد
     `InvalidData` **قبل** تمديد المخزن ⇒ الأمر `500` والجلسة تُغلق؛ وفي DATA ⇒
     `TooLarge` ⇒ `552`. الذاكرة مقيدة قبل التمديد لا بعده.
   - (ب) مهلة الخمول لكل أمر لا تغطي DATA؛ عميل يتقاطر (trickle: بايت كل ثوانٍ)
     يمسك مهمة للأبد. الإصلاح: `timeout(session_timeout, read_data(...))`؛ انتهاء
     المهلة ⇒ `421` وإنهاء الجلسة.
   - (ج) لا حدّ للاتصالات المتزامنة؛ فيضان يستنزف المهام. الإصلاح:
     `Semaphore::new(MAX_SESSIONS=256)` و`try_acquire_owned` لكل قبول؛ ما زاد
     يُسقط (`drop(stream)`) لا يُصفّ. التصريح محمول في المهمة (`let _permit`)
     ويُحرَّر عند انتهائها، فعدد الجلسات المتزامنة ≤ 256 قطعاً.

3. **M-18 — رفض المسار العكسي الصفري `MAIL FROM:<>`.** كان `angle_addr` يرفض
   `<>` بـ `501` ⇒ لا DSN/bounce (خرق RFC 5321 §4.5.1، وفقد صامت للارتدادات).
   الإصلاح: `angle_addr(input, allow_null)`؛ MAIL FROM يمرر `true` وRCPT TO يمرر
   `false`. الفراغ يُقبل فقط داخل زوايا صريحة ومع `allow_null` ⇒ `Some("")`؛
   و`RCPT TO:<>` يبقى مرفوضاً (لا تسليم إلى nobody)؛ والوسيط الفارغ بلا زوايا
   ليس مساراً صفرياً أبداً.

**الملفات المتغيّرة:**

- `crates/tira-smtp/src/server.rs`: ثابتات الحدود (`MAX_COMMAND_LINE`/
  `MAX_DATA_LINE`/`MAX_SESSIONS`)، وsemaphore في `serve_on`، و`read_line_capped`،
  وإعادة كتابة `read_data`، ومهلة DATA، و`angle_addr(allow_null)`، واستيراد
  `AsyncBufReadExt`/`Semaphore`، واختبارا وحدة جديدان.
- `crates/tira-smtp/tests/session.rs`: خمسة اختبارات تكامل جديدة (+ استيراد
  `AsyncReadExt` لقراءة الاتصال الزائد).
- `crates/tira-api/src/main.rs`: بوابة `TIRA_SMTP_ENABLE` (افتراضي off)، وإزالة
  ربط المنفذ غير المشروط.

**البوابات:** `fmt --all` = 0 (بعد ضبط مسافة تعليق واحدة)، و`clippy --workspace
--all-targets --locked -D warnings` = 0 (بعد إزالة استيراد `AsyncBufRead` زائد؛
فالطريقة `consume` تُحلّ عبر السمة العليا لـ`AsyncBufReadExt`)، و**202/202
اختباراً** على PostgreSQL حقيقي (`TIRA_REQUIRE_DATABASE=1`، 0 فشل، 0 تخطّي).
الأساس 195 + 7 جديدة: اختبارا وحدة في `server.rs`
(`null_reverse_path_only_for_mail_from` و`read_line_capped_bounds_the_buffer`)،
وخمسة تكامل في `session.rs` (`accepts_null_reverse_path_for_bounces`،
`refuses_overlong_command_line`، `refuses_overlong_data_line`،
`data_phase_times_out_on_a_stalled_client`، `caps_concurrent_sessions`). حزمة
`tira-smtp`: 11→18 (المكتبة 6→8، والتكامل 5→10). الاختبارات السبعة مؤكدة بالاسم.

**ملاحظات صادقة:**

- (أ) الخادم اليدوي معطّل افتراضياً؛ الاختبارات تشغّله عبر `serve_on` مباشرة
  (منفذ 0) لا عبر `main.rs`. بوابة `TIRA_SMTP_ENABLE` تُحقّقت بالمراجعة و`cargo
  check`؛ لم يُشغّل `main.rs` حياً ضد قاعدة إنتاج لاختبار البوابة طرف-إلى-طرف
  (يحتاج تشغيل العملية، ومنفذ SMTP الافتراضي معطّل الآن).
- (ب) محرك Stalwart متوقف؛ هذا الخادم مسار احتياط/تطوير لا محرك الإنتاج.
  التصليب لا يغني عن Stalwart للإنتاج (TLS وSMTP فعليان هناك).
- (ج) اختبار الحدّ `caps_concurrent_sessions` يفتح 257 اتصالاً: 256 ممسوكة تقرأ
  تحية `220` (التحية تُرسل بعد حجز التصريح فقط، فحجز التصاريح الـ256 مُثبت قبل
  محاولة الـ257)، ثم الـ257 يُسقط (EOF أو reset). نحو 513 مقبساً في عملية واحدة؛
  تحت حدّ `ulimit` النموذجي (1024) على وكيل ubuntu. اختبار المهلة نحو 800ms على
  loopback.
- (د) مهلة DATA هي مهلة **إجمالي النقل** (`session_timeout`، افتراضي 300s) لا
  مهلة خمول لكل قراءة. رسالة 10MB تحتاج ≥ 33KB/s لتكمل ضمن 300s؛ مقبول لأي اتصال
  حقيقي وقابل للضبط. المقايضة معلنة: حدّ مطلق لعمر المهمة (أقوى ضد التقاطر)
  مقابل قصّ نقل شرعي بطيء جداً.
- (هـ) `read_line_capped` يقيد المخزن عند `max_len`؛ و`fill_buf` نفسه يعيد حتى
  سعة `BufReader` (8KB) المحدودة. لا نمو غير محدود في أي مرحلة.

**لم يُبنَ بعد (معلن):** STARTTLS/AUTH/PIPELINING/CHUNKING والترشيح (تعود لمسار
submission والطبقة المرشحة، خارج نطاق الخادم اليدوي المعطّل)؛ وVault TIRAS2
(الوحدة التالية)؛ وموجات 3–4.

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_InfraHardening_20261110.zip`
(1,424,709 بايت، SHA-256 `b246deeddf00cd463ef5b13ab1f4a95b86f59b106e697e38ce17b3186a337270`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح والاختبارات، مصدر فقط بلا `target/` ولا
`data/` ولا `proof_try_*.exe`): `TiraMail_Snapshot_SmtpHardening_20261110.zip`
(1,431,607 بايت، SHA-256 `ECD8CED510FB5D21F5F5F5B22FEA9CB83790C9F257501FD1E536FC019F6DA07E`).

### 9.22 الموجة 2 / وحدة Vault — صيغة TIRAS2 المُصادَقة وتصليب إخراج tira-seal (10 نوفمبر 2026)

**النطاق:** حزمة `tira-vault` (`src/lib.rs` و`src/bin/tira-seal.rs`) وسكربت
`scripts/backup.ps1` والوثائق المرجعية فقط. لا يمسّ العرض ولا التخزين ولا المحرك
ولا SMTP. الختم at-rest (لا E2E)؛ هذه الوحدة تصلح سلامة الصيغة وصلاحيات الإخراج.

**القرار المعماري (من المستخدم):** «إصدار جديد متوافق TIRAS2». فالقارئ يقرأ
`TIRAS1` القديم، وكل ختم جديد `TIRAS2`، بلا كسر للأرشيفات الموجودة. تغيير الصيغة
هو القرار الذي طلبه الفحص لـM-016 (لا يكفي ترقيع خارجي).

**ما أُصلح ولماذا:**

1. **M-016 — قطع البثّ غير قابل للكشف + `verify` طمأنينة كاذبة.** كان `TIRAS1`
   يصادق كل إطار بترتيبه فقط (`AAD = frame_index`)؛ أي بادئة صحيحة تُفتح بنجاح،
   فحذف الإطار الأخير على حدود إطار لا يُكشف، و`verify` يقول «سليم» لأرشيف
   مقتطع. الإصلاح: صيغة `TIRAS2` تضيف علم نهاية (`final flag`) وطولاً تراكمياً
   مُصادَقاً عليهما في `AAD`، وإطاراً نهائياً فارغاً يصادق الطول الكلي. القارئ
   يتطلب الإطار النهائي ثم `EOF` ⇒ الاقتطاع على حدود إطار مُميت، والذيل المُلصق
   بعد النهائي مرفوض، وتبديل العلم أو الطول يكسر الوسم. هذا يزيل الحاجة إلى
   SHA-256 خارجي لكشف الاقتطاع (كان يعوّض فجوة TIRAS1).

2. **M-017 — صلاحيات إخراج `tira-seal`.** كان `File::create` يفتح الهدف بـ0644
   (النص المفكوك مقروء للجميع عند `open`)، و`--force` يقتطع الهدف الموجود **قبل**
   المصادقة (كلمة مرور خاطئة تدمّر الملف السابق)، وفشل منتصف البثّ يترك ملفاً
   جزئياً عند المسار النهائي. الإصلاح: كتابة إلى temp شقيق خاص (`0600` على Unix،
   `create_new` حصري)، و`rename` فوق الهدف فقط بعد نجاح البثّ كاملاً، و`Drop`
   يحذف temp عند أي خطأ. الهدف لا يُمَس حتى النجاح؛ وكلمة المرور الخاطئة لم تعد
   تدمّر الملف السابق.

**الصيغة (TIRAS2):**

- الترويسة: `magic[6]="TIRAS2" | salt[16] | chunk_size:u32le` (26 بايتاً، مثل
  TIRAS1؛ السحر وحده يميّز الإصدار).
- الإطار: `frame_len:u32le | nonce[12] | flags[1] | cumulative_total:u64le | ciphertext+tag[16]`.
- `AAD = index(u64le) ‖ flags(u8) ‖ cumulative(u64le)` = 17 بايتاً.
  `FRAME_OVERHEAD_V2 = 37` (مقابل 28 في TIRAS1). العلم والطول في Clear لكنهما
  مربوطان في الوسم، فقلب أيٍّ منهما يكسر المصادقة.
- إطارات البيانات `flags=0`؛ إطار نهائي واحد فارغ `flags=1` يصادق الطول الكلي.
  المدخل الفارغ ⇒ صفر إطارات بيانات + إطار نهائي (لا ترويسة مجردة قد تُحسب اقتطاعاً).
- القارئ (`open_stream_v2`): يرفض أي إطار بعد النهائي، ويتحقق أن الطول التراكمي
  المُصادَق = البايتات المفكوكة حتى الآن (دفاع عميق فوق الوسم)، ويتطلب النهائي ثم
  `EOF`. المفتاح يُشتق مرة واحدة لكل ملف (Argon2id ~50ms) لا لكل إطار؛ والذاكرة
  = `chunk_size` + إطار واحد مهما كبر المدخل.

**التوافق العكسي:** `open_stream` يقرأ السحرين: `TIRAS2` ⇒ `open_stream_v2`،
و`TIRAS1` ⇒ `open_stream_v1` (legacy). و`seal_stream` يكتب `TIRAS2` دائماً.

**الملفات المتغيّرة:**

- `crates/tira-vault/src/lib.rs`: ثابتات TIRAS2 (`STREAM_MAGIC_V2`/`FLAGS_LEN`/
  `TOTAL_LEN`/`FRAME_OVERHEAD_V2`/`AAD_LEN_V2`/`FLAG_DATA`/`FLAG_FINAL`)،
  و`seal_stream` بإطار نهائي، و`frame_aad_v2`/`emit_frame_v2`، و`open_stream`
  مُوزِّعاً، و`open_stream_v2` (فحوص الاقتطاع/التلاعب)، و`open_stream_v1`
  (legacy)، و7 اختبارات جديدة.
- `crates/tira-vault/src/bin/tira-seal.rs`: `Output`/`FileOutput`، و`commit`
  ذرّي (يغلق المقبض قبل `rename` لأن Windows يرفض rename ملف مفتوح)، و`Drop`
  ينظّف temp، و`create_output`/`unique_temp_path`/`create_temp_file` (0600)،
  و`run_to_output`، و5 اختبارات (سادس `cfg(unix)` لبتات الوضع).
- `scripts/backup.ps1`: وصف الصيغة في manifest صار `TIRAS2` (وصفي؛
  `restore-drill.ps1` يكشف السحر تلقائياً فيقرأ الاثنين).
- `README.md` و`docs/DEEP_MAP.ar.md` و`docs/deep-map.html`: وصف الصيغة الراهنة
  `TIRAS2` (مع قراءة `TIRAS1` القديم).

**البوابات:** `fmt --all` = 0 (بعد ضبط تنسيق اختبارات vault)، و`clippy --workspace
--all-targets --locked -D warnings` = 0 (بعد إصلاح `doc_lazy_continuation` في
تعليق `tira-seal.rs`: سطر فارغ يفصل القائمة عن الفقرة الختامية)، و**214/214
اختباراً** على Windows (0 فشل، 0 تخطّي)؛ و215 على Linux CI (اختبار `0600`
`cfg(unix)`). الأساس 202 + 12 جديدة: 7 في `lib.rs` و5 في `tira-seal.rs`. حزمة
`tira-vault`: 12→24 (المكتبة 12→19، والثنائي 0→5). الاختبارات مؤكدة بالاسم:

- المكتبة (7): `stream_v2_writes_tiras2_magic`،
  `stream_v2_detects_frame_aligned_truncation`،
  `stream_v2_detects_trailing_frame_after_final`،
  `stream_v2_detects_final_flag_tamper`، `stream_v2_detects_total_length_tamper`،
  `stream_v1_legacy_still_opens`،
  `stream_v1_truncation_is_not_detected_documents_legacy_gap`.
- الثنائي (5 على Windows + 1 على Unix):
  `atomic_file_publishes_content_and_leaves_no_temp`،
  `failed_write_leaves_no_final_and_no_temp`،
  `force_does_not_destroy_existing_target_on_failure` (الإصلاح الجوهري)،
  `force_replaces_existing_target_on_success` (يؤكد أن rename يستبدل على Windows)،
  `no_force_refuses_existing_target`، و`published_file_is_owner_only_0600`
  (`cfg(unix)`، يعمل على وكيل ubuntu في CI).

**ملاحظات صادقة:**

- (أ) اختبار `0600` (`published_file_is_owner_only_0600`) `cfg(unix)` ⇒ لا يعمل
  على Windows؛ يعمل على Linux CI. على Windows تحقّقنا من الذرّية (rename يستبدل
  الموجود)، وعدم ترك temp، وأمان `--force` عند الفشل؛ أما بتات الوضع فـWindows لا
  مكافئ std لها (يرث ACL المجلد الأب) — موثّق كقيد منصّة في `create_temp_file`.
- (ب) فجوة `TIRAS1` القديمة باقية: `open_stream_v1` لا يكشف الاقتطاع على حدود
  إطار (لا إطار نهائي). الاختبار
  `stream_v1_truncation_is_not_detected_documents_legacy_gap` يوثّق هذا بصراحة لا
  يُخفيه. الأرشيفات القديمة تُقرأ بضمانها الأصلي؛ وكل ختم جديد `TIRAS2` بالضمان
  الأقوى. من يريد ترقية أرشيف قديم: `open` ثم `seal` (يعيد الختم بـTIRAS2).
- (ج) لم يُعَد تشغيل `restore-drill` حياً بـTIRAS2 (المحرك متوقف؛ والنسخة الحية
  تحتاج قاعدة + blobs). `backup.ps1` حدّث وصف الصيغة، و`restore-drill` يكشف السحر
  تلقائياً. الإثبات الحي في `deep-map.html` (سطر 208، «وفي 6 نوفمبر 2026») يبقى
  `TIRAS1` لأنه سجل تاريخي دقيق لاسترجاع جرى فعلاً بذلك اليوم — لم يُزوَّر إلى
  `TIRAS2`. أما سطر 183 (وصف الحالة الراهنة للطبقة) فحُدّث إلى `TIRAS2`.
- (د) `M-017` كان مدرجاً تحت موجة 3 في خطة الفحص، لكنه في نفس الحزمة
  (`tira-seal`) ومقترن عضوياً بتغيير صيغة الختم، فأُنجز مع `M-016` هنا في وحدة
  واحدة.
- (هـ) عمود عدد الأسطر في `DEEP_MAP.ar.md` تُرك كلقطة تاريخية (لا يُحدّث مع كل
  تعديل)؛ الوصف النصي للصيغة حُدّث.
- (و) `M-016` عن التأطير لا عن KDF؛ معاملات Argon2id لم تتغيّر (finding معاملات
  Argon2 DoS يبقى لموجات لاحقة).

**لم يُبنَ بعد (معلن):** إدارة مفاتيح/KMS؛ وإعادة ختم الأرشيفات القديمة تلقائياً؛
وموجات 3–4 (H-08 نسخ المحرك، M-04 throttle، M-06 endpoint الأدلة، M-07
`spawn_blocking`، M-19 Rspamd، M-21 سلامة النسخ، وإصلاحات Low + تقوية CI).

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_SmtpHardening_20261110.zip`
(1,431,607 بايت، SHA-256 `ECD8CED510FB5D21F5F5F5B22FEA9CB83790C9F257501FD1E536FC019F6DA07E`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح والاختبارات، مصدر فقط بلا `target/` ولا
`data/` ولا `proof_try_*.exe`): `TiraMail_Snapshot_VaultHardening_20261110.zip`
(1,439,662 بايت، SHA-256 `18D50A6FACAABA390469CA832AD35DA37D4DFA266393C0D699A837DF30515692`).

### 9.23 الموجة 3 / وحدة F — تصليب زمن تشغيل API: throttle ذرّي، أدلة loopback-only، وقراءة غير حاجزة (10 نوفمبر 2026)

**النطاق:** حزمتا `tira-api` و`tira-store` فقط (مسار الطلبات زمن التشغيل). لا يمسّ
العرض ولا المحرك ولا SMTP ولا Vault. هذه الوحدة تصلح ثلاثة عيوب تشغيلية:
`M-04` (throttle)، و`M-06` (تعريض endpoint الأدلة)، و`M-07` (حجز I/O على المسار
غير المتزامن).

**القرار المعماري (من المستخدم):** `M-06` = «Loopback فقط + cache»: endpoint
الأدلة حسّاس (يكشف بنية control-plane)، فيُقصى عن الشبكة ويُخزّن مؤقتاً في العملية.
أما `M-04` و`M-07` فإصلاحات فوز-واضح لزمن التشغيل ضمن التفويض المستقل «أصلح كل
Tira Mail … قوية بدون ثغرات وسلسة» (لا قرار معماري خارجي مطلوب). `H-08` و`M-21`
و`M-19` (النسخ وRspamd) لم تُنجز هنا؛ هي وحدة G التالية (scripts).

**ما أُصلح ولماذا:**

1. **M-04 — throttle غير ذرّي + سقف يعتمد على وجود الحساب.** كان `password_login`
   يقرأ `is_blocked` ثم يتحقق من كلمة المرور (Argon2 مكلف) ثم `record_failure` عند
   الخطأ: هذا فحص-ثم-فعل (check-then-act) غير ذرّي؛ طلبات متزامنة كثيرة تمرّ كلها
   قبل أن يُسجّل أي فشل، فيتجاوز المهاجم السقف، وكلها تحرق عمل Argon2 قبل أن يبلغ
   العدّاد السقف. وكان السقف لكل عنوان بريد فقط، فمهاجم يرشّق عناوين كثيرة (موجودة أو
   غير موجودة) لا يصيب أي سقف حساب ⇒ عمل Argon2 كلي غير محدود (DoS). الإصلاح:
   - `try_attempt`/`try_attempt_capped`: حجز ذرّي قبل التحقق (reserve-before-verify).
     تحت قفل واحد، يفحص السقف ويزيد العدّاد في نفس المنطقة الحرجة؛ إن بلغ السقف
     ⇒ `false` (يُرفض الطلب قبل أي Argon2). هذا يغلق السباق: الطلبات المتزامنة
     تتسلسل على الـmutex، فيُمنح بالضبط `max` (يثبته اختبار 64 خيطاً/سقف 8 ⇒ 8 ممنوحة).
   - دلوان (bucket): per-IP (`login-ip:{ip}`، سقف `DEFAULT_IP_MAX_ATTEMPTS=40`،
     لا يُسترد عند النجاح) يحدّ العمل الكلي من مصدر واحد بغضّ النظر عن عدد العناوين
     التي يرشّقها أو وجودها؛ وper-account (سقف `max_failures=8`، يُسترد بـ`clear`
     عند النجاح) يحمي حساباً واحداً من brute-force دون قفل مستخدم شرعي أصاب أخيراً.
     مفتاح IP يحذف العنوان (لا يخلطه بالحساب). الحجز قبل verify ⇒ المحاولة المرفوضة
     لا تكلّف Argon2.
   - `profile_password`: الحجز فوراً قبل verify كلمة المرور الحالية فقط (أخطاء
     تحقق كلمة المرور الجديدة لا تحرق خانات)؛ أُبقي `is_blocked` العلوي كخروج مبكر
     رخيص للقراءة فقط، والحجز الموثوق قبل verify. أُزيل `record_failure` من فرعي
     كلمة المرور الخاطئة (الحجز عدّ المحاولة سلفاً).

2. **M-06 — endpoint الأدلة معرّض للشبكة + إعادة بناء غير محدودة.** كان
   `/api/v1/control-plane-proof` (وبديله `/api/v1/sovereignty-proof`) متاحاً لأي
   عميل، ويكشف بنية control-plane (عزل المستأجرين، audit، DMARC/DKIM/SPF، تصنيف
   المضيف). وكل طلب يعيد البناء كاملاً (قراءة artifacts + استعلامات COUNT للـaudit
   + probe RLS) ⇒ أي عميل بعيد يطلب عدداً غير محدود من إعادة البناء المكلفة (DoS)
   ويستطلع البنية. الإصلاح (قرار المستخدم «Loopback فقط + cache»):
   - `loopback_only` middleware على `route_layer` للـsub-router الخاص بالأدلة: يقرأ
     `ConnectInfo<SocketAddr>` (يملؤه `into_make_service_with_connect_info` في
     `main.rs` من مقبس TCP الحقيقي)؛ إن لم يكن loopback ⇒ **404 لا 403** (كي لا
     يؤكّد الماسح وجود المسار). يفشل مغلقاً (fail-closed) إن غاب `ConnectInfo`.
     يتجاهل رؤوس forwarded (`X-Forwarded-For`/`X-Real-IP`) فلا انتحال.
   - cache في العملية محدود بـTTL=30s (`ProofCache = Arc<tokio::sync::Mutex<Option<(Instant, Bytes)>>>`):
     الطلب البارد يبني مرة، واللاحقة ضمن TTL تعيد نفس البايتات (byte-identical، لا
     إعادة بناء ولا إعادة تسلسل). `tokio::sync::Mutex` يُحمل عبر `await` فيسلسل
     الطلبات الباردة المتزامنة إلى بناء واحد (لا thundering herd). الأخطاء لا
     تُخزّن (دليل غير متاح قد يتاح عند إعادة المحاولة). `no-store` يبقى (الـcache
     في العملية فقط؛ لا متصفح ولا وكيل يحتفظ بالحمولة الحساسة)، و`generated_at`
     يعكس بصدق وقت جمع الدليل (حتى 30s مضت على hit).
   - لماذا `Bytes` لا `Arc<ProofResponse>`: تسلسل مرة واحدة (`serde_json::to_vec`)،
     والـhit ينسخ البايتات نسخاً رخيصاً (ref-count)؛ `Arc<T>: Serialize` يحتاج ميزة
     serde `rc` (غير مفعّلة) فتجنبنا تغيير الاعتمادات. `ProofResponse` بيانات مشتقة
     بلا floats ⇒ التسلسل لا يفشل عملياً؛ والمسار الاحتياطي (`Json`) محفوظ إن فشل.

3. **M-07 — قراءة blobs + تحليل MIME على المسار غير المتزامن (blocking).** كان
   `snippet_for` و`reading_model` يقرآن الـblob من القرص (`store.raw_message`)
   ويحللان MIME (`tira_smtp::message::parse`) مباشرة داخل handler غير متزامن؛ هذه
   عمليات حاجزة تُوقف executor thread كله، فتحت حملٍ متزامن تتجمّع الطلبات ويتعطّل
   الخادم (DoS/latency). وكانت `read_message_bytes` في `scoped.rs` تقرأ القرص
  بشكل حاجز داخل معاملة. الإصلاح:
   - `PgStore::raw_message_async`: غلاف غير متزامن ينقل قراءة القرص إلى
     `spawn_blocking` (مجمع blocking المخصص)، بclone لـ`Arc<dyn BlobStore>` +
     المفتاح (لا استعارة تعبر `await`). أُضيف `StoreError::Task(#[from] JoinError)`
     ليفشل panic/إلغاء المهمة بنظافة. `raw_message` الـsync يبقى للاختبارات وللمستدعين الحاجزين.
   - `snippet_for` صار async: `spawn_blocking` ينفّذ read+parse+snippet، ثم
     `.await.ok().flatten()` (panic/cancel ⇒ لا معاينة، لا خطأ قاتل). سقف
     `SNIPPET_MAX_BYTES=262144` يبقى. `message_model` صار async، و`inbox_page`
     تسلسلي (`for message … message_model(…).await`): مهمة blocking واحدة لكل
     معاينة، لا إشباع للمجمع.
   - `reading_model`: تحليل `body_text` داخل `spawn_blocking` (البايتات تُنقل، لا
     تُستخدم بعد). `scoped.rs::read_message_bytes` يستخدم `raw_message_async`؛
     المعاملة تبقى مفتوحة عبر `await` (ترتيب audit/commit لم يتغيّر) لكن قراءة
     القرص على المجمع blocking.

**الملفات المتغيّرة:**

- `crates/tira-api/src/limiter.rs`: وصف الوحدة، `DEFAULT_IP_MAX_ATTEMPTS=40`،
  `try_attempt`/`try_attempt_capped` (حجز ذرّي)، و5 اختبارات.
- `crates/tira-api/src/pages.rs`: `snippet_for` async + `spawn_blocking`،
  `message_model` async، `inbox_page` تسلسلي، `reading_model` `spawn_blocking`،
  `password_login` (ip_key + `try_attempt_capped` + `try_attempt` قبل verify،
  إزالة `record_failure`)، `login_ip_limiter_key`، `profile_password` (حجز قبل verify).
- `crates/tira-api/src/proof.rs`: `PROOF_CACHE_TTL=30s`، `ProofCache` (Bytes)،
  `new_proof_cache`، handler الـcache، و`json_response`.
- `crates/tira-api/src/lib.rs`: `loopback_only`، `AppState.proof_cache`
  (new/with_store)، sub-router أدلة بـ`route_layer` + `merge`.
- `crates/tira-api/tests/proof.rs`: `call_from` (حقن `ConnectInfo`)، `loopback_peer`،
  و4 اختبارات.
- `crates/tira-store/src/store.rs`: `StoreError::Task`، و`raw_message_async`.
- `crates/tira-store/src/scoped.rs`: `read_message_bytes` يستخدم `raw_message_async` + وصف.

**البوابات:** `fmt --all` = 0؛ و`clippy --workspace --all-targets --locked -D warnings`
= 0؛ و**223/223 اختباراً** على Windows (0 فشل، 0 تخطّي، 0 ignored)؛ و224 على Linux CI
(اختبار `0600` `cfg(unix)` من `M-17`). الأساس 214 + 9 جديدة: 5 في `limiter` (M-04)
و4 في `proof` (M-06). الاختبارات مؤكدة بالاسم:

- limiter (5): `try_attempt_reserves_then_blocks_at_the_ceiling`،
  `a_success_refunds_the_reserved_attempt`، `try_attempt_capped_honours_a_looser_ceiling`،
  `an_expired_window_starts_over_counting_this_attempt`،
  `concurrent_reserves_cannot_exceed_the_ceiling` (64 خيطاً، سقف 8، بالضبط 8 ممنوحة).
- proof (4): `a_non_loopback_peer_cannot_see_the_evidence_endpoint`
  (203.0.113.7 ⇒ 404 للمسارين، لا تسريب)، `a_request_without_connect_info_fails_closed`
  (None ⇒ 404)، `a_forwarded_header_cannot_spoof_loopback` (XFF/X-Real-IP + peer
  بعيد ⇒ 404)، `the_evidence_payload_is_cached_within_the_ttl` (نداءان نفس `generated_at`).

**التحقق الحي (أُجري جديداً هذه الجلسة):** الخادم يُقلع نظيفاً (storage ready
`blob_root=./data/blobs`، SMTP معطّل افتراضياً، proof configured `deploy/dns`/
`tira.test`/`development`، listening `127.0.0.1:8099`، بلا أخطاء). عبر TCP loopback
حقيقي ⇒ `ConnectInfo` يُملأ من المقبس ⇒ الحارس يسمح: proof1/proof2/alias/xff كلها
200 ومتطابقة بايتياً (SHA-256 `295f1b9782e22ba19ea5aed1ba8d861ea52330c3e160ab8de6b6d1110fa2bb16`
⇒ cache hits حية). فحص XFF: peer loopback حقيقي + `X-Forwarded-For`/`X-Real-IP`
مزيف بعيد (203.0.113.7) ⇒ 200 (الحارس يستشير المقبس فقط، لا الرؤوس). proof
`status=pass`، و`generated_at=2026-11-10T18:13:07.600762Z` (مطابق عبر النداءات
الأربعة). health: `database=ok`، و`smtp=null` (معطّل)، و`uptime=48s`. الخادم أُوقف،
والمنفذ أُطلق (http=000)، والسجل نظيف.

**ملاحظات صادقة:**

- (أ) الاختبارات in-process تحقن `ConnectInfo` يدوياً؛ التحقق الحي وحده يثبت أن
  الإنتاج (`into_make_service_with_connect_info`) يملؤه من مقبس TCP حقيقي. لذا أُجري
  smoke حي جديد هذه الجلسة (لا الاكتفاء بالاختبارات).
- (ب) الحارس 404 لا 403 للبعيد (كي لا يؤكّد وجود المسار)، ويفشل مغلقاً إن غاب
  `ConnectInfo`. خلف وكيل عكسي (reverse proxy) في الإنتاج، كل العملاء يبدون loopback
  للوكيل ⇒ يجب أن يبقى endpoint الأدلة غير معرّض للعموم عبر الوكيل (لا يُمرّر للعامة)؛
  هذا قيد نشر موثّق لا ثغرة في الكود (الكود يرى peer المقبس).
- (ج) cache الـ30s في العملية فقط (لا مشترك بين عمليات/خوادم)؛ متعدد النسخ ⇒ كل
  نسخة cache مستقل. هذا مقصود (لا اعتماد خارجي). والأخطاء لا تُخزّن.
- (د) `M-04`: IP bucket لا يُسترد عند النجاح (يحدّ العمل الكلي من مصدر واحد حتى لو
  أصاب أخيراً)؛ وaccount bucket يُسترد (`clear`) عند النجاح كي لا يُقفل مستخدم شرعي.
- (هـ) `M-07` لم يضف اختبارات جديدة (السلوك مطابق: نفس البايتات تُحلّل، فقط نُقلت
  للمجمع blocking)؛ اختبارات read/inbox الموجودة تؤكّد صحة المسار بعد النقل.
  وpanic/إلغاء مهمة blocking ⇒ لا معاينة/body (`ok().flatten()`)، لا خطأ قاتل.
- (و) `H-08` و`M-21` و`M-19` لم تُنجز في هذه الوحدة؛ هي وحدة G التالية (scripts).
  و`M-04`/`M-06`/`M-07` كلها زمن تشغيل API.

**لم يُبنَ بعد (معلن):** وحدة G (`M-19` Rspamd install-path، و`M-21` سلامة النسخ
الآن + تأجيل offsite/schedule/retention بقرار المستخدم، و`H-08` نافذة إيقاف + سكربت
نسخ المحرك)؛ وموجة 4 (إصلاحات Low + تقوية CI). وإدارة مفاتيح/KMS وإعادة ختم
الأرشيفات القديمة تلقائياً (من §9.22) تبقى معلنة.

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_VaultHardening_20261110.zip`
(1,439,662 بايت، SHA-256 `18D50A6FACAABA390469CA832AD35DA37D4DFA266393C0D699A837DF30515692`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح والاختبارات، مصدر فقط بلا `target/` ولا
`data/` ولا `proof_try_*.exe`): `TiraMail_Snapshot_RuntimeHardening_20261110.zip`
(1,449,120 بايت، SHA-256 `10A517818608FD0901D6422CBCC09AE47A1C5A993612B6B5895CC07FDFF67221`).

### 9.24 الموجة 3 / وحدة G — سلامة النسخ، ونسخ المحرك بنافذة إيقاف، ومسار تثبيت Rspamd (10 نوفمبر 2026)

**النطاق:** `scripts` و`deploy` فقط (طبقة التشغيل/النسخ على عقدة التحكم)، مع إصلاح
اختبار test-only في `tira-api`. لا يمسّ مسار طلبات API ولا العرض ولا crypto. تصلح
هذه الوحدة ثلاثة عيوب تشغيلية: `M-19` (مسار تثبيت Rspamd)، و`M-21` (سلامة النسخ
الاحتياطي)، و`H-08` (نسخ محرك Stalwart بنافذة إيقاف). وتضم إصلاحين تقنيين اكتُشفا
أثناء التحقق: اختيار tar المحمول (Resolve-Tar)، واختبار limiter متذبذب.

**القرار المعماري (من المستخدم):**

- `H-08` = «نافذة إيقاف + سكربت»: مخزن RocksDB المدمج لا يُنسخ حياً (compaction/WAL
  قد يعطي لقطة ممزقة)، فالسكربت يفشل مغلقاً ما لم يتحقق أن المحرك متوقف، ولا يوقف/يشغّل
  المحرك بنفسه (نافذة الإيقاف إجراء المشغّل، موثّق في الترويسة).
- `M-21` = «سلامة الآن + تأجيل offsite»: تحقق hash قبلي + توثيق manifest الآن؛ أما
  offsite/schedule/retention فمؤجلة بقرار المستخدم (معلنة أدناه).
- `M-19` فوز-واضح ضمن التفويض المستقل «أصلح كل Tira Mail … قوية بدون ثغرات وسلسة»
  (لا قرار معماري خارجي مطلوب).

**ما أُصلح ولماذا:**

1. **M-19 — مسار تثبيت Rspamd كان ينسخ `*.inc` فقط.** كان README القديم يطلب
   `local.d/*.inc`، فيثبت `options.inc` و`worker-proxy.inc` لكنه يُسقط صامتاً ثلاثة
   overrides من نوع `.conf` يحمّلها Rspamd فعلياً من local.d:
   - `actions.conf`: عتبات reject/add_header/greylist الصريحة.
   - `dkim_signing.conf`: `enabled = false` كي لا يوقّع Rspamd مرتين (Stalwart يملك
     DKIM)؛ بدونه قد يوقّع افتراض الحزمة.
   - `multimap.conf`: يربط خرائط allowlist/blocklist المؤسسية بمغلف SMTP؛ بدونه تبقى
     الخرائط عاطلة فلا تفعل ضوابط المرسل شيئاً.
   الإصلاح: `deploy/rspamd/install.sh` (جديد) ينسخ local.d كاملاً (`cp -a "$SRC/local.d/."`
   ⇒ الامتدادان معاً + قالب `redis.conf.disabled` الخامل)، وينسخ maps، و`chmod 0644`،
   و`chown` للـmaps لحساب rspamd إن وُجد. ثم حارس انحدار M-19 غير فارغ (non-vacuous):
   قائمة صريحة بـ3 `.conf` + 2 `.inc` تتحقق أنها هبطت فعلاً بعد النسخ (نسخ فاتها صامتاً
   هو عين العيب). ثم بوابة fail-closed على `rspamadm configtest`: إن فشل، الخروج غير
   صفري ولا restart (الdaemon الحي يحتفظ بإعداده السابق الجيد في الذاكرة حتى restart
   يدوي، فلا إعداد مكسور يُسقط البريد وحده). يفحص ترويسةً: `rspamadm` في PATH (البوابة
   إلزامية)، وصلاحية الكتابة، واكتمال المصدر. `--restart` اختياري، و`RSPAMD_CONFDIR`
   تجاوز للوجهة.

2. **M-21 — سلامة النسخ: لا تحقق hash قبلي + manifest غير موثّق.** كان `backup.ps1`
   يحزم blobs دون التحقق أن كل blob مطابق لعنوانه (content-address = SHA-256 المحتوى)،
   فbit-rot أو تسميم بنفس الحجم (same-size poison) يمرّ صامتاً إلى الأرشيف. الإصلاح:
   - `Get-BlobPreflightState` (backup-common.ps1): قبل الحزم يفحص كل blob مقابل عنوانه؛
     يصنّف Missing/WrongSize/Corrupt ويعيد Checked. `Test-ContentAddress` يعيد hash
     المحتوى ويطابقه بالـ64-hex (الحجم وحده لا يكشف تسميم بنفس الحجم؛ إعادة الhash إلى
     العنوان هي المرجع). مفتاح غير صالح ⇒ throw.
   - `backup.ps1`: الفساد (Corrupt) يرمي دائماً حتى مع `-AllowMissingBlobs` (العَلم
     يغطي المفقود فقط لا المسموم). ويُكتب `manifest.json` بقائمة `files[].sha256` ثم
     `manifest.sha256` (pin)، مع read-back تأكيد بعد الكتابة (لا يُعلن نجاح قبل التحقق
     أن الـpin على القرص مطابق).
   - `restore-drill.ps1`: يفك الختم (إن sealed) ⇒ يتحقق الـpin ⇒ hash كل ملف مقابل
     manifest ⇒ `pg_restore --exit-on-error` ⇒ counts ⇒ schema/structural ⇒ إعادة hash
     كل blob إلى عنوانه. اسم ScratchDb يجب أن يبدأ بـ`tira_restore_` (لا يُسقط database
     حقيقياً بالخطأ).

3. **H-08 — لا نسخ لمحرك Stalwart (مخزن RocksDB المدمج).** لم يكن هناك سكربت ينسخ مخزن
   المحرك (`C:\tools\stalwart\run\data`)؛ ونسخه حياً غير آمن. الإصلاح (قرار «نافذة إيقاف
   + سكربت»):
   - `Test-EngineStopped` (backup-common.ps1): يفشل مغلقاً ما لم تتحقق ثلاثاً: لا process
     باسم stalwart، ولا management listener reachable (TcpClient بمهلة)، وLOCK حر
     (يُفتح بـ`FileShare.None`؛ إن كان محجوزاً ⇒ المحرك حي). أي إشارة حياة ⇒ رفض.
   - `engine-backup.ps1` (جديد): يشتق DataDir من `config.json` (`.path`؛ النسبي يُحل ضد
     EngineRunDir)، يتحقق التوقف (throw إن حي)، يتحقق بنية RocksDB للمصدر
     (`Test-RocksDbStructure`: CURRENT يسمي `MANIFEST-NNNNNN` موجود + IDENTITY +
     `OPTIONS-*`)، ينسخ config + `data/` + tls اختيارياً إلى stage مؤقت (المصدر
     read-only)، يجرد (`Get-FileInventory`: اسم نسبي بforward-slash + bytes + sha256)،
     يتحقق بنية الـstage، يحزم `engine.tar` (bsdtar)، يكتب `manifest.json`
     (format=`tira-engine-backup/1`، `engine.stop_verified=true`، inventory،
     `archive.sha256`) + `manifest.sha256`، read-back تأكيد، ثم ختم اختياري TIRAS2
     (seal/verify للثلاثة ثم حذف النص الصريح). `try/finally` يزيل الـstage دائماً ويزيل
     الهدف إن لم ينجح (لا أرشيف ناقص). لا يوقف/يشغّل المحرك.
   - `engine-restore-drill.ps1` (جديد): يفك الختم ⇒ يتحقق الـpin ⇒ hash `engine.tar` ⇒
     يفك إلى extract ⇒ لكل ملف في الinventory: اسم regex آمن + رفض `..` + حارس احتواء
     (`$full.StartsWith($extractFull+'\')`) + exists/size/sha256 ⇒ `checked==count` ⇒ لا
     ملفات زائدة غير مجرودة ⇒ بنية RocksDB سليمة. لا يشغّل المحرك ولا يمسّ المخزن الحي.
   - `restore-drill.ps1`: وسيط جديد `-EngineBackupDir` يربط drill المحرك بعد drill
     control-plane (سلسلة واحدة تثبت الطبقتين معاً).

4. **Resolve-Tar — إصلاح portability (اكتُشف أثناء التحقق).** كان `backup.ps1`/
   `restore-drill.ps1` يختار tar عبر `Get-Command tar.exe`؛ على عقدة التطوير يرث PATH
   من Git Bash فيفوز GNU tar 1.35 ويقرأ `C:\...` كمضيف بعيد ⇒ «Cannot connect to C:
   resolve failed». الإصلاح: `Resolve-Tar` يفضّل bsdtar الأصلي في
   `%SystemRoot%\System32\tar.exe` (3.8.9، يفهم مسارات Windows) ثم يرتد لـGet-Command،
   ويرمي إن غاب. اختبار انحدار يزرع `tar.exe` وهمياً (MZ) أول PATH ويؤكد أن Resolve-Tar
   يتجاهله ويعيد System32.

5. **اختبار limiter متذبذب — إصلاح test-only.** كان
   `an_expired_window_starts_over_counting_this_attempt` يستخدم نافذة 1ms + تأكيدَين
   متتاليَين؛ تحت حمل CPU (بعد spawn عمليات كثيرة) يسبق scheduler الخيط >1ms بين الحجز
   والتأكيد التالي فتنقضي النافذة مبكراً ويصبح تأكيد «blocked» ناجحاً خطأً (فشل 1 من 8
   تشغيلات). الإصلاح: نافذة 200ms + sleep 500ms (يتحمل jitter الجدولة ويظل يثبت
   الانقضاء)، مع تعليق يشرح السباق. لا تغيير في سلوك الإنتاج.

**الملفات المتغيّرة/الجديدة:**

- `deploy/rspamd/install.sh` (جديد) و`deploy/rspamd/README.md` (تحديث) — M-19.
- `scripts/backup-common.ps1`: Resolve-Tar، Test-ManagementListener، Test-EngineStopped،
  Get-FileInventory، Test-RocksDbStructure (H-08)؛ Test-ContentAddress،
  Get-BlobPreflightState (M-21) + المساعدات الموجودة.
- `scripts/backup.ps1`: preflight فساد + manifest read-back + Resolve-Tar (M-21).
- `scripts/restore-drill.ps1`: Resolve-Tar + سلسلة `-EngineBackupDir` (M-21/H-08).
- `scripts/engine-backup.ps1` و`scripts/engine-restore-drill.ps1` (جديدان، H-08).
- `scripts/tests/backup-integrity.Tests.ps1` (12 اختباراً) و
  `scripts/tests/engine-backup.Tests.ps1` (جديد، 13 اختباراً).
- `crates/tira-api/src/limiter.rs`: إصلاح الاختبار المتذبذب (test-only).

**البوابات:** `fmt --all` = 0؛ و`clippy --workspace --all-targets --locked -D warnings`
= 0؛ و**223/223 اختباراً** على Windows (0 فشل/تخطّي/ignored، `TIRA_REQUIRE_DATABASE=1`
ضد PostgreSQL حي)؛ واختبار limiter المتذبذب أُعيد 20 مرة ⇒ 20/20 (السباق زال).
و`bash -n deploy/rspamd/install.sh` = 0. واختبارات PowerShell مستقلة (بلا اعتماد):
backup-integrity **12/12**، وengine-backup **13/13**.

**التحقق الحي (أُجري جديداً هذه الجلسة):** ثلاثة harnesses تحت `target/` (مؤقتة،
مستبعدة من اللقطة):

- M-21 e2e (**12/12**): scratch DB + migrations + tenant/domain/mailbox + 3 رسائل
  بblobs حقيقية content-addressed ⇒ backup سليم exit 0 + manifest read-back؛
  restore-drill كامل (re-hash + structural) exit 0 + سلسلة المحرك؛ تسميم blob بنفس
  الحجم ⇒ backup يرفض CORRUPT؛ حذف blob + `-AllowMissingBlobs` ⇒ يظل CORRUPT (العَلم
  لا يغطي السم). أُسقطت scratch DBs.
- H-08 e2e (**13/13**): مصفوفة dummy (backup/drill exit 0؛ رفض listener؛ رفض LOCK
  محجوز؛ corrupt tar ⇒ hash mismatch؛ tamper manifest ⇒ pin mismatch) + **المخزن
  الحقيقي المتوقف** `C:\tools\stalwart\run`: backup exit 0 وdrill exit 0. الجرد
  الحقيقي: **64 ملفاً (config + data/) / 2,617,207 بايت**، `stop_verified=true`، وtls
  مستبعد (المفتاح الخاص)، و`engine.tar` = 2,670,592 بايت
  (SHA-256 `4da81294d2a45135b8d316bc74284bbf5db6a254fa32a10fc83b4cf33e2ab5c1`).
- H-08 sealed (**6/6**): `TIRA_SEAL_PASSPHRASE` من env ⇒ backup `-Seal` exit 0؛ لا يبقى
  إلا `.tira` (حذف النص الصريح)؛ drill يفك ويخرج 0؛ رفض بلا passphrase؛ رفض `.tira`
  ممسوس (GCM auth).

**ملاحظات صادقة:**

- (أ) H-08 على المخزن الحقيقي = إثبات سلامة وبنية (static) على مخزن **متوقف** (backup
  read-only + drill يفك ويعيد hash ويتحقق بنية RocksDB). **لا** يُقلع محرك حي من النسخة
  ولا يُثبت أنه يخدم البريد منها؛ هذا drill تحقق سلامة/بنية لا اختبار إقلاع. والسكربت
  لا يوقف/يشغّل المحرك (نافذة الإيقاف إجراء المشغّل، موثّق في الترويسة).
- (ب) M-21: offsite/schedule/retention **مؤجلة** بقرار المستخدم (سلامة الآن فقط). لا
  نسخ خارج العقدة ولا جدولة ولا سياسة احتفاظ بعد.
- (ج) M-19: `install.sh` syntax-checked في CI لكنه **لم يُشغّل ضد Rspamd حي** على عقدة
  التطوير (لا حزمة ولا Redis هناك)؛ التحقق وظيفي على copies + بوابة configtest، لا
  تثبيت حي. انظر README «ما لم يُثبت هنا».
- (د) Resolve-Tar يصلح اختيار tar على Windows حيث كان GNU tar يفوز PATH ويقرأ `C:`
  كمضيف بعيد؛ يفضّل bsdtar الأصلي في System32، والارتداد لـGet-Command يبقى لمن يملك
  `tar.exe` آخر صالحاً في PATH.
- (هـ) إصلاح limiter test-only (نافذة 200ms)؛ لا يمسّ منطق الحجز الذرّي في الإنتاج
  (المثبت باختبار 64 خيطاً). و`an_expired_window_is_not_blocked` (ينام قبل التأكيد)
  تُرك كما هو (لا سباق فيه).
- (و) الـharnesses تحت `target/` (مؤقتة، قابلة لإعادة الإنتاج، مستبعدة من اللقطة)؛
  تُركت في مكانها (لا حذف بلا نسخة verified). واللقطة مصدر فقط.

**اكتمال موجة 3:** بهذه الوحدة (G) تُغلق كل findings موجة 3 (H-08, H-10, M-04, M-07,
M-10, M-12..M-15, M-17, M-19..M-22) عدا ما أُجّل صراحةً بقرار المستخدم. تحقّقنا من
السجل الملزم هذه الجلسة: H-10/M-20/M-22 في §9.20 (وحدة E)، وM-10 في §9.17 (وحدة B)،
وM-12..M-15 في §9.19 (وحدة D)، وM-17 في §9.22 (Vault)، وM-04/M-07 في §9.23 (وحدة F)،
وH-08/M-19/M-21 في هذه الوحدة (§9.24). كما أن Resolve-Tar يغلق بند المتانة الذي
أُجّل صراحةً في §9.20 ملاحظة (د) (اختيار tar على Windows).

**لم يُبنَ بعد (معلن):** موجة 4 (إصلاحات Low + تقوية CI: إضافة shellcheck/PowerShell-test
job إلى CI، وpermissions/SHA-pinning/cargo audit، وفحص YAML كامل للworkflow).
وoffsite/schedule/retention للنسخ (M-21، مؤجلة بقرار المستخدم). وإدارة مفاتيح/KMS
وإعادة ختم الأرشيفات القديمة تلقائياً (من §9.22) تبقى معلنة.

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_RuntimeHardening_20261110.zip`
(1,449,120 بايت، SHA-256 `10A517818608FD0901D6422CBCC09AE47A1C5A993612B6B5895CC07FDFF67221`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح والاختبارات، مصدر فقط بلا `target/` ولا
`data/` ولا `proof_try_*.exe`): `TiraMail_Snapshot_BackupEngine_20261110.zip`
(1,474,177 بايت، SHA-256 `4B156D11C4CC3B09ADB5E7D1FCBC12949743CCB9B2C5B4E812FA7DFFDA359A2A`).

### 9.25 الموجة 4 / وحدة H — متانة طبقة التخزين: ترتيب الأقفال، ومهلات الـpool، وفحص hex في path_for (10 نوفمبر 2026)

**النطاق:** `tira-store` فقط (طبقة التخزين: PostgreSQL + blobs). لا يمسّ مسار طلبات
API ولا العرض ولا crypto ولا السكربتات. تصلح هذه الوحدة ثلاثة عيوب Low من نوع
«فوز-واضح» ضمن التفويض المستقل «أصلح كل Tira Mail … قوية بدون ثغرات وسلسة»، بلا قرار
معماري خارجي: (1) ترتيب أقفال `FOR UPDATE` لمنع deadlock، (2) مهلات pool كي لا
يعلّق `get()` إلى الأبد، (3) فرض `is_content_key` في `path_for` كدفاع عميق.

**ما أُصلح ولماذا:**

1. **ترتيب الأقفال حتمي — منع deadlock في fan-out.** كان `append_message` يأخذ
   أقفال `FOR UPDATE` على صفوف الصناديق بترتيب `envelope.rcpt_to` كما وصل. تسليمَان
   متزامنان لنفس الصندوقين لكن بترتيب معكوس في المظروف يقفلان A→B وB→A ⇒ دورة
   deadlock؛ يكشفها PostgreSQL فيجهض إحدى المعاملتين بخطأ deadlock، وهو ما يترجم إلى
   451 قابل لإعادة المحاولة ⇒ ارتداد بريد كان يجب قبوله. الإصلاح: فرز `targets` حسب
   `mailbox.id` تصاعدياً قبل حلقة القفل، فيصبح ترتيب الأخذ عالمياً وحتمياً لكل تسليم؛
   المعاملة الثانية تنتظر الأولى بدل أن تشكل دورة. النسخ المرتجعة أصبحت مرتبة حسب
   mailbox id لا حسب موضع المظروف؛ ولا يعتمد أي مستدعٍ على الترتيب القديم (معالج SMTP
   يتجاهل المتجه كلياً `Ok(_) => Ok(())`، والاختبارات إما مستلم واحد أو تساوي
   blob_key). اختبار `fan_out_locks_mailboxes_in_deterministic_order` ينشئ 3 صناديق
   ويبني المظروف بترتيب id تنازلي ثم يؤكد أن النسخ تعود تصاعدية.

2. **مهلات pool — لا تعليق أبدي.** كان `build_pool` يترك كل مهلات deadpool على
   `None`، فـ`get()` قد ينتظر إلى الأبد إذا تعطلت القاعدة أو كانت كل الخانات محجوزة
   باستعلامات بطيئة ⇒ اعتمادية بطيئة واحدة تكوم طابور الطلبات كله حتى تنحشر العملية.
   الإصلاح: بنية `PoolTimeouts` (connect=5s، wait=10s، create=10s، recycle=2s) +
   `pg_config.connect_timeout(...)` + `wait/create/recycle_timeout` على
   `Pool::builder`. واكتُشف أثناء الاختبار أن deadpool يشترط runtime صريحاً متى ضُبطت
   أي مهلة (وإلا فشل البناء بـ«Timeouts require a runtime»)؛ فأضيف
   `.runtime(Runtime::Tokio1)` (الكومة كلها على tokio). هذا الارتداد كشفته اختبارات
   `proof.rs` القائمة (تستدعي `PgStore::connect`) وأُصلح داخل الوحدة نفسها.
   و`build_pool_with_timeouts` تجاوزٌ للاختبار (مهلة wait بالميلي ثانية تثبت السلوك
   سريعاً) ولضبط المشغّل لاحقاً من الإعدادات. اختباران:
   `default_timeouts_bound_every_checkout_path` (بلا قاعدة: المسارات الأربعة مقيدة +
   connect ≤ create)، و`wait_timeout_bounds_a_checkout_when_the_pool_is_exhausted`
   (قاعدة حية: pool بخانة واحدة، حجزها ⇒ get() ثانٍ يفشل بـ`PoolError::Timeout(Wait)`
   خلال <3s، ثم التحرير يعيد الصلاحية).

3. **فحص hex في path_for — دفاع عميق.** كان `path_for` يرفض الاجتياز (`..`، مسار
   مطلق، backslash، أجزاء فارغة) لكنه لا يفرض أن يكون المفتاح عنوان محتوى صالحاً (عيب
   Low، غير قابل للوصول عن بعد اليوم لأن كل المستدعين يمررون مفاتيح content).
   الإصلاح: فرض `is_content_key` (3 shards + 64 lowercase hex + تطابق بادئة الـshard)
   بحيث لا يستطيع `blob_key` معادٍ أو فاسد أن يسمي مساراً اعتباطياً خارج تخطيط
   الـshard. كل مفتاح حقيقي ينتجه `key_for`، فالقيد دفاع عميق بحت فوق حارس الاجتياز.
   حُدّث اختبار `missing_blob_reports_not_found` ليستخدم مفتاحاً صالح التكوين
   (`00/00/0…0`)، وأضيف `path_for_rejects_a_non_content_key_even_without_traversal`
   (7 مفاتيح ليست عناوين محتوى لكنها بلا اجتياز ⇒ InvalidKey).

**الملفات المتغيّرة/الجديدة:**

- `crates/tira-store/src/store.rs`: فرز `targets` حسب `mailbox.id` (ترتيب أقفال حتمي).
- `crates/tira-store/src/db.rs`: `PoolTimeouts` + `build_pool_with_timeouts` +
  `connect_timeout` + مهلات builder + `Runtime::Tokio1`.
- `crates/tira-store/src/blob.rs`: فرض `is_content_key` في `path_for`.
- `crates/tira-store/tests/blob.rs`: تحديث `missing_blob_reports_not_found` + اختبار
  `path_for_rejects_a_non_content_key_even_without_traversal` (جديد).
- `crates/tira-store/tests/storage_safety.rs`: اختبار
  `fan_out_locks_mailboxes_in_deterministic_order` (جديد).
- `crates/tira-store/tests/pool_timeouts.rs` (جديد): اختبارا المهلات.

**البوابات:** `fmt --all` = 0؛ و`clippy --workspace --all-targets --locked -D warnings`
= 0؛ و**227/227 اختباراً** على Windows (223 أساس + 4 جديدة، 0 فشل/تخطّي/ignored،
`TIRA_REQUIRE_DATABASE=1` ضد PostgreSQL حي). الاختبارات الجديدة/المعدلة الخمسة كلها
خضراء (منها اختبار wait ضد قاعدة حية لا تخطّي).

**ملاحظات صادقة:**

- (أ) اختبار ترتيب الأقفال يثبت الترتيب الحتمي (الآلية المانعة للـdeadlock) لا افتعال
  deadlock قسري (يصعب إنتاجه حتمياً في اختبار)؛ الفرز هو النهج القياسي لمنع deadlock.
- (ب) مهلات pool: `connect_timeout` يقيد TCP+المصافحة؛ اختبار wait يستخدم 80ms ليثبت
  التوصيل سريعاً، وقيم الإنتاج 5/10/10/2s. اشتراط runtime صريح عند ضبط المهلات كشفته
  اختبارات قائمة وأُصلح داخل الوحدة (لا ارتداد متبقٍ).
- (ج) فحص hex في `path_for` دفاع عميق (غير قابل للوصول عن بعد اليوم)؛ يشدد العقد كي
  لا يسيء مستدعٍ مستقبلي استخدامه.
- (د) `NoTls` على اتصال القاعدة (عيب Low منفصل) **مؤجل** لقرار المستخدم (ليس ضمن هذه
  الوحدة)؛ وكذلك zeroize، وdown-migrations، وربط OTP (M-05، قرار منتجي).
- (هـ) الـharnesses المؤقتة تحت `target/` (سكربت اللقطة والتحقق) تُركت في مكانها (لا
  حذف بلا نسخة verified)؛ واللقطة مصدر فقط.

**المتبقي من موجة 4 (معلن):** تقوية CI (permissions/SHA-pinning/cargo audit +
shellcheck + job اختبار PowerShell + فحص YAML كامل)، ونظافة أسرار السكربتات
(`stalwart-admin.ps1` temp/env، وسكربتات proof: TrustAllCertificates وكلمات مرور
hardcoded)، وتقليل كشف `/healthz`، والتحقق من فهرس تعبير login. والقرارات المؤجلة
للمستخدم: DB TLS، وzeroize، وdown-migrations، وM-05 OTP.

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_BackupEngine_20261110.zip`
(1,474,177 بايت، SHA-256 `4B156D11C4CC3B09ADB5E7D1FCBC12949743CCB9B2C5B4E812FA7DFFDA359A2A`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح والاختبارات، مصدر فقط بلا `target/` ولا
`data/` ولا `proof_try_*.exe`): `TiraMail_Snapshot_StoreRobustness_20261110.zip`
(1,482,845 بايت، SHA-256 `DB430366CC8922F6AF22AEC810FF448E7D655D284466C9EEABBF843DAA674247`).

### 9.26 الموجة 4 / وحدة I — تصليب أسرار OTP في tira-auth: حجب Debug وفشل الساعة مغلقاً (10 نوفمبر 2026)

**النطاق:** `tira-auth` فقط (`otp.rs` + `AuthError` في `lib.rs`). لا يمسّ مسار طلبات
API ولا العرض ولا التخزين ولا السكربتات. تصلح هذه الوحدة عيبين Low من نوع «فوز-واضح»
ضمن التفويض المستقل، بلا قرار معماري خارجي: (6) `IssuedOtp` يكشّف الرمز الصريح عبر
`Debug`، و(7) `now_unix` بـ`unwrap_or(0)` يفشل مفتوحاً (fail open) عند تعطل الساعة.
قرار `zeroize` (مسح الأسرار من الذاكرة) مؤجل للمستخدم (معلن أدناه).

**ما أُصلح ولماذا:**

1. **حجب الرمز في Debug.** كان `#[derive(Debug)]` على `IssuedOtp` يطبع
   `code: "123456"`؛ فأي `tracing::debug!(?issued)` أو رسالة panic تسرّب رمزاً حياً
   صالحاً للاستخدام مرة واحدة إلى السجلات. الإصلاح: تنفيذ `Debug` يدوي يطبع
   `code: "<redacted>"` ويُبقي `challenge` ظاهراً (وهو آمن: لا يحمل إلا hash
   Argon2id + expiry + عدّاد المحاولات، لا الرمز). أُزيل `Debug` من الـderive وبقي
   `Clone`. اختبار `issued_otp_debug_redacts_the_plaintext_code` يؤكد أن `{:?}` على
   الحمولة لا يحوي الرمز، ويحوي `<redacted>` و`challenge`.

2. **فشل الساعة مغلقاً (fail closed).** كان `now_unix()` يرجع `unwrap_or(0)` إذا
   كانت الساعة قبل epoch أو غير مقروءة ⇒ `is_expired_at(0)` على `expires_at = ttl`
   يعطي `0 >= 300` = false ⇒ الرمز لا ينتهي أبداً (fail open)، و`issue` يبني expiry
   وهمياً. الإصلاح: `now_unix() -> Result<u64, AuthError>` مع خطأ جديد
   `AuthError::Clock`؛ و`issue` يقرأ الساعة مرة واحدة مقدّماً `let now = now_unix()?`
   فيفشل مغلقاً (لا يصدر رمزاً بـexpiry فاسد)؛ و`is_expired()` يفوّض إلى
   `expired_given(now)` الذي يرجع `true` عند `Err` (الساعة المكسورة لا تمدّ عمر
   الرمز أبداً). فصل القرار (`expired_given`) عن قراءة الساعة (`now_unix`) يجعل فرع
   `Err` قابلاً للاختبار حتمياً بلا mock لساعة النظام. اختبار
   `a_broken_clock_fails_closed_not_open`: `Ok` قبل expiry ⇒ غير منتهٍ، `Ok` عند/بعد
   expiry ⇒ منتهٍ، `Err(Clock)` ⇒ منتهٍ (fail closed لا fail open).

**الملفات المتغيّرة/الجديدة:**

- `crates/tira-auth/src/otp.rs`: `Debug` يدوي لـ`IssuedOtp`، `expired_given`،
  `now_unix -> Result`، `issue` ينشر خطأ الساعة، وتحديث `now_unix()` في اختبار
  work-factor، واختباران جديدان.
- `crates/tira-auth/src/lib.rs`: متغيّر `AuthError::Clock` جديد.

**البوابات:** `fmt --all` = 0؛ و`clippy --workspace --all-targets --locked -D warnings`
= 0؛ و**229/229 اختباراً** على Windows (227 أساس + 2 جديدة، 0 فشل/تخطّي/ignored،
`TIRA_REQUIRE_DATABASE=1` ضد PostgreSQL حي). جناح `tira-auth` ارتفع 18 ⇒ 20.

**ملاحظات صادقة:**

- (أ) `IssuedOtp`/`issue` غير مربوطين بمسار HTTP بعد (M-05 قرار منتجي مؤجل)؛ التصليب
  دفاع استباقي كي لا يتسرب الرمز عند الربط، ولا مستدعٍ خارجي يعتمد على `Debug` اليوم
  (تحققنا: الاستخدام محصور في `otp.rs` واختباراته).
- (ب) `AuthError` في `tira-api/src/auth.rs` نوع محلي منفصل (NoStore/Missing/
  Unavailable)؛ إضافة `Clock` إلى `tira_auth::AuthError` لا تمسّ أي match قائم
  (تحققنا: لا match شامل على النوع الأخير، واستخداماه للبناء لا للتفريع).
- (ج) فرع خطأ الساعة لا يمكن افتعاله على ساعة نظام سليمة؛ لذلك فُصل القرار عن
  القراءة ليُختبر `Err` حتمياً. الساعة تقرأ `Ok` على العقدة الحية.
- (د) `zeroize` (مسح الأسرار من الذاكرة عند الإسقاط) **مؤجل** لقرار المستخدم
  (اعتمادية جديدة)؛ ليس ضمن هذه الوحدة. وكذلك DB TLS، وdown-migrations، وM-05 OTP.
- (هـ) الـharnesses المؤقتة تحت `target/` (سكربت اللقطة والتحقق) تُركت في مكانها (لا
  حذف بلا نسخة verified)؛ واللقطة مصدر فقط.

**المتبقي من موجة 4 (معلن):** كما في §9.25 — تقوية CI (permissions/SHA-pinning/
cargo audit + shellcheck + job اختبار PowerShell + فحص YAML)، ونظافة أسرار السكربتات
(`stalwart-admin.ps1` temp/env، وسكربتات proof)، وتقليل كشف `/healthz`، والتحقق من
فهرس تعبير login. والقرارات المؤجلة للمستخدم: DB TLS، وzeroize، وdown-migrations،
وM-05 OTP.

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_StoreRobustness_20261110.zip`
(1,482,845 بايت، SHA-256 `DB430366CC8922F6AF22AEC810FF448E7D655D284466C9EEABBF843DAA674247`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح والاختبارات، مصدر فقط بلا `target/` ولا
`data/` ولا `proof_try_*.exe`): `TiraMail_Snapshot_OtpHardening_20261110.zip`
(1,486,495 بايت، SHA-256 `D0E88B439C03D45A2B1B755640267EF702EECDCD6BFD8A7EAA3B6E07DF945A9A`).

### 9.27 الموجة 4 / وحدة J — نظافة أسرار سكربتات الإثبات + إعادة تحقّق خط الأساس وإصلاح انحدار الدوران (12 نوفمبر 2026)

**النطاق:** `scripts/` (وحدة `proof-common.ps1` المشتركة + خمسة سكربتات إثبات تتبنّاها +
اختبار `proof-hygiene.Tests.ps1`)، و`crates/tira-api/src/proof.rs` و`crates/tira-api/tests/proof.rs`
(إصلاح اختبارات فقط، لا منطق إنتاجي). لا يمسّ مسار طلبات API ولا العرض ولا التخزين ولا
التعمية. تُغلق هذه الوحدة عمل «وحدة J» (نظافة أسرار السكربتات)، وتُنفّذ المرحلة 0 من
الموجة الإلزامية: إعادة تحقّق خط الأساس حيًّا وإصلاح انحدار اكتُشف أثناءها، وتسجيل قرار
هوية الدخول بصدق.

**ما أُصلح ولماذا:**

1. **نظافة أسرار السكربتات (وحدة J).** جُمعت القرارات الخطرة في `proof-common.ps1`:
   `Resolve-ProofPassword` (أولوية: وسيط صريح ← متغيّر بيئة ← فشل مغلق، بلا سرّ مثبّت؛
   الفراغ/المسافات فقط = غياب؛ يحفظ المسافات ذات المعنى في كلمة المرور)، و`Test-LoopbackHost`
   (`IPAddress.IsLoopback` يغطّي `127.0.0.0/8` و`::1` و`[::1]` و`localhost`)، و
   `Get-ProofCertificateCallback` (افتراضي `$null` = تحقّق النظام؛ السماح بتعطيل التحقّق
   opt-in للمضيف الحلقي فقط، ويرفض أي مضيف بعيد)، و`New-ProofSslStream`، و
   `Set-ProofRestCertificatePolicy` (سياسة `ICertificatePolicy` مترجمة لأن
   `Invoke-RestMethod` يُشغّل TLS على خيط .NET عامل بلا runspace PowerShell؛ تُعيد السياسة
   السابقة للاستعادة في `finally`، ومقيّدة بالحلقي). تتبنّى السكربتات الخمسة
   (`bridge/dkim/imap/jmap/smtp-proof.ps1`) هذه الحرّاس؛ كلمة المرور من
   `TIRA_PROOF_PASSWORD` (بيئة، لا argv ولا سرّ مثبّت)، وأُزيل الحرفيّ القديم
   `TiraTest2026` من كل سكربتات الإثبات الستة العليا. 16 اختبارًا سلبيًّا مستقلًّا يُثبت
   السلوك الفاشل-مغلقًا بلا محرك حي.

2. **إعادة تحقّق خط الأساس (المرحلة 0).** أُعيد تشغيل البوابة الكاملة ضد PostgreSQL حي
   بـ`--no-fail-fast`. اكتُشف أن شجرة العمل **لم تكن خضراء**: دوران مفاتيح DKIM في
   12 نوفمبر (نما المنطقة 24⇒26 سجلًّا منطقيًّا، و2⇒4 selectors: ‏`v1-*-20261104` +
   `v1-*-20261112`) كسر **أربعة** اختبارات تُجمّد الأعداد القديمة — اثنان في
   `src/proof.rs` (`parses_the_real_tira_zone`، `ed25519_key_is_measured_in_bytes`) واثنان
   في `tests/proof.rs` (`real_artifacts_produce_a_passing_proof`،
   `weak_rsa_key_fails_the_check_without_a_503`). التشغيل السابق المتوقف-عند-أول-فشل كان
   قد أخفى فشلَي التكامل.

3. **الإصلاح المبدئي (لا تجميد الأعداد الجديدة).** أعداد السجلات/المفاتيح صارت تؤكد
   **أرضية** (`>= 24` سجلًّا، `>= 2` مفتاح) وتتحقّق من **كل** مفتاح منشور (كل rsa
   ‏2048 بت، كل ed25519 ‏32 بايت، وكلها verified)، فيُقبل النمو المشروع عند الدوران
   بينما يظلّ الانكماش/الفساد العرضي مُفشِلًا — وهذا أقوى من المساواة المجمّدة القديمة.
   واختبار «المفتاح الضعيف يُفشل» صار يُضعف **كل** selectors الـRSA (بالصيغتين المفردة
   والمتعددة الأسطر ذات الأقواس، كاشفًا RSA من وسم `k=rsa` لا من اسم selector مثبّت)،
   لأن نقطة الأدلة تُمرّر `dkim.rsa` متى بلغ **أي** selector ‏RSA ‏2048 بت — وهذا سلوك
   دوران صحيح. إضعاف القديم فقط كان يترك الجديد قويًّا فلا يُختبر مسار الفشل. الربط
   الأمني الدقيق (`artifact.active_keys` يُعيد تحقّق كل selector مُعلن نشطًا من المنطقة)
   كان صحيحًا أصلًا ولم يتغيّر.

**الملفات المتغيّرة/الجديدة:**

- `scripts/proof-common.ps1`: الحرّاس المشتركة (جديدة).
- `scripts/tests/proof-hygiene.Tests.ps1`: 16 فحصًا سلبيًّا (جديد).
- `scripts/{bridge,dkim,imap,jmap,smtp}-proof.ps1`: تتبنّى الحرّاس؛ كلمة المرور من البيئة؛
  TLS مقيّد بالحلقي.
- `crates/tira-api/src/proof.rs`: اختبارا وحدة (أرضية + تحقّق كل المفاتيح).
- `crates/tira-api/tests/proof.rs`: اختبارا تكامل (مصفوفة dkim أرضية + تحقّق الكل؛ جراحة
  إضعاف كل RSA).
- `target/unitj-verify.ps1` و`target/unitj-snapshot.ps1` و`target/unitj-snapshot-check.ps1`:
  harnesses مؤقتة (مستبعدة من الأرشيف).

**البوابات:** `fmt --all` = 0؛ و`clippy --workspace --all-targets --locked -D warnings` = 0؛
و**244/244 اختبارًا** على Windows (0 فشل/تخطّي/ignored، `TIRA_REQUIRE_DATABASE=1` ضد
PostgreSQL حي، 32 جناحًا). بوابة سكربتات وحدة J: 20 سكربتًا تُحلّل، 16/16 نظافة إثبات،
6 سكربتات إثبات نظيفة من السرّ القديم. المجموع ارتفع من 229 (§9.26) إلى 244 بفعل العمل
المُتدخِّل في 12 نوفمبر (أبرزه جناح `tira-api` الذي ارتفع 38⇒50 مع اختبارات الإرسال
`send::tests`)؛ **هذه الوحدة لم تُضف اختبارات بل أصلحت 4 قائمة**.

**ملاحظات صادقة:**

- (أ) هذه اللقطة تُصوّر حالة **مختلطة** لا لقطة وحدة J نقيّة بتاريخ 10 نوفمبر: سكربتات
  وحدة J (10 نوفمبر) + دوران منطقة العرض في 12 نوفمبر وما رافقه من تغييرات اختبارات/وثائق/
  تخزين + إصلاح هذه الوحدة لأربعة اختبارات. تاريخ الاسم `20261112` (يوم الالتقاط) وفق
  العُرف، لا `20261110`. لم يُرجَع شيء؛ تغييرات العرض محفوظة كما هي وستُصحّح بصدق في
  المرحلة 1 (#42 تقرير FINAL-REPORT، #44 صدق DNS).
- (ب) **قرار هوية الدخول مسجّل: كلمة مرور فقط (Argon2id).** ‏OTP (`tira-auth/otp.rs`)
  يبقى مكتبة مُختبَرة لكن **غير مربوطة**؛ لا يُبنى 2FA ولا يُلمّح إليه. أي نسخة واجهة/وثيقة
  تُلمّح إلى 2FA تُصحّح في المرحلة 1. ملاحظة §9.26 (د) التي أجّلت ربط M-05 OTP صارت
  محلولة: «كلمة مرور فقط، OTP غير مربوط».
- (ج) دلالة `dkim.rsa` «أي selector قوي يُمرّر» مقصودة وصحيحة للدوران؛ الضمان الصارم يسكن
  `artifact.active_keys` (إعادة تحقّق كل مفتاح مُعلن نشطًا من المنطقة). **لا منطق إنتاجي
  تغيّر في هذه الوحدة** — فقط اختبارات أُصلحت لتطابق المنطقة التي نمت مشروعًا.
- (د) نُسخ الخردة `.txt` في `crates/tira-store/src/` (`store_raw.txt`، `store_raw2.txt`،
  `purge_snip.txt`) مؤكّدة غير حاملة (`لا include_str!` ولا مرجع مسار في أي مكان بـ`crates/`)
  و**محفوظة** في هذه اللقطة؛ حجرها هو المرحلة 1 #43 بعد وجود هذه النسخة الموثّقة (لا حذف
  بلا نسخة احتياطية).
- (هـ) الـharnesses المؤقتة تحت `target/` تُركت في مكانها (قابلة لإعادة الإنتاج، مستبعدة من
  الأرشيف المصدري فقط). اللقطة مصدر فقط (بلا `target/` ولا `data/` ولا `.git/` ولا `*.exe`
  ولا `.env` ولا `*.local` ولا `proof_try_*`).
- (و) محرك Stalwart يبقى **متوقفًا** (عملية منفصلة)؛ مسار الإرسال/SMTP مغطّى باختبارات وحدة
  (`send::tests`، `tira-smtp`) تنجح، لكن لم تُجرَ جولة محرك حيّة في هذا الارتكاز. إعادة
  التحقّق الحي للواجهة/المتصفح تخصّ المرحلة 1 #41.

**المتبقي من الموجة الإلزامية (معلن):** المرحلة 1 — #41 مصالحة نسخة الواجهة مع مسار
الإرسال الحيّ، #42 تصحيح FINAL-REPORT من الحالة المتحقّقة (الإصدار 0.1.0 لا 1.0.0،
و244 اختبارًا)، #43 حجر الخردة + حارس نظافة المصدر، #44 صدق DNS (تعليم مضيف البريد،
تحقّق سلامة الهدف، وسم `uit.ac.ma` عرضًا، تجريد المسار المطلق التنموي). المرحلة 2 —
#45 إتمام الأسرار (حجب Debug لـ`IssuedToken.secret`/`SmtpRelay.password`، تقييد
`SmtpRelay::plaintext` بالحلقي، مسح `PGPASSWORD`)، #46 تقليل كشف `/healthz` + فهرس تعبير
الدخول. وتقوية CI (#28: ‏`permissions: contents: read`، تثبيت actions بـSHA، ‏`cargo audit`،
job على `windows-latest` يُشغّل `.Tests.ps1`، ‏`shellcheck`/`bash -n` لـ`install.sh`،
concurrency، timeout). القرارات المؤجلة للمستخدم: DB TLS، وzeroize، وdown-migrations.
ربط OTP: محسوم «كلمة مرور فقط».

نقطة الرجوع قبل هذه الوحدة (للتراجع عنها): `TiraMail_Snapshot_OtpHardening_20261110.zip`
(1,486,495 بايت، SHA-256 `D0E88B439C03D45A2B1B755640267EF702EECDCD6BFD8A7EAA3B6E07DF945A9A`).
لقطة حالة هذه الوحدة (الكود بعد الإصلاح والاختبارات، مصدر فقط بلا `target/` ولا `data/`
ولا `proof_try_*.exe`): `TiraMail_Snapshot_ScriptHygiene_20261112.zip`
(1,535,221 بايت، 162 مدخلًا، SHA-256 `6CA4FD008852DD61985183834DAE111AE3E826EF4A9038C99699D58EE1C1CD66`؛
فحص تسرّب نظيف، وتحقّق مستقل: الملفات الحرجة موجودة و`CORRECTIONS.md` بالداخل عند 9.26).

### 9.28 الموجة الإلزامية / المرحلة 1 — الصدق: مصالحة الواجهة مع الإرسال الحيّ، تصحيح التقرير، حجر الخردة، وصدق DNS (12 نوفمبر 2026)

**النطاق:** أربعة إصلاحات صدق (#41–#44) لا تغيّر منطق الإنتاج الأمني: نسخة الواجهة
(`crates/tira-frontend`)، وتقرير الحالة (`docs/FINAL-REPORT.ar.md`)، ونظافة المصدر
(`crates/tira-store` + `quarantine/`)، وصدق مولّد/صادرات DNS (`scripts/dns-zone.ps1` +
`scripts/dns-common.ps1` + `deploy/dns/*`). لا يمسّ التعمية ولا التخزين ولا مسار الطلبات.
تُغلق هذه الوحدة المرحلة 1 من الموجة الإلزامية؛ نقطة الرجوع قبلها = ارتكاز المرحلة 0
(`ScriptHygiene_20261112`).

**ما أُصلح ولماذا:**

1. **#41 مصالحة نسخة الواجهة مع مسار الإرسال الحيّ.** زرّ «رسالة جديدة» في الشريط الجانبي
   كان زرّاً معطّلاً بوسم «قريبًا» بينما مسار `/mail/{id}/compose` حيّ ويعمل — أي واجهة
   تكذب على المستخدم وتخالف قاعدة «لا شيء يتظاهر بالعمل». صار رابطاً حيّاً
   (`components.rs:227`: ‏`a class="btn btn--block" href=(format!("/mail/{}/compose", mailbox.id))`)
   يفتح نموذج تحرير حقيقيّاً (`components.rs:324` ‏compose، حقول to/subject/body وPOST إلى
   `/mail/{id}/send`). ومدخل «المُرسَل» صار صادقاً: «لا عرض لمجلد المُرسَل بعد»
   (`components.rs:262`) بدل إيحاء بوجود مجلد. حُدّث اختبار
   `inbox_renders_real_rows_and_links` (`components.rs:989`) ليؤكد وجود رابط compose الحيّ
   وغياب `is-soon`/«قريبًا» (`components.rs:1006-1008`)، وحُدّثت قاعدة التوثيق الثانية في
   `lib.rs:14-17`. **تحقّق حيّ بالمتصفح:** دخول بتوكن قصير الأجل، ثم النقر على «رسالة
   جديدة» انتقل فعلياً إلى `/compose` وعرض نموذجاً عاملاً (لقطتا `target/phase1-inbox.png`
   و`target/phase1-compose.png`)؛ لا `is-soon` ولا «قريبًا» ولا «غير موصول» في الصفحة
   المُصادَقة، والميزات غير المبنيّة مجمّعة بصدق تحت «غير متاح بعد».

2. **#42 تصحيح FINAL-REPORT.ar.md من الحالة المتحقّقة.** التقرير كان يدّعي ما ليس
   متحقّقاً: الإصدار 1.0.0 (الصحيح 0.1.0 من `Cargo.toml:15`)، و«14 فحصاً» (الصحيح 16)،
   و242 اختباراً (الصحيح 244 ثم 245)، وSMTP كأنه يعمل افتراضياً (الصحيح: معطّل افتراضياً
   يُفعّل بـ`TIRA_SMTP_ENABLE=1` على loopback فقط، والمُرحِّل الصادر يرفض الإرسال حتى يُضبط
   `TIRA_SMTP_HOST` بـSTARTTLS إلزامي)، ولمحات 2FA (الصحيح: كلمة مرور فقط Argon2id؛ ‏OTP
   مكتبة مُختبَرة غير مربوطة). صُحّحت §2.2 لتسرد معرّفات الفحوص الـ16 بدقة، و§7 و§8
   لتعداد ما لم يُبنَ بصدق (عرض المُرسَل، ‏HTML آمن، بحث/مجلدات/مهملات، مرفقات، دورة حياة
   Stalwart/Rspamd). لا يُحتفظ بأي ادّعاء غير متحقّق.

3. **#43 حجر خردة `.txt` من `src/` + حارس نظافة المصدر.** كانت ثلاث نسخ خردة
   (`store_raw.txt`، `store_raw2.txt`، `purge_snip.txt`) داخل `crates/tira-store/src/` —
   ملفات غير مصدرية في شجرة الكود. نُقلت **كما هي** (لا حذف) إلى
   `quarantine/debris-20261112/` بعد تأكيد وجودها في ارتكاز المرحلة 0 (لا حذف بلا نسخة
   احتياطية). وأُضيف حارس اختبار `source_hygiene.rs`
   (`crates/tira-store/src/source_hygiene.rs:49` ‏`crate_src_dirs_contain_only_rust_sources`،
   مربوط بـ`lib.rs:25`) يفشل إن وُجد أي ملف غير `.rs` تحت `crates/*/src/`. استُخدمت الخردة
   نفسها كعيّنة سلبية: الحارس يفشل بوجودها وينجح بعد حجرها — إثبات سلبي←إيجابي بلا إنشاء
   ملفات خردة جديدة.

4. **#44 صدق DNS: تعليم المضيف + تحقّق سلامة الهدف + وسم demo.** المولّد كان يصدّر مناطق
   عرض كأنها إنتاج: منطقة `uit.ac.ma` (نطاق أكاديمي حقيقي المظهر) تُشير بمضيف البريد
   المستعار `mail.tira.test` بلا أي تحذير، و`zoneFile` في JSON كان مساراً مطلقاً تنمويّاً
   (`C:\...`) غير محمول. أُضيفت دالة نقية `Get-MailHostFinding`
   (`scripts/dns-common.ps1:16`) تُرجع تحذير `mail-host-external` حين يكون هدف MX خارج
   النطاق (مضيف مستعار/مشترك)، أو `mail-host-mismatch` حين يخالف `-MailHost` المطلوب؛
   ويتبنّاها `dns-zone.ps1:354` (وسيط جديد `-MailHost` بـ`:59`، وdot-source بـ`:79`). وصار
   `zoneFile` اسماً أساساً محمولاً (`dns-zone.ps1:472`: ‏`"$name.zone"`). وُسمت المنطقتان
   بترويسة `;` توضّح أنهما عرض/تصدير فقط (tira.test نطاق محجوز RFC 6761؛ ‏uit.ac.ma عيّنة
   لا تخدمها Tira Mail ولا تتحكم بها — لا تُنشر كما هي). وبما أن المحللّات تتجاهل أسطر
   `;` لم يتغيّر عدّ السجلات (26 لـtira.test). أُضيف تحذير `mail-host-external` يدوياً إلى
   `uit.ac.ma.dns.json:30` مطابقاً حرفياً لمخرج الدالة (المحرك متوقف فلا regeneration حيّ)،
   واختبار اتساق يثبت التطابق. 14 اختباراً offline (`scripts/tests/dns-zone.Tests.ps1`)
   كلها خضراء.

**الملفات المتغيّرة/الجديدة:**

- `crates/tira-frontend/src/components.rs`: رابط compose حيّ (:227)، سبب المُرسَل الصادق
  (:262)، اختبار محدّث (:989، :1006-1008).
- `crates/tira-frontend/src/lib.rs`: قاعدة التوثيق الثانية (:14-17).
- `docs/FINAL-REPORT.ar.md`: 12 تصحيحاً (الإصدار/الفحوص/الاختبارات/SMTP/2FA/§2.2/§7/§8).
- `crates/tira-store/src/source_hygiene.rs`: حارس نظافة المصدر (جديد).
- `crates/tira-store/src/lib.rs`: ‏`mod source_hygiene` (:25).
- `quarantine/debris-20261112/{store_raw,store_raw2,purge_snip}.txt`: الخردة محفوظة (نقل).
- `scripts/dns-common.ps1`: ‏`Get-MailHostFinding` نقية (جديدة).
- `scripts/dns-zone.ps1`: ‏`-MailHost` (:59)، dot-source (:79)، استدعاء الفحص (:354)،
  ‏`zoneFile` محمول (:472).
- `scripts/tests/dns-zone.Tests.ps1`: 14 فحصاً offline (جديد).
- `deploy/dns/{tira.test,uit.ac.ma}.zone`: ترويسة `;` demo؛ و`*.dns.json`: ‏`zoneFile`
  أساسي + تحذير `mail-host-external` لـuit (:30).
- `target/phase1-snapshot.ps1` وharnesses التحقق: مؤقتة (مستبعدة من الأرشيف).

**البوابات:** `fmt --all` = 0؛ و`clippy --workspace --all-targets --locked -D warnings` = 0؛
و**245/245 اختباراً** على Windows (0 فشل/تخطّي/ignored، ‏`TIRA_REQUIRE_DATABASE=1` ضد
PostgreSQL حي) — ارتفع 244⇒245 بإضافة حارس النظافة فقط؛ وbuild = 0 بعد تحرير قفل الثنائي
الذي كانت تحمله عملية خادم قديمة. بوابات offline: 14/14 ‏dns-zone، و16/16 ‏proof-hygiene
(من §9.27، لم تتغيّر). **تحقّق حيّ:** متصفح (#41) + endpoint الأدلة
`/api/v1/control-plane-proof` = HTTP 200 وstatus=pass و16/16 فحصاً pass،
و`artifact.record_count` = «26 records in both the artifact and the parsed zone»،
و`generator_findings` = 9 بلا error (tira.test نظيف)، ولا تسريب `zoneFile`/مسار مطلق/منفذ.

**ملاحظات صادقة:**

- (أ) تحذير uit.ac.ma في JSON مُضاف يدوياً ومطابق لمخرج الدالة عبر اختبار اتساق، لأن
  Stalwart متوقف؛ عند تشغيل المحرك وإعادة التصدير سيولّده المولّد تلقائياً.
- (ب) عدّ السجلات لم يتغيّر بترويسات `;` لأن المحللّات (Rust ‏`strip_comment` وPowerShell
  ‏`Get-ZoneRecords`) تتجاهل التعليقات؛ تحقّق حيّاً عبر endpoint الأدلة (26).
- (ج) توكن التحقق الحيّ صدر قصير الأجل (يوم واحد) ثم **أُبطل** بعد التحقق، وأُزيلت ملفاته
  السرّية المؤقتة؛ لا سرّ مثبّت ولا تسريب.
- (د) الخردة محفوظة في `quarantine/` (مضمّنة في اللقطة) ولم تُحذف؛ الحارس يمنع عودتها إلى
  `src/`.
- (هـ) لا منطق إنتاجي أمني تغيّر في المرحلة 1؛ كلها صدق واجهة/وثائق/نظافة/تحذيرات DNS.

**المتبقي من الموجة الإلزامية (معلن):** المرحلة 2 — #45 إتمام الأسرار (حجب Debug
لـ`IssuedToken.secret`/`SmtpRelay.password`، تقييد `SmtpRelay::plaintext` بالحلقي، مسح
`PGPASSWORD`)، #46 تقليل كشف `/healthz` + فهرس تعبير الدخول، و#28 تقوية CI
(`permissions: contents: read`، تثبيت actions بـSHA، ‏`cargo audit`، job على
`windows-latest` يُشغّل `.Tests.ps1`، ‏`shellcheck`/`bash -n` لـ`install.sh`، concurrency،
timeout). ثم مراحل 3–5 (بوابات قرار، بناء، نشر خارجي).

نقطة الرجوع قبل هذه المرحلة (للتراجع عنها): `TiraMail_Snapshot_ScriptHygiene_20261112.zip`
(1,535,221 بايت، SHA-256 `6CA4FD008852DD61985183834DAE111AE3E826EF4A9038C99699D58EE1C1CD66`).
لقطة حالة هذه المرحلة (مصدر فقط، تشمل `quarantine/`، بلا `target/` ولا `data/` ولا `*.exe`):
`TiraMail_Snapshot_Honesty_20261112.zip`
(1,545,526 بايت، 165 مدخلاً، SHA-256 `7A618AF9057B2E74536AC56431BA69E0960D39F5299CA4F0EB53A16076FFC547`؛
فحص تسرّب نظيف، وتحقّق مستقل: الخردة والحارس وسكربتات DNS والتقرير موجودة،
و`CORRECTIONS.md` بالداخل عند 9.27).

### 9.29 الموجة الإلزامية / المرحلة 2 — إتمام الأسرار، تقليل كشف healthz، فهرسة الدخول، وتقوية CI (12 نوفمبر 2026)

**النطاق:** ثلاثة إصلاحات (#45، #46، #28) تُغلق المرحلة 2 من الموجة الإلزامية. ‏#45 يُتمّ
تصليب الأسرار (حجب `Debug`، تقييد SMTP النصي بالحلقي، مسح `PGPASSWORD`)؛ #46 يُقلّص كشف
`/healthz` ويجعل استعلام الدخول sargable (مدعوماً بالفهارس)؛ #28 يُقوّي CI (أقل صلاحيات،
تثبيت actions بـSHA، تدقيق RustSec، ‏job على Windows، lint للصدفة، concurrency/timeout).
لا يمسّ منطق التعمية ولا مسار الطلبات الأمني ولا هجرة قاعدة. نقطة الرجوع قبلها = لقطة
المرحلة 1 (`Honesty_20261112`).

**ما أُصلح ولماذا:**

1. **#45 إتمام الأسرار.** ثلاث ثغرات كشف/إساءة استخدام أُغلقت:
   - **حجب `Debug` للأسرار.** ‏`IssuedToken` كان يشتقّ `Debug` فيطبع `secret` نصاً صريحاً
     في أي log/panic؛ صار `#[derive(Clone)]` فقط (`token.rs:44-45`) مع `Debug` يدوي
     (`token.rs:54`) يستبدل السرّ بـ`<redacted>` ويطبع `record` وحده. وكذلك ‏`SmtpRelay`
     (`client.rs:82`) صار `Debug` يدويّاً (`client.rs:92`) يحجب `password` بـ`<redacted>`
     ويطبع host/port/username/starttls. اختباران سلبيان يثبتان أن `format!("{:?}")` لا
     يحتوي السرّ (`token.rs:326`، `client.rs:208`).
   - **تقييد SMTP النصي بالحلقي.** ‏`SmtpClient::plaintext` (`client.rs:52`) كان يبني
     ناقلاً نصاً صريحاً لأي مضيف؛ صار يرفض غير الحلقي قبل بناء النقل برسالة تُرشد إلى
     STARTTLS، عبر مصنّف صارم `is_loopback_host` (`client.rs:141`) يقبل `localhost`
     و`127.0.0.0/8` و`::1` فقط. المسارَان المستدعيان (`send.rs:355` deliver_outbound،
     و`send.rs:490` compose) يعالجان الرفض بصدق (خطأ upstream / شاشة compose بخطأ).
     اختبار `plaintext_submission_is_loopback_only` (`client.rs:234`، ‏`#[tokio::test]`
     لأن بناء النقل يُطلق مهمة pool) يثبت قبول الحلقي ورفض البعيد، و`loopback_classification_is_strict`
     (`client.rs:261`) يثبت صرامة المصنّف.
   - **مسح `PGPASSWORD`.** ‏`backup.ps1` كان يترك كلمة مرور القاعدة في بيئة العملية بعد
     النسخ؛ صار يحفظ القيمة السابقة (`backup.ps1:73`) ويستعيدها في `trap` الخطأ
     (`backup.ps1:82`) وفي النهاية الطبيعية (`backup.ps1:391`) عبر دالة نقية
     `Restore-DatabasePassword` (`backup-common.ps1:177`) تُزيل المتغيّر إن لم يكن موجوداً
     سابقاً. ‏`ConvertTo-SafeDatabaseUrl` (`backup-common.ps1:160`) ينقل كلمة المرور من
     argv إلى `PGPASSWORD` (يفكّ ترميز URL). أربعة فحوص hygiene جديدة
     (`backup-integrity.Tests.ps1:149-173`) داخل try/finally يحفظ/يستعيد بيئة المستدعي.

2. **#46 تقليل كشف `/healthz` + فهرسة الدخول.**
   - **تقليل الكشف.** ‏`HealthBody` (`lib.rs:285`) كان يكشف `uptime_secs` و`modules` و`smtp`
     (طوبولوجيا داخلية) لأي caller غير مُصادَق؛ صار حقلين فقط: `status` و`database` — ما
     يحتاجه مسبار liveness حصراً. المعالج `healthz` (`lib.rs:365`) يبني الحقلين ويبقي منطق
     503 عند تعذّر القاعدة. ‏`uptime_secs()` (`lib.rs:279`) بقيت `pub` على `AppState` (API
     عام) لكن لم تعد تُستدعى من المسبار. حُدّث `healthz_reports_ok_r2` (`health.rs:10`)
     ليؤكد غياب الحقول الثلاثة وأن عدد مفاتيح JSON = 2 (`health.rs:46`). وحُدّثت الوثائق
     الحيّة التي كانت تعرض الشكل القديم (`README.md:143`، `DEEP_MAP.ar.md:239`).
   - **فهرسة الدخول (sargable).** ‏`credential_by_address` (`identity.rs:74`) كان يطابق
     `m.local_part || '@' || d.name = $1` — دمج عبر جدولين غير sargable يفرض Seq Scan على
     `mailboxes` و`domains` عند كل دخول. صار يفصل العنوان في Rust بـ`rsplit_once('@')`
     (`identity.rs:78`) ويطابق `WHERE d.name = $1 AND m.local_part = $2` (`identity.rs:90`)،
     وهو مدعوم بالكامل بالفهارس الفريدة الموجودة. عنوان بلا `@` يُرجع `None` بلا خطأ.
     اختبار انحدار `login_lookup_resolves_exactly_and_misses_cleanly` (`persist.rs:311`)
     يثبت الإصابة الدقيقة والغياب النظيف (local/domain خاطئ، بلا `@`، فارغ، وبأحرف كبيرة
     غير مُطبّعة). **لا هجرة ولا فهرس جديد:** الفهارس الفريدة `domains_name_key(name)`
     و`mailboxes_address_key(domain_id, local_part)` تكفي؛ الإصلاح هو إعادة كتابة الاستعلام.

3. **#28 تقوية CI.** أُعيد بناء `.github/workflows/ci.yml` بأربع jobs بدل واحدة:
   - **أقل صلاحيات + تزامن + مهلات.** ‏`permissions: contents: read` على مستوى السير
     (`ci.yml:10`)، و`concurrency` بإلغاء التشغيل المتقادم (`ci.yml:15`)، و`timeout-minutes`
     لكل job (‏30/15/10/15) فلا يشغل معلّق runnerاً ست ساعات.
   - **تثبيت actions بـSHA.** كل `uses` مثبّت بـcommit كامل (40 hex) لا وسم متحرّك، حُلّ
     حيّاً من refs أعلى المنبع: ‏`actions/checkout@11d5960a…` (v4)،
     `dtolnay/rust-toolchain@b725eda5…` (stable، مع `toolchain: stable` صراحةً لأن التثبيت
     بـSHA يُلغي دلالة الاسم)، `Swatinem/rust-cache@d1527c8f…` (v2؛ كان annotated tag ففُكّ
     إلى commit)، و`taiki-e/install-action@cfdb7ed4…` (main).
   - **job تدقيق RustSec.** ‏`audit` (`ci.yml:102`) يُثبّت `cargo-audit` (`ci.yml:118`)
     ويُشغّل `cargo audit` (`ci.yml:121`) فيفشل البناء عند أي advisory معروف.
   - **job الصدفة.** ‏`shell` (`ci.yml:123`) يفحص `bash -n deploy/rspamd/install.sh`
     (`ci.yml:134`) و`shellcheck` (`ci.yml:141`) — المُثبّت يعمل كـroot على عقدة بريد.
   - **job Windows.** ‏`windows-scripts` (`ci.yml:143`) يُشغّل مجموعات `.Tests.ps1` الأربع
     على `windows-latest` بـWindows PowerShell 5.1 (`shell: powershell`)؛ كلها مكتفية ذاتياً
     (لا PostgreSQL ولا Stalwart ولا شبكة) فتُختبَر بالكامل على runner نظيف.

**الملفات المتغيّرة/الجديدة:**

- `crates/tira-store/src/token.rs`: ‏`Clone` بلا `Debug` مشتق (:44-45)، `Debug` يدوي (:54)،
  اختبار الحجب (:326).
- `crates/tira-smtp/src/client.rs`: بوابة الحلقي (:52)، `Debug` يدوي لـ`SmtpRelay` (:92)،
  `is_loopback_host` (:141)، ثلاثة اختبارات (:208/:234/:261).
- `crates/tira-api/src/send.rs`: معالجة رفض النصي (:355، :490).
- `scripts/backup-common.ps1`: ‏`Restore-DatabasePassword` (:177).
- `scripts/backup.ps1`: حفظ/استعادة `PGPASSWORD` (:73/:82/:391).
- `scripts/tests/backup-integrity.Tests.ps1`: أربعة فحوص hygiene (:149-173)؛ 16 إجمالاً.
- `crates/tira-api/src/lib.rs`: ‏`HealthBody` بحقلين (:285)، `healthz` (:365).
- `crates/tira-api/tests/health.rs`: تأكيد الشكل المقلَّص (:10/:46).
- `crates/tira-store/src/identity.rs`: استعلام sargable (:74/:78/:90).
- `crates/tira-store/tests/persist.rs`: اختبار انحدار الدخول (:311).
- `README.md` (:143) و`docs/DEEP_MAP.ar.md` (:239): شكل `/healthz` المقلَّص.
- `.github/workflows/ci.yml`: أربع jobs مُقوّاة (إعادة بناء).
- `target/phase2-snapshot.ps1` وأدوات التحقق (yq/shellcheck) وسجلّاتها: مؤقتة (مستبعدة).

**البوابات:** `fmt --all -- --check` = 0؛ و`clippy --workspace --all-targets --locked -D warnings`
= 0؛ و**250/250 اختباراً** على Windows (0 فشل/تخطّي/ignored، ‏`TIRA_REQUIRE_DATABASE=1` ضد
PostgreSQL حي، 32 ثنائياً/doc-test) — ارتفع 245⇒250 بإضافة أربعة اختبارات #45 وواحد #46.
بوابات offline: ‏`cargo audit` نظيف (242 اعتمادية، 0 advisory، exit 0)؛ و`shellcheck v0.11.0`
على `install.sh` = exit 0؛ و`bash -n install.sh` = سليم؛ ومجموعات PowerShell الأربع = 59/59
(‏backup-integrity 16، engine-backup 13، dns-zone 14، proof-hygiene 16). و`ci.yml` YAML صالح
(تحقّق بـyq v4.54.3): أربع jobs، ‏`permissions: contents: read`، concurrency، مهلات، وكل
`uses` مثبّت بـSHA. **تحقّق حيّ:** تشغيل الخادم ثم `GET /healthz` ⇒ HTTP 200
`{"status":"ok","database":"ok"}` (حقلان، لا uptime/modules/smtp)، ثم إيقاف نظيف؛ و`EXPLAIN`
لاستعلام الدخول الجديد ⇒ ثلاثة Index Scans (`domains_name_key`، `mailboxes_address_key`،
`mailbox_credentials_pkey`) بكلفة 0.71..16.84 بلا أي Seq Scan.

**ملاحظات صادقة:**

- (أ) **لا فهرس/هجرة جديدة لدخول sargable:** الفهارس الفريدة الموجودة تكفي، والإصلاح هو
  إعادة كتابة الاستعلام موثّقة بـ`EXPLAIN` حيّ؛ إضافة فهرس زائد كانت ستكون تكراراً بلا داعٍ.
- (ب) **CI — ما تحقّق محلياً وما لا:** تحقّقت محلياً من صلاحية YAML ومن كل بوابة قابلة
  للتشغيل هنا (audit/shellcheck/bash -n/مجموعات PowerShell). أمّا التنفيذ الفعلي على GitHub
  runners (خدمة postgres على ubuntu، ‏PowerShell 5.1 على windows-latest، تنزيل install-action
  لـcargo-audit) فلم يُتحقّق منه محلياً — مُعلن بصدق لا مُفترض.
- (ج) ‏`cargo audit` نظيف اليوم؛ البوابة ستفشل مستقبلاً عند ظهور advisory في اعتمادية —
  إشارة صادقة تُظهر ولا تُخفى ولا تُتجاهل.
- (د) ‏SHAs حُلّت من refs أعلى المنبع حيّاً عبر GitHub API (لا اختلاق)؛ و`rust-cache` v2
  كان annotated tag ففُكّ إلى commit قبل التثبيت.
- (هـ) **لا 2FA:** الهوية كلمة مرور فقط (Argon2id)؛ ‏OTP مكتبة مُختبَرة غير مربوطة (لم تتغيّر
  في هذه المرحلة)، التزاماً بقرار الموجة الإلزامية.
- (و) أدوات التحقق المؤقتة (yq/shellcheck) وسجلّات البوابة تحت `target/` (مستبعدة من اللقطة،
  قابلة لإعادة الإنتاج)؛ لا سرّ مثبّت ولا تسريب.

**المتبقي من الموجة الإلزامية:** لا شيء — المراحل 0/1/2 مكتملة ومُوثّقة. التالي مراحل 3–5
(بوابات قرار: TLS للقاعدة/zeroize/وصل OTP/مصير SMTP؛ ثم بناء: المُرسَل/compose حيّ/HTML
آمن/بحث/مجلدات/مهملات/دورة حياة Stalwart/Rspamd؛ ثم نشر خارجي) بانتظار توجيه المستخدم.

نقطة الرجوع قبل هذه المرحلة (للتراجع عنها): `TiraMail_Snapshot_Honesty_20261112.zip`
(1,545,526 بايت، SHA-256 `7A618AF9057B2E74536AC56431BA69E0960D39F5299CA4F0EB53A16076FFC547`).
لقطة حالة هذه المرحلة (مصدر فقط، تشمل `quarantine/`، بلا `target/` ولا `data/` ولا `*.exe`):
`TiraMail_Snapshot_SecretsCi_20261112.zip`
(1,554,174 بايت، 165 مدخلاً، SHA-256 `3F5A5FDCDBA94C994A80D1BB789B99C03F8EA5F940E881FBFDD211ECDA567E1A`؛
فحص تسرّب نظيف، وتحقّق مستقل: ‏`ci.yml` وملفات #45/#46 موجودة، و`CORRECTIONS.md` بالداخل عند 9.28).

### 9.30 — الدفعة الأولى: ملكية موارد النسخ والاستعادة، واختبار مسارات الفشل (2026-11-14)

**النطاق والإذن:** تنفيذ الدفعة الأولى التي أجازها المستخدم: لقطة رجوع للشيفرة، اختبارات
معزولة، وإصلاح سلامة موارد النسخ وتجربة الاستعادة. لا تشغيل على البريد الحقيقي، ولا
إيقاف/بدء PostgreSQL أو Stalwart أو خدمة التطبيق، ولا حذف لأي نسخة مستخدم سابقة، ولا Git.
سمح المستخدم صراحةً بـ`RemoteSigned` لعمليات PowerShell المحلية فقط بعد أن منع Windows
التشغيل؛ لم تتغيّر سياسة الجهاز/المستخدم الدائمة. سكربتات الإنتاج شُغّلت في هذه الدفعة
داخل حواجز الاختبار فقط، وليس مباشرةً على أدوات PostgreSQL أو بيانات التشغيل.

**تصحيح لتفسير §9.29:** نجاح اختبارات المساعدات آنذاك لم يثبت سلامة دورة النسخ كاملة.
استعادة `PGPASSWORD` بعد التنظيف في `trap` كانت قابلة للتجاوز إذا رمى التنظيف استثناءً.
ووصف المراحل السابقة بالمكتملة لا يعني أن المشروع خالٍ من العيوب أو جاهز للإنتاج.
هذه الدفعة تعالج ما ثبت في دورة حياة الموارد، ولا تغلق بقية نتائج التدقيق.

**الإصلاحات وسببها:**

1. **إنشاء وجهات جديدة قبل امتلاك حق تنظيفها.** أُضيف `New-BackupDirectory` إلى
   `scripts/backup-common.ps1`: اسم UTC مع GUID، وإنشاء المجلد النهائي بلا `-Force`.
   لا تعود الدالة بالمسار إلا بعد نجاح الإنشاء؛ لذلك يبقى متغيّر الوجهة لدى المستدعي
   فارغًا إذا فشل اكتسابها. يستخدمها `backup.ps1` و`engine-backup.ps1` لوجهات النسخ،
   ولتجهيز المحرك، وكذلك `restore-drill.ps1` لمجلد العمل. مجرد اختيار اسم، أو تطابق
   البادئة، أو وجود المجلد لا يمنح إذن حذفه. المجلد الأب قد يوجد مسبقًا ولا يُنظّف.
2. **استعادة السر مستقلة عن التنظيف.** استُبدل `trap` في `backup.ps1` بـ`try/catch/finally`.
   استعادة القيمة السابقة لـ`PGPASSWORD` داخل `finally` داخلي مستقل تعمل حتى عندما
   يفشل حذف الناتج الناقص أو إصدار التحذير. الاختبار يتحقق من القيمة داخل عملية
   العامل نفسها، قبل إعادة بيئة الاختبار؛ ففحص بيئة العملية الأم وحدها لا يثبت ذلك.
3. **أخطاء التنظيف ظاهرة ولا تمحو السبب الأصلي.** حذف ناتج ناقص مملوك يستخدم
   `-ErrorAction Stop`. في نسخ المحرك تُحاول إزالة الوجهة الناقصة حتى إذا فشل تنظيف
   التجهيز. تُجمع أخطاء العملية والتنظيف في `AggregateException` مع تضمين رسائلها
   في الرسالة الظاهرة، لأن `ErrorRecord.ToString()` في PowerShell 5.1 لا يعرض تلقائيًا
   تفاصيل الاستثناءات الداخلية. إذا اكتمل أرشيف المحرك وفشل تنظيف التجهيز، يُحفظ
   الأرشيف المكتمل وتفشل العملية بدل حذف النسخة الجيدة أو إخفاء بقايا التجهيز.
4. **قاعدة تجربة موجودة تعني الرفض، لا الحذف وإعادة الإنشاء.** الاسم الافتراضي صار
   `tira_restore_<GUID>`. الاسم الممرّر يجب أن يكون ASCII صغيرًا وأرقامًا/شرطة سفلية،
   بطول إجمالي لا يتجاوز 63 بايتًا؛ الفحص يستخدم `\A` و`\z` لرفض السطر الجديد النهائي
   أيضًا. تُفحص `pg_database` في قاعدة الصيانة `postgres`؛ وجود الاسم أو فشل الاستعلام
   يوقف التجربة قبل فك التشفير/الاستخراج. يبقى نجاح `createdb` هو شرط اكتساب المورد
   إذا تسابق منشئ آخر بعد الفحص، ولا يوجد أي حذف تمهيدي.
5. **تنظيف قاعدة التجربة مرتبط بالإنشاء والهوية.** بعد نجاح `createdb` يُسجّل OID،
   ولا تبدأ الاستعادة إذا تعذّر إثباته. قبل التنظيف يُعاد فحصه؛ اختلافه أو تعذّر
   فحصه يمنع الحذف. `dropdb` لا يستخدم `--force` ولا `--if-exists`، وفشله يفشل
   التجربة. التجربة الفاشلة و`-Keep` يحتفظان بالقاعدة التي أُنشئت ومجلد العمل
   للفحص. أُزيل اقتراح أمر حذف يدوي بالاسم وحده من رسالة الفشل.
6. **لا إعلان نجاح قبل إتمام التنظيف.** رسائل `Backup written` و`Engine backup written`
   و`RESTORE DRILL PASSED` أصبحت بعد `finally`. فشل حذف قاعدة التجربة أو ملفات العمل
   لا يمرّ معه إعلان نجاح. إذا حُذفت القاعدة المملوكة بنجاح ثم فشل حذف ملفات العمل،
   تبقى الملفات وتفشل العملية؛ لا تُدّعى إعادة القاعدة التي حُذفت بالفعل.

**اختبارات تنفيذية جديدة — وليست دليل استعادة حقيقية:**

الملف `scripts/tests/backup-lifecycle.Tests.ps1` يشغّل نقاط الدخول الفعلية، وليس نسخًا
من منطقها أو مطابقة نصية فقط. يستبدل حدود PostgreSQL وtar وفحص توقف المحرك بمحاكاة
معلنة ومغلقة: أي أداة أو SQL غير معروف يفشل، ولا مسار رجوع لتنفيذ أمر حقيقي.
ملفات الأدوات الوهمية غير قابلة للتنفيذ، ومضيفا الاختبار ضمن `.invalid`. إنشاء
الملفات ونسخها وحساب SHA-256 والتحقق من manifest حقيقية، لكنها على بيانات اصطناعية
موسومة تحت جذر مؤقت فريد؛ لا تُعرض هذه البيانات على أنها بريد مستخدم أو نسخة حقيقية.

العمال عمليات PowerShell منفصلة، ولكل منها TEMP/TMP وPGPASSWORD مستقلان. aliases
ثابتة تبقى فعالة بعد dot-source للمساعدات؛ دوالها عامة داخل العامل فقط حتى تتمكن
أوامر الوحدات، مثل `Get-FileHash` الذي يستدعي `Resolve-Path`، من الوصول إليها.
تُرفض المسارات خارج جذر الاختبار وreparse points. التزامن بحواجز أحداث ذات مهلة،
دون sleeps أو polling. لا يُنظّف الجذر حتى تنتهي عمليات الاختبار التي بدأها المشغّل.

تغطي الحالات الـ42: تصادم الوجهة والتجهيز مع بقاء sentinel أجنبي بايتًا ببايت؛
فشل العمل وحذف الناتج المملوك وحده؛ نجاح نسخ اصطناعية ذات hashes صحيحة؛ اجتماع
خطأ العمل وخطأي التنظيف؛ بقاء أرشيف مكتمل مع فشل تنظيف التجهيز؛ استعادة كلمة المرور
الغائبة/الموجودة رغم استثناء التنظيف/التحذير؛ أربعة سيناريوهات تزامن للنسخ والمحرك؛
قاعدة موجودة/فشل استعلام/OID غير صالح/سباق إنشاء؛ فشل restore أو hash أو التقاط OID؛
النجاح و`-Keep` وتغير OID وفشل drop أو تنظيف العمل؛ فرادة الاسم الافتراضي وحدوده.

**النتائج المعتمدة بعد تصحيح بيئة الاختبار:**

| البوابة | النتيجة الفعلية |
|---|---|
| خط الأساس قبل تعديل سكربتات الإنتاج | 59/59 من المجموعات الأربع القائمة |
| backup-integrity بعد الإصلاح | 16/16 |
| engine-backup بعد الإصلاح | 13/13 |
| dns-zone بعد الإصلاح | 14/14 |
| proof-hygiene بعد الإصلاح | 16/16 |
| backup-lifecycle بعد الإصلاح | 42/42 |
| الإجمالي المحلي النهائي | **101/101، صفر فشل** |
| تحليل صياغة جميع `scripts/**/*.ps1` | صفر أخطاء |
| YAML وخطوة backup-lifecycle عبر yq | صالح، والخطوة تطابق الأمر المتوقع |
| نطاق ملفات المصدر مقابل لقطة ما قبل الإصلاح | صفر ملفات محذوفة، وصفر تغييرات خارج النطاق |

واجهت أولى تشغيلات الاختبار أخطاء في نطاق دوال المحاكاة وطول مسارات .NET Framework؛
أُصلحت قبل اعتماد النتائج. لم تُعَدّ هذه الإخفاقات عيوبًا في الإنتاج أو تُخفَ باعتبارها
نجاحًا. جرى تشغيل النسخة النهائية من الاختبار أيضًا على شيفرة ما قبل الإصلاح المستخرجة
من ZIP المتحقق إلى مجلد مستقل، دون استبدال ملفات المشروع الحالي: **9 حالات نجحت
و33 فشلت، exit 1 متوقع**. هذه 33 حالة مخالفة للمتطلبات الجديدة وليست ادعاء وجود 33
ثغرة مستقلة؛ منها حذف sentinel أجنبي، وعدم استعادة `PGPASSWORD`، والحذف التمهيدي
للقاعدة، وإعلان النجاح رغم فشل التنظيف. سيناريوهات التزامن القديمة تتأثر بعبور حد
الثانية، بينما حقن تصادم الإنشاء نفسه حتمي.

أُضيفت مجموعة دورة الحياة إلى job `windows-scripts` في `.github/workflows/ci.yml`
بـ`RemoteSigned`. التنفيذ على GitHub runners لم يحدث هنا؛ صلاحية YAML والبوابات
المحلية لا تساوي تشغيل CI المستضاف. لم تُعد اختبارات Rust في هذه الدفعة ولا تغيّرت
ملفات Rust أو إعدادات التشغيل. راجعت الفروق ومسارات الحذف والأخطاء مباشرةً؛ أما
المراجعة المنفصلة المفوّضة سابقًا فلم تصل نتيجتها، فلا تُحتسب مراجعة مستقلة مكتملة.

**الملفات:** تعديلات `scripts/backup-common.ps1` و`scripts/backup.ps1` و
`scripts/engine-backup.ps1` و`scripts/restore-drill.ps1` و`.github/workflows/ci.yml`،
مع الاختبار الجديد وهذا القسم. أدوات اللقطات/البوابات وتقاريرها في مجلد الجلسة
`C:\Users\HSYSTEM\Documents\Qoder\2026-11-08\982ccb24`، وليست جزءًا من التطبيق:
`batch1-gates.ps1`، `batch1-regression-before.ps1`، `batch1-source-check.ps1`،
`batch1-snapshot.ps1`، وسجلات `batch1-after-*.log` و`batch1-regression-original.log`.

**الحدود المتبقية — لا تُغلقها هذه النتيجة:**

- لا اتصال PostgreSQL حقيقي ولا restore فعلي ولا تشغيل/استعادة Stalwart. ACL والأدوار
  المحذوفة بسبب `--no-owner/--no-privileges`، وصحة تشغيل التطبيق بعد الاستعادة، ما زالت
  تحتاج دفعة منفصلة واختبار تعافٍ حقيقيًا على بيئة مخصصة.
- فحص OID ثم drop عمليتان غير ذريتين؛ كما توجد نافذة بين CREATE والتقاط OID.
  ليست هذه حماية من مدير قاعدة مميّز يستبدل القاعدة أثناء إحدى النافذتين. يجب استخدام
  خادم تجربة معزول. حماية المجلدات تفترض أيضًا آباء محميين من استبدال المسارات/الوصلات
  بواسطة طرف يملك صلاحيات مماثلة؛ GUID و`New-Item` ليسا قفلًا ضد مهاجم على نظام الملفات.
- اتساق PostgreSQL مع blobs ونافذة توقف RocksDB طوال النسخ لم يُعاد تصميمهما؛ فحوص
  العدّ/التجزئة ليست snapshot مشتركة ولا دليلًا على صلاحية إقلاع المحرك.
- فشل تنظيف التجهيز قد يترك نصًا صريحًا؛ يظهر فشل الآن لكنه لا يشفّر البقايا ولا
  يضمن محوًا آمنًا. إصلاحات التشفير/Vault، صلاحيات staging، URI query passwords،
  الإرسال والطوابير والجلسات وربط المحرك ليست ضمن هذه الدفعة.
- لا تعديل على قرار الهوية: كلمة مرور فقط باستخدام Argon2id. ولا ادعاء بخلو المشروع
  من الثغرات أو جاهزيته للنشر لمجرد نجاح هذه الفحوص.

**لقطات الرجوع:** لم تُستخدم سكربتات النسخ الجاري إصلاحها لأخذ لقطة الشيفرة.
تحققت أداة مستقلة من SHA-256 للارتكاز السابق `TiraMail_Snapshot_SecretsCi_20261112.zip`
ومن قراءة مدخلاته الـ165، ثم أنشأت لقطة قبل الإصلاح بإنشاء ZIP حصري لا يستبدل الموجود:

`TiraMail_Snapshot_BackupOwnership_Before_20261113-221453_badcf1c1252641cf86e01b01028a96c6.zip`

165 ملفًا، 1,558,646 بايتًا، SHA-256:
`596B8CE9E0EDFE79749E9DDEAEE9BDF9DC158C602E5DC6BB7C0CCAC32C0B8694`.
التاريخ في اسمها UTC كما أُنشئت فعلًا. لكل مدخل تحقق الحجم والتجزئة، مع إعادة
تجزئة المصدر لرصد تغيّره أثناء الالتقاط. اللقطات مصدر فقط: تستبعد `target/` و`data/`
و`.git/` وملفات البيئة والمفاتيح المحددة والأدوات التنفيذية؛ **ليست نسخًا احتياطية
للبريد أو قاعدة البيانات**. لقطة الإنجاز تُنشأ بعد هذا القسم بالطريقة نفسها؛ ويضاف
إيصال اسمها/تجزئتها بعد التحقق، خارج محتوى ZIP، لتجنب الاعتماد الدائري على بصمتها.

**إيصال لقطة الإنجاز — أُضيف بعد إغلاق الأرشيف والتحقق منه:**

`TiraMail_Snapshot_BackupOwnership_After_20261114-192750_ceda963b5b7146549762eb1e162fccbb.zip`

المجلد: `C:\Users\HSYSTEM\Desktop\project\screenshot mail.ma`.
166 ملفًا، 1,579,448 بايتًا، SHA-256:
`1D954485B69FEF50F79A80D1FFDB2E08331D76FCE4D3038CECE94E1E76D216DC`.
أُعيد فتح ZIP والتحقق من حجم وSHA-256 لكل ملف ومطابقته للمصدر. يحتوي الأرشيف
§9.30 كاملًا حتى الفقرة السابقة، ولا يحتوي هذا الإيصال اللاحق. الشيفرة والاختبارات
داخله هي النسخة التي اجتازت البوابات؛ لا تغيير لاحق عليها في هذه الدفعة.
المقارنة النهائية للمصدر: 6 ملفات قائمة معدّلة + ملف اختبار جديد، ولا ملفات مصدر
محذوفة ولا تغييرات خارج النطاق. المراجعة المستقلة مسجلة كمتابعة مفتوحة (#59)، وليست
شرطًا نُفّذ أو نتيجة نجاح مفترضة.

### 9.31 إزالة quarantine/debris-20261112 (15 نوفمبر 2026)

**السياق:** مراجعة ثابتة قبل الهاكاثون كشفت وجود المجلد
`quarantine/debris-20261112/` (3 ملفات، 102,478 بايتًا) خارج `.gitignore`، وباسم
يوحي بمخلفات قد تحتوي بيانات بريد حقيقية. المشروع ليس مستودع git، فالقاعدة وحدها
لا تحمي عند ضغط المجلد أو مشاركته.

**نتائج الفحص الآلي (قراءة فقط، قبل أي تعديل):**

| الملف | الحجم | SHA-256 كامل | PII | المحتوى |
|---|---|---|---|---|
| `purge_snip.txt` | 10,082 B | `B94D6104D3D6F78A6F1D01063EAC098D6E83D50848E53C7FE5C250E9D088CD80` | 0 | مقتطف `Select-String` من `store.rs` (كود فقط) |
| `store_raw.txt` | 46,198 B | `9727FEFAF2419E73D14D84CEC3C9FD9D0C51952513472F626CB1185B0CBEC017` | 0 | نسخة كاملة من `store.rs` (562 سطرًا) |
| `store_raw2.txt` | 46,198 B | `9727FEFAF2419E73D14D84CEC3C9FD9D0C51952513472F626CB1185B0CBEC017` | 0 | نسخة مطابقة بايت-ببايت للأول |

- صفر عناوين بريد، صفر ترويسات رسائل (`From:`/`Subject:`/`Message-ID`)، صفر أحرف
  عربية، صفر مفاتيح خاصة، صفر مفاتيح blobs بصيغة hex.
- `sqlData=13` = استعلامات SQL داخل الكود المصدري، وليست صفوف بيانات.
- `store_raw` و`store_raw2` متطابقان تمامًا، وكلاهما يختلف عن `store.rs` الحالي
  (sha256 مختلف) → محتوى عفا عليه الزمن ولا قيمة أرشيفية له.

**القرار:** حذف المجلد بالكامل، لا اكتفاء بـ`.gitignore`، لأن المشروع ليس مستودع
git، والتكرار بلا قيمة، ووجود مجلد `debris` في حزمة الهاكاثون يُفسَّر كضعف هندسي.

**الإجراءات المُنفَّذة:**

- [x] نسخة استرجاع قبل الحذف:
  `D:\TiraMail_Snapshot_20261115-QuarantineFix.zip` — 223 مدخلًا، 1,618,006 بايتًا،
  SHA-256: `9B8F310CB3CA37C1B465D0A4BC8027D818819B358727F4D7E654258BF82DF41A`.
  تستبعد `target/`، وتضمّن `quarantine/` قبل الحذف و`data/` (بريد العرض)، فهي
  **نسخة استرجاع سرية لا تُشارك ولا تُرفع إلى خدمة سحابية غير مشفّرة**.
- [x] حذف `quarantine/debris-20261112/` بالكامل (`Test-Path` يعيد `False`).
- [x] إضافة `quarantine/` إلى `.gitignore` مع تعليق يوضح الغرض.
- [x] تسجيل هذا القسم في `CORRECTIONS.md`.

**الدرس:** أي مجلد مؤقت أو `debris` يُحذف فور انتهاء الغرض منه، ويُضاف إلى
`.gitignore` من البداية. الفوضى التنظيمية ليست مشكلة جمالية فقط: في سياق CNDP /
القانون 09-08، وجود مجلد باسم `quarantine` في حزمة تُسلَّم للجنة يفتح سؤالًا لا
يجب أن يُفتح أصلًا.

### 9.32 لقطة source-only مطابقة لعرف §9.30 (15 نوفمبر 2026)

**السياق:** بعد حذف `quarantine/` بقي على `D:` أرشيف استرجاع كامل يضمّن `data/`
(بريد العرض) والـdebris المحذوف. هذا مناسب للرجوع، لكنه يخالف عرف §9.30: اللقطات
مصدر فقط، وتستبعد `target/` و`data/` و`.git/` والبيئة والمفاتيح. لذلك أُنشئت لقطة
ثانية نظيفة تصلح للمشاركة والمراجعة.

**اللقطة:**

`D:\TiraMail_Snapshot_20261115-SourceOnly.zip`

207 مدخلًا، 1,598,581 بايتًا، SHA-256:
`AF02C292C0F579099C3DF0DC1919F99975BB9A4F3F4675EE0D35F169F9FB9046`.

**الاستبعادات المتحقَّق منها (0 مدخل لكل منها):** `target/`، `data/`،
`quarantine/`، `.env` و`.env.*`، `*.key`، `*.pem`، `*.local`.

**التحقق من المحتوى:** `Cargo.lock`، `crates/tira-api/src/proof.rs`،
`crates/tira-store/src/proof.rs`، و`crates/tira-store/tests/rls.rs` كلها داخل
الأرشيف. الأرشيف الكامل (1,618,006 بايتًا) يبقى **سجلًا سريًا فقط** على `D:`، ولا
يُشارك ولا يُرفع إلى خدمة سحابية غير مشفّرة.

### 9.33 تحديث أرقام README.md (15 نوفمبر 2026)

**السياق:** فحص ثابت كشف أن أرقام `README.md` تعود إلى حالة أقدم (87 اختبارًا،
24 سجل DNS، 7 فحوص)، بينما تجاوزها عمل P3 وproof endpoint.

**القياسات الفعلية (15 نوفمبر 2026):**

| القياس | الأمر | النتيجة |
|---|---|---|
| الاختبارات | `cargo test --workspace --no-fail-fast` مع `TIRA_REQUIRE_DATABASE=1` | **250 ناجحًا، 0 فاشل، 0 متجاهل**، exit `0`، 32 ملف اختبار |
| اختبارات تتصل بالقاعدة | إسناد ملفات الاختبار | **48** عبر 8 ملفات |
| سجلات `tira.test` | `deploy/dns/tira.test.zone` + `.dns.json` | **26 سجلًا، 4 مفاتيح نشطة، 9 فحوص** |
| سجلات `uit.ac.ma` | `deploy/dns/uit.ac.ma.zone` + `.dns.json` | **18 سجلًا، مفتاحان نشطان، 8 فحوص** |
| حجم binary (debug) | `target/debug/tira-api.exe` | **156.9 MiB** |

توزيع الـ48: `persist` 5، `rls` 7، `storage_safety` 4، `pool_timeouts` 2،
`read` 9، `proof` 10، `session` 3، `bridge` 8.

**التغييرات في `README.md`:**

- سطر 100–101: 87 ← **250**، و«20» ← **48** اختبارًا في ملفات تتصل بالقاعدة.
- سطر 106–108: قائمة ملفات اختبار القاعدة أُعيد بناؤها بالأعداد المقاسة.
- سطر 319: تاريخ التحقق النهائي 6 نوفمبر ← **15 نوفمبر 2026**.
- سطر 329: **250/250** (48 على PostgreSQL).
- سطر 333: DKIM ← **4 مفاتيح (RSA×2 + Ed25519×2)**.
- سطر 334: DNS ← **9 فحوص و26 سجلًا لـtira.test، و8 و18 لـuit.ac.ma**.

**ملاحظة:** `README.md` لا يذكر حجم الـbinary؛ الرقم 156.9 MiB يخصّ
`docs/DEEP_MAP.ar.md` ويُصحَّح في §9.35.

**الدرس:** رقم في التوثيق بلا أمر قياس مرافق يتحوّل إلى أسطورة. كل رقم في README
صار الآن مقترنًا بأمر أو ملف مصدر يمكن إعادة إنتاجه.

### 9.34 تثبيت إصدار Rust عبر rust-toolchain.toml (15 نوفمبر 2026)

**السياق:** لم يكن هناك `rust-toolchain.toml` ولا `rust-version` في `Cargo.toml`،
فكل جهاز وكل تشغيل CI يستخدم ما يصادفه من `stable`. هذا يجعل البناء غير قابل
لإعادة الإنتاج، ويسمح بأن يمرّ اختبار محلي على مُصرِّف مختلف عن CI.

**الإصدار الفعلي المُقاس:**

```
rustc 1.99.1 (58cf8f9c3 2026-10-13)
cargo 1.99.1 (5f94df478 2026-08-27)
```

**عائق حقيقي كُشف أثناء التثبيت:** `rustup` كان يحلّ `1.99.1` إلى
`x86_64-pc-windows-msvc` (الـdefault host)، وهذا الجهاز بلا `link.exe`. أُثبت
الفشل بترجمة برنامج فارغ: `linker link.exe not found`. في المقابل نجح الربط مع
`stable-x86_64-pc-windows-gnu`. تثبيت الإصدار وحده كان سيكسر البناء المحلي بصمت
(يمرّ `cargo check` لأنه لا يربط، ويفشل `cargo test`/`build`).

**الإجراءات المُنفَّذة:**

- [x] `rustup set default-host x86_64-pc-windows-gnu` (إعداد على مستوى الجهاز،
  قابل للعكس بـ`rustup set default-host x86_64-pc-windows-msvc`). هذا يطابق ما
  يفترضه `scripts/dev.ps1` أصلًا (GNU + MinGW، بلا Visual Studio).
- [x] `rustup toolchain install 1.99.1 --profile minimal --component rustfmt
  --component clippy` → `1.99.1-x86_64-pc-windows-gnu`.
- [x] إنشاء `rust-toolchain.toml`:

  ```toml
  [toolchain]
  channel = "1.99.1"
  components = ["rustfmt", "clippy"]
  profile = "minimal"
  ```

- [x] تحديث `.github/workflows/ci.yml`: `toolchain: stable` ← **`1.99.1`** في
  وظيفتي `test` و`audit`. الأكشن يصدّر `RUSTUP_TOOLCHAIN`، وهو يتجاوز
  `rust-toolchain.toml`؛ لذلك الرقمان يجب أن يُرفعا معًا، وإلا اختبر CI مُصرِّفًا
  غير الذي يختبره المطوّر محليًا.

**التحقق:**

```
rustup show active-toolchain
→ 1.99.1-x86_64-pc-windows-gnu (overridden by '...\rust-toolchain.toml')

rustfmt 1.10.0-stable (58cf8f9c35 2026-10-13)
clippy 0.1.99 (58cf8f9c35 2026-10-13)

cargo check --workspace --all-targets
→ Finished `dev` profile ... in 49.04s (exit 0)
```

**الدرس:** تثبيت الإصدار بلا تثبيت الـhost/target/linker وهمُ إعادة إنتاج. على
Windows بلا MSVC، `channel = "1.99.1"` وحدها تكسر الربط؛ الصواب هو تثبيت
الإصدار في الملف، وتثبيت الـdefault host في بيئة الجهاز، وتثبيت الرقم نفسه في
CI حيث يتجاوز الأكشن الملف.

### 9.35 تحديث مراجع `DEEP_MAP.ar.md` (15–16 نوفمبر 2026)

**السياق:** `DEEP_MAP.ar.md` كان يحمل أرقام حالة أقدم: 10220 سطر Rust، 119
اختبارًا، 121.4 MiB، 239 مدخلًا في `Cargo.lock`، و5 ترحيلات SQL. أُعيد قياس كل
رقم بأمر صريح قبل تعديله، ولم يُنسخ رقم من خطة العمل.

**القياسات الفعلية (15 نوفمبر 2026):**

| القياس | الأمر | النتيجة |
|---|---|---|
| أسطر Rust | `Get-Content \| Measure-Object -Line` على 55 ملف `*.rs` | **17554** سطرًا غير فارغ (19345 إجمالًا) |
| أسطر اختبارات التكامل | مجموع `crates/*/tests/*.rs` (12 ملفًا) | **3269** |
| الاختبارات | `cargo test --workspace --no-fail-fast` مع `TIRA_REQUIRE_DATABASE=1` | **250** ناجحًا، 0 فاشل |
| اختبارات PostgreSQL | إسناد ملفات الاختبار | **48** |
| مداخل `Cargo.lock` | `^name = ` | **242** (234 خارجية + 8 محلية) |
| حجم `tira-api.exe` | `Get-Item ... .Length` | **164,589,642 بايت = 156.96 MiB** |
| سطور SQL | 9 ترحيلات في `crates/tira-store/migrations` | **540** سطرًا غير فارغ |
| سكربتات PowerShell | `scripts/*.ps1` | **19** |
| مسارات HTTP | `.route(` في `tira-api/src/lib.rs` | **23** مسارًا |
| سجلات DNS | `deploy/dns/*.dns.json` | **26/9** لـ`tira.test`، **18/8** لـ`uit.ac.ma` |
| اختبارات الواجهة | `cargo test -p tira-frontend -- --list` | **27** |

**التغييرات في `DEEP_MAP.ar.md` (بأرقام الأسطر الحالية):**

- **17:** `10220`/46 ملفًا ← **`17554`/55**.
- **18:** `1897`/8 ملفات ← **`3269`/12**.
- **19:** `119` ← **`250`**؛ و«15 للـ`tira-frontend`» ← **27**.
- **20:** `30` ← **`48`** مع توزيعها: `persist` 5، `rls` 7، `storage_safety` 4،
  `pool_timeouts` 2، `read` 9، `proof` 10، `session` 3، `bridge` 8.
- **21:** `20` ← **21**، والطريقة صُحِّحت من `cargo tree --depth 1` (يعطي 17
  خارجية فريدة) إلى `[workspace.dependencies]`.
- **22:** `239`/`231` ← **`242`/`234`**.
- **23:** `121.4 MiB`/`127,273,094` ← **`156.96 MiB`/`164,589,642`**.
- **25:** `13` ← **`23`** مسارًا، وأُعيد بناء التعداد ليطابق `.route(`.
- **26:** `363`/5 ترحيلات ← **`540`/9** (`0001`–`0009`).
- **28:** `14` ← **`19`** سكربتًا.
- **29:** `24/نطاق` و«7 فحوص» ← **26 لـ`tira.test` و18 لـ`uit.ac.ma`**،
  و**9/8 فحوص** (DKIM بأربعة محدِّدات).
- **34:** `10220` ← **`17554`**.
- **117:** داخل مخطط P3: «24 سجلاً + 7 فحوص لكل نطاق» ← **«26/9 (tira.test) · 18/8 (uit)»**،
  بعرض صندوق مطابق (64 حرفًا).

*§2 خريطة الملفات — أُعيد قياس أسطر كل الصفوف؛ هذه هي التي تغيّرت:*

| السطر | الملف | قبل | بعد |
|---:|---|---:|---:|
| 160 | `tira-api/src/proof.rs` | 1446 | **1592** |
| 161 | `tira-store/src/store.rs` | 454 | **720** |
| 162 | `tira-vault/src/lib.rs` | 375 | **684** |
| 163 | `tira-smtp/src/server.rs` | 369 | **515** |
| 164 | `tira-api/tests/read.rs` | 346 | **352** |
| 165 | `tira-store/tests/rls.rs` | 334 | **496** (+ «4 اختبارات» ← **7**) |
| 166 | `tira-frontend/src/lib.rs` | 296 | **56** (+ تصحيح الوصف) |
| 167 | `tira-api/src/read.rs` | 292 | **229** |
| 168 | `tira-api/tests/proof.rs` | 285 | **425** (+ «6» ← **10**) |
| 169 | `tira-store/src/token.rs` | 260 | **326** |
| 171 | `tira-store/src/bin/tira-token.rs` | 223 | **255** |
| 172 | `tira-vault/src/bin/tira-seal.rs` | 208 | **455** |
| 173 | `tira-smtp/tests/session.rs` | 199 | **349** |
| 175 | `tira-store/tests/persist.rs` | 182 | **334** |
| 177 | `tira-api/src/auth.rs` | 151 | **183** |
| 178 | `tira-api/src/lib.rs` | 136 | **507** |
| 179 | `tira-store/src/blob.rs` | 135 | **312** |
| 180 | `tira-auth/src/otp.rs` | 118 | **261** |
| 181 | `tira-smtp/src/client.rs` | 100 | **254** |
| 182 | `tira-store/tests/blob.rs` | 92 | **226** (+ «5» ← **11**) |
| 183 | `tira-store/src/db.rs` | 78 | **147** |
| 185 | `tira-api/src/main.rs` | 77 | **138** |
| 187 | `tira-api/tests/health.rs` | 54 | **117** (+ «3» ← **5**) |
| 188 | `tira-auth/src/password.rs` | 52 | **103** |
| 189 | `tira-store/src/model.rs` | 46 | **73** |
| 191 | `tira-store/src/lib.rs` | 21 | **32** |
| 192 | `tira-auth/src/lib.rs` | 17 | **20** |

لم تتغيّر: 170 (`tira-module` 231)، 174 (`0004_rls.sql` 191)،
176 (`tira-store/src/proof.rs` 154)، 184 (`message.rs` 77)،
186 (`0001_init.sql` 74)، 190 (`0005_audit.sql` 38)، 193 (`smtp/src/lib.rs` 12).

*بقية الملف:*
- **195:** `1446` ← **`1592`**.
- **327:** «20 مباشرة، 239 مدخلًا» ← **«21 معلنة، 242 مدخلًا»**.
- **349:** إصدار `base64` `0.23.1` ← **`0.22.1`** (وصف «فك ترميز DKIM SPKI» كان
  صحيحًا أصلًا؛ الخطأ كان في الإصدار: `0.23.1` نسخة شفافة لا نسختنا).
- **350:** صف `native-tls` `0.2.18` ← **`rustls` 0.23.46 / `ring` 0.17.14**.
  `native-tls` غير موجود في `Cargo.lock` إطلاقًا.
- **352–353:** أُضيف توضيح أن `rustls`/`ring` يدخلان عبر `lettre` لا عبر
  `[workspace.dependencies]`.
- **358:** `118.8 MiB` ← **`156.96 MiB`**.
- **360–364:** أُعيدت كتابة فقرة الخطر: كانت تقول إن `native-tls` اختير لأن
  `ring` يحتاج مترجم C، وهذا يعاكس `Cargo.toml` الذي يوثّق اختيار `rustls`
  (ring-backed) بدل `native-tls`. الفقرة الآن تصف القرار الحقيقي وثمنه (`cc`).
- **380 (G9):** «`rls.rs`، 4 اختبارات» ← **7**.
- **435:** `119/119` ← **`250/250`** («والآن الـworkspace كله»).
- **709–711:** أُضيف تعليق على تحقق 7 نوفمبر المؤرَّخ: بقي `109/109` ثم
  `119/119` كسجل تاريخي، مع ذكر **250/250** كعدد حالي.
- **810/833/884:** `176/176` بقيت كسجل 8 نوفمبر، مع تعليق «العدد الحالي
  **250/250** — §1». لم تُستبدل، حفاظًا على التسلسل الزمني.

**انحرافات عن خطة العمل (بشفافية):**

1. `DEEP_MAP.ar.md` **لا يحتوي `87/87`**. الأرقام القديمة للاختبارات كانت
   `119/119` (سطرا 435 و710) و`176/176` (ثلاثة أقسام مؤرَّخة). عُدِّلت الأولى،
   وعُلِّقت الثانية بدل محوها.
2. صف `base64`: الافتراض أن الوصف قديم غير صحيح؛ الوصف كان محدَّثًا، والإصدار
   هو ما كان قديمًا.
3. أرقام قديمة إضافية اكتُشفت وأُصلحت خارج القائمة الرباعية: ترحيلات SQL
   (5←9)، مداخل `Cargo.lock` (239←242)، سكربتات PowerShell (14←19)، مسارات HTTP
   (13←23)، الاعتماديات المعلنة (20←21)، وصف صف الواجهة، وفقرة `native-tls`
   التي كانت تناقض `Cargo.toml`.
4. لم يُغيَّر حجم `stalwart.exe` (164,546,560 بايت) لأن الملف خارج المستودع
   ولم يكن متاحًا للقياس.

**حادثة تقنية أثناء التنفيذ:** التعديل الجماعي عبر
`[IO.File]::WriteAllLines` حوّل الملف من LF إلى CRLF (كل الأسطر). كُشف فورًا
وأُعيد إلى **LF فقط** بـ`UTF8Encoding($false)`. التحقق النهائي: `crlf=0`،
`lf=889`، `bom=False`.

**التحقق من عدم وجود تغييرات غير مقصودة:** نسخة `D:` غير متاحة الآن (القرص غير
موصول)، لكن وُجدت لقطة أحدث على `C:`:
`TiraMail_Snapshot_RestoreAccess_Before_20261114-200241_0ffe495e…zip`.
استُخرج منها `docs/DEEP_MAP.ar.md` (69,034 بايت، 884 سطرًا) وقُورن بالنسخة الحالية
بخوارزمية LCS: **115 سطرًا متغيّرًا (55 محذوفًا + 60 مضافًا)**، وكلها مطابقة
للتغييرات المذكورة أعلاه؛ لا سطر غير مقصود. الفرق الصافي **+5 أسطر**: 3 لملاحظة
`rustls`، و2 لتغليف تعليق 250/250.

**الدرس:** الملف الذي يوصف بأنه «خريطة حيّة» يجب أن يُقاس كالاختبارات: كل رقم
معه أمر إعادة إنتاج. والأخطر من الرقم القديم هو **وصف قرار قديم** (`native-tls`)
يبقى يشرح اختيارًا لم يُتخذ أصلًا.

### 9.36 إزالة الإحالة المكسورة إلى `CNDP-ACCESS-AUDIT.ar.md` (16 نوفمبر 2026)

**السياق:** `crates/tira-store/migrations/0005_audit.sql` كان يشرح سياسة التقليم
في تعليق من ثلاثة أسطر، ويُحيل القارئ إلى `docs/CNDP-ACCESS-AUDIT.ar.md`. الوثيقة
**غير موجودة** في المستودع ولا في أي لقطة متاحة، فكان التعليق يرسل القارئ إلى
طريق مسدود. الإحالة في تعليق ليست توثيقًا إذا كانت تشير إلى لا شيء.

**البحث قبل التعديل:** بُحث عن السلسلة `CNDP-ACCESS-AUDIT` في `docs/`،
و`crates/`، و`scripts/`، وفي كل ملفات `*.md`. النتيجة: **مرجع حيّ واحد فقط**،
وهو الأسطر 16–18 من `0005_audit.sql`. لم يكن هناك مرجع ثانٍ في `README.md` ولا
في `DEEP_MAP.ar.md` ولا في أي ترحيل آخر.

**القرار (الخيار ب):** إزالة الإحالة من SQL، وتوثيق السياسة هنا. لم تُنشأ وثيقة
`CNDP-ACCESS-AUDIT.ar.md` بديلة: إنشاء وثيقة لأن كودًا يشير إليها هو تغطية
(cover-up) لا توثيق، والوثيقة الحقيقية تحتاج قرارًا وسياقًا لا سطر تعليق.

**المحذوف (الأسطر 16–18 قبل التعديل):**

```sql
-- Retention: 12 months hot, then aggregated and dropped. The rule lives in
-- `docs/CNDP-ACCESS-AUDIT.ar.md`; a scheduled deletion is an operational task,
-- not a migration, because a migration must never silently delete evidence.
```

**البديل (الأسطر 16–18 بعد التعديل):**

```sql
-- Retention: 12 months hot, then aggregated and dropped. Trimming is an
-- operational task (by hand or from cron), never a migration step: a
-- migration must not delete evidence silently.
```

**ملاحظة لغوية:** تعليقات `0005_audit.sql` كلها بالإنجليزية، فكُتب البديل
بالإنجليزية حفاظًا على اتساق الملف. المضمون هو المطلوب حرفيًا؛ اللغة تتبع
الملف لا الرسالة.

**السياسة (المرجع الآن: هذا القسم + `COMMENT ON TABLE`):**

- **12 شهرًا** في التخزين الساخن (`hot`)، ثم تجميع (`aggregate`) وحذف.
- التقليم **مهمة تشغيلية** تُنفَّذ **يدويًا أو عبر cron**، وليس خطوة ترحيل.
- **لا حذف تلقائي داخل migration**، لأن الحذف الصامت لا يمكن تدقيقه لاحقًا.
- `COMMENT ON TABLE audit_log` في السطرين 37–38 يحمل نفس الحكم بصيغة مختصرة
  («CNDP 09-08: keep 12 months, then aggregate»)، فلا تعارض بين التعليق
  والـschema.

**التحقق:**

| الفحص | قبل | بعد |
|---|---|---|
| أسطر الملف | 42 | **42** (3 محذوفة + 3 مضافة) |
| البايتات | 2106 | **2064** (−42) |
| `bom` | False | **False** |
| `crlf` | 0 | **0** |
| مراجع حيّة في `crates/` و`scripts/` | 1 | **0** |
| SHA-256 | `E1E169AD…5329CE9` | `21294CFB…B47B3E1F` |

`Compare-Object` بين النسخة الاحتياطية والنسخة الجديدة أظهر **3 أسطر محذوفة
و3 أسطر مضافة فقط**؛ كل عبارات SQL (`ALTER TABLE`، `CHECK`، الفهارس،
`COMMENT ON`) بقيت بايتًا ببايت. النسخة الاحتياطية قبل التعديل:
`%TEMP%\opencode\fix5-0005_audit.sql.bak`.

**ملاحظة على القياس:** لم تبقَ للسلسلة `CNDP-ACCESS-AUDIT` إحالة حيّة. كل
ظهور لها الآن محصور في هذا القسم من `docs/CORRECTIONS.md` (السياق، والبحث،
والقرار، ونص السطر المحذوف، وهذه الملاحظة) — أي سجل تاريخي لما أُزيل، لا رابط
يتبعه قارئ. أي بحث في `crates/` أو `scripts/` يعطي **صفرًا**.

**الدرس:** كل إحالة في تعليق هي وعد للقارئ. الوعد الذي لا يمكن الوفاء به أسوأ
من الصمت، لأن القارئ يظن أنه أمام وثيقة مفقودة بينما هو أمام سياسة مكتوبة في
مكان واحد فقط.

### 9.37 تصحيح ادعاء التشفير في سكربتات النسخ: `AES-GCM` ← `ChaCha20-Poly1305` (16 نوفمبر 2026)

**السياق:** `scripts/engine-backup.ps1` كان يصف ختم النسخة بـ«(TIRAS2, AES-GCM)».
الخوارزمية في الكود ليست AES-GCM إطلاقًا، بل **ChaCha20-Poly1305**. الوصف كان
يعطي المدقّق ثقة زائفة: يظن أن الأرشيف محمي بمعيار AES-GCM (المألوف لدى لجان
التدقيق) بينما هو محمي بمرشّح مختلف تمامًا.

**مصدر الحقيقة (بأرقام الأسطر):**

| الملف | السطر | الدليل |
|---|---:|---|
| `crates/tira-vault/src/lib.rs` | 42 | `use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};` |
| `crates/tira-vault/src/lib.rs` | 104، 165 | `ChaCha20Poly1305::new(...)` في `seal` و`seal_stream` |
| `crates/tira-vault/src/lib.rs` | 13، 53 | صيغة البث الحالية `TIRAS2` (`STREAM_MAGIC_V2`) |
| `crates/tira-vault/src/lib.rs` | 30–32 | `TIRAS1` **إرث يُقرأ فقط**، «never written» |
| `crates/tira-vault/src/lib.rs` | 142 | `seal_stream` «Encrypts a stream … in the **TIRAS2** format» |
| `crates/tira-vault/src/bin/tira-seal.rs` | 28، 342 | «stream-encrypt Tira backups (ChaCha20-Poly1305 + Argon2id)» ثم نداء `seal_stream` |
| `crates/tira-vault/Cargo.toml` | — | `description = "… (ChaCha20-Poly1305 + Argon2id)"` |
| `Cargo.toml` | 46–47 | `chacha20poly1305 = "0.10"`، `argon2 = "0.5"` |
| `Cargo.lock` | — | حزم `aes*`: **0**؛ `chacha20poly1305`: 1؛ `argon2`: 1 |

**تصحيح فرضية الخطة (بشفافية):** افترضت خطة العمل أن `TIRAS2` قد يكون «وهمًا»
وأن الصواب هو `TIRAS1, ChaCha20-Poly1305`. القياس يعاكس ذلك:

- `TIRAS2` **حقيقي، وهو صيغة الكتابة الحالية** (`STREAM_MAGIC_V2`، و`seal_stream`
  يكتبه، و`tira-seal` ينادي `seal_stream`).
- `TIRAS1` **لا يُكتب أبدًا**؛ يُقرأ فقط لفتح أرشيفات قديمة.

لذلك **أُبقي `TIRAS2` وصُحّح اسم الشفرة فقط**. كتابة `TIRAS1` كانت ستستبدل خطأً
بخطأ جديد، لأنها تصف صيغة لا ينتجها السكربت.

**المحذوف (في `engine-backup.ps1`، السطران 24–25):**

```text
      * Optionally seals every artifact with tira-seal (TIRAS2, AES-GCM) so the
        archive is authenticated at rest, not just integrity-pinned.
```

**البديل (الأسطر 24–26):**

```text
      * Optionally seals every artifact with tira-seal (TIRAS2,
        ChaCha20-Poly1305) so the archive is authenticated at rest, not just
        integrity-pinned.
```

**العيب الثاني المكتشف خارج النطاق المعلن:** أثناء مسح `scripts/` ظهر الادعاء
نفسه في `scripts/backup.ps1` السطر 35: «TIRAS2 AES-GCM authenticates every
file». لم يُطلب هذا الملف في الخطة، لكن تركه يعني شحن وصف خاطئ معروف — وهو
بالضبط ما عاقبه §9.35. صُحّح أيضًا، وهو تعديل سطر تعليق واحد قابل للتراجع
بنسخة `.bak` المذكورة أدناه.

**المحذوف (في `backup.ps1`، الأسطر 35–37):**

```text
      seal the archive (-Seal): TIRAS2 AES-GCM authenticates every file, so any
      tampering breaks the tag. Off-site copies, scheduling and retention are
      deliberately out of scope here (deferred; see CORRECTIONS.md).
```

**البديل (الأسطر 35–38):**

```text
      seal the archive (-Seal): TIRAS2 ChaCha20-Poly1305 authenticates every
      file, so any tampering breaks the tag. Off-site copies, scheduling and
      retention are deliberately out of scope here (deferred; see
      CORRECTIONS.md).
```

**ملاحظة على إعادة التغليف:** الاسم الجديد أطول بـ10 محارف من `AES-GCM`،
فأُعيد لفّ السطور للحفاظ على عرض كتلة التعليق (نحو 80 عمودًا). الكلمات لم
تتغيّر إلا في اسم الشفرة؛ لا معنى جديد ولا محتوى محذوف.

**ظهور تاريخي ثالث (لم يُعدَّل):** في §9.24 (10 نوفمبر 2026) يرد وصف «رفض
`.tira` ممسوس (GCM auth)» في السطر 2173. تلك فقرة مؤرَّخة تصف تجربة جرت فعلًا،
فتُركت كما هي حفاظًا على التسلسل الزمني؛ لكن الوسم هناك من **ChaCha20-Poly1305**
لا من GCM، وهذا القسم هو التصحيح الملزم. السلوك نفسه صحيح ومُنفَّذ (رفض أرشيف
ممسوس)؛ الخطأ في تسمية الوسم فقط. في المقابل، إشارات `AES-256` في §8.4 وفي
`DEEP_MAP.ar.md` السطر 299 **لا تُمسّ**، لأنها تصف حزمة TLS 1.3 في `STARTTLS`
وهي صحيحة كما قيست.

**التحقق:**

| الفحص | `engine-backup.ps1` | `backup.ps1` |
|---|---:|---:|
| البايتات | 14825 ← **14843** (+18) | 16940 ← **16956** (+16) |
| الأسطر (newlines) | 296 ← **297** (+1) | 400 ← **401** (+1) |
| `bom` | False ← **False** | False ← **False** |
| `crlf` | 0 ← **0** | 0 ← **0** |
| تحليل PowerShell (`Parser::ParseFile`) | **0 أخطاء** | **0 أخطاء** |
| SHA-256 بعد | `EB2B5783…C60F0079` | `7F3A8C09…84ADAA68` |

`Compare-Object` أظهر **3 أسطر مضافة مقابل سطرين محذوفين** في الأول،
و**4 مقابل 3** في الثاني — كلها داخل كتلة التعليق، ولا سطر تنفيذي تغيّر.
`Select-String 'AES|GCM'` في `scripts/` بعد التعديل: **0 مطابقة**.
النسخ الاحتياطية: `%TEMP%\opencode\fix6-engine-backup.ps1.bak`،
`%TEMP%\opencode\fix6-backup.ps1.bak`.

**الدرس:** تعليق يسمّي خوارزمية خطأ أخطر من تعليق غائب، لأنه يُنتج «دليلًا»
يُبنى عليه قرار. والفرق بين `TIRAS1` و`TIRAS2` هنا لم يكن تفصيلًا: الأول لا
يكتشف قطعًا محاذيًا للإطار، والثاني يكتشفه. وصف الأرشيف بالصيغة القديمة كان
سيوحي بضعف لا وجود له، ووصفه بـ`AES-GCM` كان سيوحي بمعيار لا نستخدمه.

### 9.38 تعليق الأرقام التاريخية لـDNS في `DEEP_MAP.ar.md` (16 نوفمبر 2026)

**السياق:** البوابة 5 (اتساق الوثائق) طابقت نمط `24 سجلاً|7 فحوص` في
`docs/DEEP_MAP.ar.md` السطرين 699–700، داخل قسم مؤرَّخ «ما أُنجز فعلاً
(6 نوفمبر 2026)». الأرقام كانت صحيحة في 6 نوفمبر (24 سجلاً و7 فحوص لكل نطاق)،
ثم تغيّرت إلى **26/9** لـ`tira.test` و**18/8** لـ`uit.ac.ma` (المقيس في Fix 2،
والموثَّق في §1 من الخريطة: السطر 29 في الجدول، والسطر 117 في المخطط).

**القرار:** تعليق لا محو، بنفس منهج §9.35 مع `109/109` و`119/119` و`176/176`:
يبقى نص 6 نوفمبر كما هو، ويُضاف الواقع الحالي بين قوسين على السطر نفسه.

**المضاف (السطر 700):**

```markdown
  7 فحوص *(الأرقام الحالية: 26 سجلاً و9 فحوص لـ`tira.test`، و18 و8 لـ`uit.ac.ma` — §1)*،
```

**التحقق:**

| الفحص | قبل | بعد |
|---|---:|---:|
| البايتات | 70017 | **70132** (+115) |
| الأسطر (newlines) | 889 | **890** (+1) |
| `bom` | False | **False** |
| `crlf` | 0 | **0** |
| SHA-256 | `9837BFED…64D69F9E` | `C860DBF4…8D97ED1` |

`Compare-Object` أظهر **سطرًا واحدًا استُبدل بسطرين**، ولا تغيير آخر في الملف.
النسخة الاحتياطية: `%TEMP%\opencode\fix7-deep-map.ar.md.bak`.

**ملاحظة صادقة على البوابة نفسها:** النص التاريخي «24 سجلاً» و«7 فحوص» باقٍ
حرفيًا داخل السطرين 699–700، لأن مبدأ «السجلات تُعلَّق لا تُمحى» يفرض بقاءه.
لذلك **نمط البوابة 5 سيطابق هذين السطرين مرة أخرى**، ولا يمكن أن يعطي صفرًا
إلا بمحو السجل التاريخي أو بإعادة صياغة أرقامه. التعليق يغيّر **المعنى**
(لم يبقَ ادعاءً حاليًا) لا **المطابقة النصية**.

البوابة الصحيحة بعد هذا الإصلاح: «صفر مطابقة *غير معلَّقة*»، أي كل مطابقة
يجب أن تجاورها عبارة «الأرقام الحالية». البديل الميكانيكي (تغيير النمط ليستثني
السطرين) مقبول أيضًا، لكنه يجب أن يُكتب صراحةً في تعريف البوابة لا أن يُخفى.

**الدرس:** أداة الاتساق التي تعمل بـ`grep` لا تعرف الفرق بين ادعاء حالي وسجل
مؤرَّخ. إن لم يُكتب هذا الفرق في النص نفسه («الأرقام الحالية: …»)، فسيظل
الـ`grep` محقًّا في الشك، وسيظل القارئ غير قادر على التمييز.

### 9.39 اللقطة النهائية `TiraMail_Snapshot_20261116-HackathonReady.zip` (16 نوفمبر 2026)

**السياق:** بعد نجاح البوابات الخمس على الشجرة النهائية، أُنشئت لقطة تسليم على
القرص الخارجي `D:` (كان موصولًا هذه المرة). قبلها تحققت لقطتا 15 نوفمبر اللتان
كانتا غير قابلتين للتحقق وقت Fix 5 وFix 6.

**الملف:**

| العنصر | القيمة |
|---|---|
| المسار | `D:\TiraMail_Snapshot_20261116-HackathonReady.zip` |
| الحجم | **1,591,288 بايت** |
| SHA-256 | `F36CA269754D3BAA55CE1469C7BA0D1671489B1B6F54A16F0B56EE3B55872C97` |
| المدخلات | **168** ملفًا (168 entry، بلا مجلدات فارغة) |
| الحجم قبل الضغط | 2,920,651 بايت (نسبة الضغط 0.545) |

**المضمّن:** `crates/` (8 حزم، 55 ملف `*.rs`، 9 ترحيلات)، `scripts/` (19 سكربت
+ 8 اختبارات Pester)، `docs/` (16 مستندًا + 15 لقطة QA)، `deploy/`، `.github/`،
`Cargo.toml`، `Cargo.lock`، `rust-toolchain.toml`، `README.md`، `.gitignore`،
`FOLDER_STRUCTURE.md`، وتقريرا BARID.

**المستبعد (بقصد):** `data/` (مخزن البلوبات وبيانات التشغيل)، `target/`،
`.env*`، `quarantine/`.

**التحقق:**

| الفحص | النتيجة |
|---|---|
| استخراج كامل + مقارنة SHA-256 لكل ملف | **168/168، 0 اختلاف، 0 مفقود** |
| أسماء مدخلات بفواصل `\` | **0** (كلها `/`) |
| `data/` · `target/` · `.env` · `quarantine/` | **0 · 0 · 0 · 0** |
| `crates/tira-api/src/proof.rs` | ✅ 59,118 بايت |
| `crates/tira-store/tests/rls.rs` | ✅ 20,274 بايت |
| `rust-toolchain.toml` | ✅ 86 بايت |
| `docs/DEEP_MAP.ar.md` | ✅ 70,132 بايت (890 سطرًا) |
| `docs/FINAL-REPORT.ar.md` | ✅ 15,163 بايت (يحتوي §10) |
| `docs/CORRECTIONS.md` | ✅ 275,980 بايت (§9.31–§9.38) |

**تصحيح تقني أثناء البناء:** المحاولة الأولى استخدمت
`ZipFile::CreateFromDirectory` فأنتجت أسماء مدخلات بفواصل `\` (سلوك .NET Framework
على Windows)، وهي غير محمولة إلى `unzip` على Linux/macOS. أُعيد البناء بـ
`ZipArchive.CreateEntry` وأسماء ذات `/`. البناء الأول (1,589,762 بايت،
SHA-256 `20344003…2A344D82`) استُبدل ولم تبقَ منه نسخة.

**تسلسل التوثيق (بشفافية):** اللقطة تحتوي `FINAL-REPORT §10`، لكنها **لا تحتوي
§9.39 نفسه**، لأن القسم كُتب بعد إنشاء الأرشيف. هذا حد طبيعي: الأرشيف لا يمكن أن
يحتوي بصمة نفسه. من يفتح اللقطة يجد التوثيق حتى §9.38، ثم يقرأ §9.39 من الشجرة
الحيّة.

**التحقق من لقطات 15 نوفمبر (أول مرة يتاح فيها):** بعد توصيل `D:`، ثبت أن
البصمتين المسجلتين في §9.31 و§9.32 صحيحتان:

| اللقطة | البايتات | SHA-256 | مطابقة للسجل |
|---|---:|---|---|
| `TiraMail_Snapshot_20261115-QuarantineFix.zip` | 1,618,006 | `9B8F310C…82DF41A` | ✅ |
| `TiraMail_Snapshot_20261115-SourceOnly.zip` | 1,598,581 | `AF02C292…F9FB9046` | ✅ |

**الدرس:** الأرشيف ليس «نسخة احتياطية» حتى يُستخرج ويُقارن ملفًا بملف. البايتات
والبصمة تثبتان النقل لا السلامة الداخلية. والفحص الذي كشف عيب الفواصل لم يكن
البصمة، بل **فتح الأرشيف وقراءة أسماء مدخلاته** — البصمة السليمة لأرشيف غير
محمول تبقى بصمة سليمة لأرشيف غير محمول.

### 9.40 عقد الإرسال الآلي في Tira — `POST /api/v1/outbound` (17 نوفمبر 2026)

**السياق:** مسار الإنشاء البشري (`POST /mail/{id}/send`) كان *form-first*: يحاول
الـrelay أولًا، ولا يحفظ في `outbound_queue` إلا **الفشل**. هذا سلوك صحيح لبشر
يرى الخطأ ويقرّر، وخاطئ لمتصل آلي (Jami3ati) لا يستطيع قبول «تسليم أو لا شيء»:
إذا سقط الردّ بعد نجاح الإرسال لا يعرف ماذا حدث، وإذا سقط قبل الحفظ فقد القرار.

**القرار المعماري:** توسيع `outbound_queue` القائم لا إنشاء طابور ثانٍ. النتيجة:
سياسة إعادة محاولة واحدة، dead letter واحدة، وسجل تدقيق واحد — للبشر وللآلات.

**الضمانات الخمسة (مبنيّة، لا موعودة):**

| # | الضمان | لماذا هذا الشكل تحديدًا |
|---|---|---|
| 1 | `202 Accepted` لا `200 OK` | `200` يعني «تمّ». لا شيء تمّ عند القبول؛ حُفظ الصف فقط. `202` هو الترميز الوحيد الصادق. |
| 2 | `idempotency_key` إلزامي + `UNIQUE (tenant_id, idempotency_key)` | الشبكة تُسقط الردود بعد النجاح. إعادة الطلب تُعيد **نفس** `id` مع `duplicate: true`، ولا تُنشئ رسالة ثانية. |
| 3 | مجموعة ترويسات مغلقة (4 حقول) | `headers: {}` حرّ = حقن ترويسات (`CRLF`، `Bcc:`، `From:` مزيّف). الأعمدة الصريحة تجعل المجموعة مغلقة وقت الترجمة. |
| 4 | مقارنة `tenant_id` صريحة بعد `get_mailbox` | `get_mailbox` يعمل بدور المالك ويتجاوز RLS؛ المقارنة هي الحدّ الفعلي، لا «تحقق زائد». |
| 5 | `503` عند غياب relay | قبول بريد لا يمكن تسليمه كذب بنيوي. |

**الترحيل `0010_outbound_api.sql` (41 سطرًا):**

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

الفهرس **جزئي** لأن صفوف المتصفح تبقى `NULL`؛ `ON CONFLICT` يذكر المسند
(`WHERE idempotency_key IS NOT NULL`) لأن PostgreSQL يشترطه للاستدلال على فهرس جزئي.

**تغييرات الكود (كلها مقاسة بعد `fmt`):**

| الملف | التغيير | الأسطر الآن |
|---|---|---:|
| `crates/tira-store/migrations/0010_outbound_api.sql` | جديد: 4 أعمدة + فهرس جزئي | 41 |
| `crates/tira-store/src/db.rs` | تسجيل الترحيل | 169 |
| `crates/tira-store/src/model.rs` | 4 حقول في `OutboundQueueEntry` | 90 |
| `crates/tira-store/src/store.rs` | `enqueue_outbound_api` + إرجاع الأعمدة 14–17 | 861 |
| `crates/tira-smtp/src/client.rs` | `build_message_with` + `validate_address` | 383 |
| `crates/tira-api/src/send.rs` | `send_api` + تحقق + `outbound_headers` | 1,270 |
| `crates/tira-api/src/lib.rs` | تسجيل المسار | 543 |
| `crates/tira-api/tests/outbound.rs` | جديد: 8 اختبارات تكامل | 337 |

**الاختبارات (+18، من 251 إلى 269):**

| الحزمة | قبل | بعد | الفرق |
|---|---:|---:|---:|
| `tira-api` lib — `send::tests` | 12 | 20 | **+8** |
| `tira-smtp` lib — `client::tests` | 5 | 7 | **+2** |
| `tira-api` — `tests/outbound.rs` (قاعدة حقيقية) | 0 | 8 | **+8** |

**البوابات الخمس (17 نوفمبر 2026، بالترتيب الحرفي):**

| البوابة | الأمر | النتيجة |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | ✅ exit `0` |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ exit `0`، 0 تحذير |
| 3 | `cargo check --workspace --all-targets` | ✅ exit `0` |
| 4 | `.\scripts\dev.ps1 test-db` (`TIRA_REQUIRE_DATABASE=1`) | ✅ **269 passed / 0 failed / 0 ignored**، 33 هدفًا، exit `0` |
| 5 | اتساق الوثائق | ✅ **0 ادعاء حالي قديم** — التفصيل أدناه |

**البوابة 5 — التفصيل (لا «صفر مطابقة» مطلق):** النمط
`250/250 | 250 اختبار | الـ48 | 48 على PostgreSQL | 17554 | 3269 | 540` طابق
**6 مواضع**، كلها إمّا مؤرَّخة أو معلَّقة:

| الملف | السطر | السياق | التصنيف |
|---|---:|---|---|
| `README.md` | 330 | داخل «التحقق النهائي (15 نوفمبر 2026)» | مؤرَّخ، ويُجاوزه قسم 17 نوفمبر |
| `README.md` | 362 | «(كان 540)» داخل قسم 17 نوفمبر | تعليق فرق، لا ادعاء |
| `docs/DEEP_MAP.ar.md` | 449 | «(العدد الحالي بعد §9.40: **269/269** — §0)» | معلَّق |
| `docs/FINAL-REPORT.ar.md` | 186 | §10.1 المؤرَّخ 16 نوفمبر | مؤرَّخ، ويُجاوزه §10.6 |
| `docs/FINAL-REPORT.ar.md` | 195 | جدول «12 نوفمبر \| 16 نوفمبر» | مؤرَّخ بعموديه |
| `docs/FINAL-REPORT.ar.md` | 196 | جدول «12 نوفمبر \| 16 نوفمبر» | مؤرَّخ بعموديه |

كذلك: **0 مسار مكسور** في `README.md` (كل `crates/`·`docs/`·`scripts/`·`deploy/`
مذكور موجود على القرص)، و**269** موجودة في الوثائق الخمس
(`README` 2، `DEEP_MAP` 6، `FINAL-REPORT` 2، الترشّح 3، الخطة 1).

**ملفات الوثائق بعد التعديل (مقاسة):**

| الملف | البايتات | الأسطر | SHA-256 (طرفان) |
|---|---:|---:|---|
| `docs/CORRECTIONS.md` | 287,434 | 3,539 | `4881B332…2341D227` |
| `docs/DEEP_MAP.ar.md` | 71,167 | 905 | `42BCFF65…C5355827` |
| `docs/FINAL-REPORT.ar.md` | 18,687 | 279 | `122CCA30…2F786B89` |
| `docs/CANDIDATURE-Jami3ati-Observatory.md` | 14,530 | 208 | `65368CB4…19D283AB` |
| `docs/OBSERVATORY-DEEP-PLAN.ar.md` | 31,045 | 489 | `6C298386…828D08C6` |
| `README.md` | 23,924 | 380 | `7EC7B041…862C573F` |

> أرقام `CORRECTIONS.md` في هذا الجدول قيست **بعد كتابة §9.40 وقبل تثبيت الجدول
> نفسه**؛ إضافة هذا الجدول غيّرت بايتاته وبصمته. الملف لا يستطيع احتواء بصمته
> النهائية، كما أن الأرشيف لا يستطيع احتواء بصمته (§9.39).

**النسخة الاحتياطية والتراجع:** ستة ملفات Tira قبل التعديل في
`%TEMP%\opencode\observatory-pre-20261117-004721\`
(`db.rs`، `model.rs`، `store.rs`، `client.rs`، `send.rs`، `lib.rs`). الترحيل
0010 يُلغى يدويًا بـ`DROP INDEX` ثم `DROP COLUMN` الأربعة (الـSQL في
`OBSERVATORY-DEEP-PLAN.ar.md` §12)؛ صفوف المتصفح القديمة تبقى صالحة لأن الأعمدة `NULL`.

**ما لم يُبنَ (بصراحة):** الطرف الثاني (Jami3ati: `subscriptions`، double opt-in،
worker) **لم يُكتب بعد**. لا إرسال فعلي من Jami3ati إلى هذا المسار في هذه الجلسة.
وDKIM يبقى مسؤولية الـrelay: لا توقيع محلي، ولا وعد بنتيجة خضراء على المسرح.

**الدرس:** ترميز الردّ جزء من العقد لا زخرفة. `200` هنا كان سيكذب بلا أن يكسر
اختبارًا واحدًا، لأن الاختبارات تقيس ما نكتبه لا ما يعنيه الرمز. والترويسة
المفتوحة ليست «مرونة»: هي سطح حقن. القائمة المغلقة أرخص من كل فحص لاحق.

### 9.41 لقطة 17 نوفمبر 2026 — `TiraMail_Snapshot_20261117-ObservatoryAPI.zip`

**السياق:** بعد نجاح البوابات الخمس على الشجرة التي تحتوي عقد الإرسال الآلي.
القرص `D:` **غير موصول** في هذه الجلسة (خلافًا لـ16 نوفمبر)، فحُفظت اللقطة على
قرص النظام خارج المستودع: `C:\Users\HSYSTEM\Desktop\project\`.

**الملف:**

| العنصر | القيمة |
|---|---|
| المسار | `C:\Users\HSYSTEM\Desktop\project\TiraMail_Snapshot_20261117-ObservatoryAPI.zip` |
| الحجم | **1,626,902 بايت** |
| SHA-256 | `69320A2D4C6B8B2A491F44E5B638FB21EF8F8E1B3B5F20F35838FCE4C397E72F` |
| المدخلات | **172** ملفًا (بلا مجلدات فارغة) |
| الحجم قبل الضغط | 3,034,368 بايت (نسبة الضغط **0.536**) |

**المضمّن:** `crates/` (8 حزم، 56 ملف `*.rs`، **10 ترحيلات**)، `scripts/`،
`docs/` (ويشمل `CANDIDATURE-Jami3ati-Observatory.md` و`OBSERVATORY-DEEP-PLAN.ar.md`)،
`deploy/`، `.github/`، `Cargo.toml`، `Cargo.lock`، `rust-toolchain.toml`،
`README.md`، `.gitignore`، `FOLDER_STRUCTURE.md`، وتقريرا BARID.

**المستبعد (بقصد):** `data/`، `target/`، `.env*`، `quarantine/`.

**التحقق (استخراج كامل ثم مقارنة):**

| الفحص | النتيجة |
|---|---|
| استخراج كامل + SHA-256 لكل ملف | **172/172، 0 اختلاف، 0 مفقود** |
| أسماء مدخلات بفواصل `\` | **0** (كلها `/`) |
| `data/` · `target/` · `quarantine/` · `.env*` | **0** |
| `crates/tira-store/migrations/0010_outbound_api.sql` | ✅ 2,043 بايت |
| `crates/tira-api/tests/outbound.rs` | ✅ 10,662 بايت |
| `docs/CORRECTIONS.md` | ✅ 287,786 بايت (يحتوي §9.31–§9.40) |
| `docs/OBSERVATORY-DEEP-PLAN.ar.md` | ✅ 31,045 بايت |
| `docs/CANDIDATURE-Jami3ati-Observatory.md` | ✅ 14,530 بايت |
| `README.md` | ✅ 23,924 بايت |

**تسلسل التوثيق:** اللقطة تحتوي §9.40 (عقد الإرسال الآلي) لكنها **لا تحتوي
§9.41**، لأن القسم كُتب بعد إنشاء الأرشيف. من يفتحها يجد التوثيق حتى §9.40.

**الدرس:** تغيير القرص ليس تغييرًا في المحتوى؛ اللقطة تُوصف ببصمتها لا بمكانها.
لذلك سُجِّل المسار الكامل، لأن `D:` قد يعود غدًا بملف مختلف بنفس الاسم.

### 9.42 الحلقة 1 في Jami3ati — الاشتراك، double opt-in، وطابور الإرسال (17 نوفمبر 2026)

**السياق:** §9.40 بنى طرف Tira من العقد فقط، وانتهى صراحةً بـ«الطرف الثاني لم يُكتب بعد».
هذا القسم يبني ذلك الطرف: Jami3ati يقرّر **ماذا** يُقال ولمن، ويرسل عبر
`POST /api/v1/outbound` وحده — لا اتصال مباشر بـ SMTP، ولا قراءة لقاعدة Tira.

**ما بُني: 16 ملفًا جديدًا + 4 معدَّلة (73,792 بايت في المجموع).**

| الملف | الحالة | البايتات | الأسطر |
|---|---|---:|---:|
| `app/core/tokens.py` | جديد | 2,165 | 60 |
| `app/core/clock.py` | جديد | 397 | 13 |
| `app/models/subscription.py` | جديد | 1,115 | 25 |
| `app/models/notification.py` | جديد | 2,601 | 57 |
| `app/schemas/subscription.py` | جديد | 961 | 38 |
| `app/schemas/notification.py` | جديد | 1,678 | 64 |
| `app/services/tira_outbound.py` | جديد | 3,815 | 124 |
| `app/services/deliveries.py` | جديد | 11,677 | 361 |
| `app/api/routes/subscriptions.py` | جديد | 6,389 | 174 |
| `app/api/routes/notifications.py` | جديد | 4,661 | 147 |
| `tests/conftest.py` | جديد | 2,605 | 85 |
| `tests/test_tokens.py` | جديد | 1,891 | 46 |
| `tests/test_subscriptions.py` | جديد | 8,057 | 224 |
| `tests/test_deliveries.py` | جديد | 11,476 | 342 |
| `tests/test_tira_outbound.py` | جديد | 4,741 | 153 |
| `pytest.ini` | جديد | 55 | 4 |
| `app/core/config.py` | معدَّل | 1,710 | 44 |
| `app/main.py` | معدَّل | 4,342 | 124 |
| `app/api/router.py` | معدَّل | 460 | 18 |
| `app/api/routes/reports.py` | معدَّل | 2,996 | 80 |

**الجداول الثلاثة — المبني فعلًا يخالف §3.2 من الخطة، والفرق مسجَّل لا مخفي:**

| الخطة (`OBSERVATORY-DEEP-PLAN` §3.2) | المبني فعلًا | لماذا |
|---|---|---|
| `UNIQUE (email_normalized, institution_code)` | `UNIQUE (email)` | صف واحد لكل عنوان: صفّان لنفس البريد = تنبيهان لنفس الشخص عند كل تغيير. |
| `institution_code TEXT` | `institution_id TEXT` | Jami3ati يملك `institutions.id` أصلًا؛ رمز ثانٍ = مسار تطبيع ثانٍ بلا فائدة. |
| `unsubscribe_token_hash BYTEA NOT NULL` | **لا عمود** | التوكن مشتق `HMAC-SHA256(secret, "unsub:"+id)`؛ لا شيء يُخزَّن، فيبقى صالحًا بعد أي نسخة احتياطية، وصف مسروق لا يُنتج توقيعًا. |
| `status IN ('pending','active','unsubscribed','bounced')` | `('pending','active','unsubscribed')` | لا webhook من Tira إلى Jami3ati بعد؛ `bounced` بلا مصدر = ادعاء. |
| `notifications.payload JSONB` | `title_ar`, `body_ar`, `kind`, `source_url`, `verified_at`, `dedupe_key` | أعمدة صريحة تُفهرَس وتُقارَن؛ `JSONB` لا يمنع صفًّا بلا `source_url`. |
| `status IN ('queued','accepted','failed','bounced')` | `('pending','accepted','failed','dead')` | `dead` حالة نهائية بعد استنفاد المحاولات؛ `queued` كان اسمًا لحالة اسمها في الكود `pending`. |
| لا أعمدة للترويسات | `correlation_id`, `list_unsubscribe`, `reply_to` مجمَّدة وقت الإدراج | الرسالة المُوعَد بها لا تتغيّر إذا عُدِّل الإشعار لاحقًا (`subject`/`body` مجمَّدان كذلك). |
| `id UUID` | `TEXT` بصيغة `sub-<12 hex>` | اتساق مع `rep-…` و`ntf-…` في نفس القاعدة. |

**نقاط العقد التي ثُبِّتت في الكود (لا في الوثيقة فقط):**

1. **`202` = قبول، لا تسليم.** الـ worker يكتب `accepted` فقط عند `202`، ويحفظ `tira_message_id`؛ لا كلمة «سُلِّم» في أي مسار.
2. **`400`/`404` = `failed` بلا إعادة** — الطلب نفسه لن ينجح أبدًا، فإعادته ضجيج.
3. **`429`/`5xx`/خطأ نقل = إعادة بتباعد `30s → 60s → … → 900s`**، ثم `dead` عند `DELIVERY_MAX_ATTEMPTS=3`.
4. **`FOR UPDATE SKIP LOCKED`** في السحب: عاملان متوازيان لا يأخذان الصف نفسه.
5. **`idempotency_key` فريد** في `notification_deliveries`: إعادة تشغيل نفس الإشعار لا تُنتج بريدًا ثانيًا.
6. **`List-Unsubscribe` لا يُرسَل إلا إذا بدأ `JAMI3ATI_PUBLIC_BASE_URL` بـ`https://`** — مجموعة Tira المغلقة ترفض `<http://…>`، والرابط في نص الرسالة يبقى يعمل دائمًا.
7. **`JAMI3ATI_HMAC_SECRET` إلزامي:** غيابه = `503`، لا اشتراك بلا رابط إلغاء قابل للتحقق.
8. **`POST /notifications` محميّ بـ`X-Jami3ati-Admin`** بمقارنة ثابتة الزمن؛ غياب `JAMI3ATI_ADMIN_TOKEN` = `503` (سطح الكتابة مُطفأ، لا مفتوح).

**الاختبارات: 51 خضراء، 0 فاشلة (8.17 ثانية).**

| الملف | العدد | ملاحظة |
|---|---:|---|
| `tests/test_deliveries.py` | 18 | جديد — رسم الحالات، الإعادة، `dead`، النطاق، idempotency، بوابة الإدارة |
| `tests/test_subscriptions.py` | 12 | جديد — double opt-in، تدوير التوكن، الانتهاء، التلاعب، الإلغاء GET/POST |
| `tests/test_tira_outbound.py` | 12 | جديد — شكل الحمولة، مجموعة مغلقة، تعيين الرموز، بلا شبكة |
| `tests/test_tokens.py` | 5 | جديد — عشوائية، digest، HMAC حتمي، رفض التلاعب |
| `tests/test_health.py` | 2 | قائم (6 نوفمبر) |
| `tests/test_universities.py` | 2 | قائم (6 نوفمبر) |
| **المجموع** | **51** | **47 جديدًا + 4 قائمة** |

**البرهان الحي — عبر لغتين، على `tira-api` حقيقي لا على fake:**

| المحاولة | النتيجة المقاسة |
|---|---|
| إرسال أول (موضوع عربي `فتح الترشيح — ENSIAS 2026`) | **`202 accepted`**، `id = 2eb56304-685a-48e8-9e46-657030dd0fba` |
| إعادة الإرسال بنفس `idempotency_key` | **`202`**، **نفس `id`** — لا صف ثانٍ |
| `idempotency_key = "has a space"` | **`400`**: `idempotency_key must be 8-128 chars of [A-Za-z0-9._:-]` |
| `list_unsubscribe = "<http://insecure.example/x>"` | **`400`**: `list_unsubscribe must be <https://...> or <mailto:...>` |
| صف `outbound_queue` | **1 صف فقط** للمفتاح المُعاد؛ الموضوع العربي سليم؛ الحقول الأربعة المغلقة مخزَّنة حرفيًا |

**التنظيف بعد البرهان (مقاس، لا موعود):** توكن البروفة `7d31c4e8-ccb2-4937-97ed-a78d14d44d9d`
مُبطَل (exit `0`)، `DELETE 1` لصفوف `probe:%`، `tira-api` PID 18048 مُوقَف،
`probe-token.txt` محذوف. توكن العرض `f3716171-…` **لم يُمسّ**.

**خلل حقيقي كشفه الاختبار قبل الإرسال:** أول تشغيل لـ`test_payload_matches_the_closed_contract`
فشل لأن الاختبار افترض أن `correlation_id` يخضع لقيد `8-128` مثل `idempotency_key`.
في Tira `optional_key` يقبل `1-128` (السطر 518 من `send.rs`). صُحِّح الاختبار، لا الكود:
القيدان مختلفان بقصد، والخلط بينهما كان سيُنتج رفضًا على المسرح لمفتاح قصير مشروع.

**ما لم يُبنَ (بصراحة، وليس نقصًا مستورًا):**

- **لا poller** (كشف تغيير المصدر) — الحلقة 2 تبقى `POST /notifications` يدوية الإطلاق.
- **لا صفحة `/n/{id}` في Jami3ati** — التحقق العام الحالي هو أرشيف `GET /notifications` مع `source_url` و`verified_at`؛ صفحة Tira منفصلة.
- **لا `bounced`** — يحتاج webhook من Tira إلى Jami3ati، وهو غير موجود.
- **لا إرسال إلى بريد حقيقي في هذه الجلسة:** relay البروفة كان `127.0.0.1:1` (منفذ مغلق)، فبقي صف Tira `pending`. هذا سلوك صحيح (لا ادعاء وصول)، لكنه **ليس** إثبات تسليم.
- **لا signup عام قبل الحدث** — الصندوق مُهيَّأ مسبقًا.
- `datetime.utcnow()` بقي في `app/models/report.py` (تحذيران) عمدًا: خارج نطاق هذا التعديل. التحذيرات هبطت **246 → 31**، والـ31 كلها من `pydantic` class-based config القائم.

**تشغيل العرض (الأوامر كما جُرِّبت):**

```powershell
$env:DATABASE_URL              = "postgresql://postgres@127.0.0.1:55432/jami3ati_test"
$env:JAMI3ATI_HMAC_SECRET      = "<سر عشوائي 32+ بايت>"
$env:JAMI3ATI_ADMIN_TOKEN      = "<رمز إدارة>"
$env:JAMI3ATI_PUBLIC_BASE_URL  = "http://127.0.0.1:8000"
$env:MODERATOR_EMAIL           = "yassine@tira.test"
$env:TIRA_OUTBOUND_URL         = "http://127.0.0.1:8080/api/v1/outbound"
$env:TIRA_OUTBOUND_TOKEN       = "<توكن Tira الحيّ>"
$env:TIRA_MAILBOX_ID           = "33333333-3333-4333-8333-333333333333"
uvicorn app.main:app --host 127.0.0.1 --port 8000
```

بدون `TIRA_OUTBOUND_TOKEN` + `TIRA_MAILBOX_ID` يبقى الـ worker **مُطفأً** ويُسجّل ذلك؛
هذا مقصود: تشغيل بلا اعتماد لا يجوز أن يدّعي إرسالًا.

**البوابة 5 بعد هذه التعديلات (17 نوفمبر 2026):** ✅ **0 ادعاء حالي قديم**،
**0 مسار مكسور** في `README.md` (16 مسارًا مُتحقَّقًا)، والملفات الستة كلها UTF-8 صالح
بلا BOM وبلا علامات موضع نائبة. مطابقات النمط القديم بقيت **6** كما في §9.40 (كلها مؤرَّخة أو معلَّقة)؛
أُضيف الرقم `51` في أربعة ملفات، بلا تعارض مع `269` (حزمة Tira) لأنه يخصّ حزمة أخرى.

**ملفات الوثائق بعد التعديل (مقاسة):**

| الملف | البايتات | الأسطر | SHA-256 (طرفان) |
|---|---:|---:|---|
| `docs/CORRECTIONS.md` | 300,473 | 3,702 | `D2F2609F…99E275DD` |
| `docs/OBSERVATORY-DEEP-PLAN.ar.md` | 33,483 | 514 | `4BD6C595…82B832CA` |
| `docs/CANDIDATURE-Jami3ati-Observatory.md` | 16,437 | 214 | `E0F35984…29179CC5` |
| `docs/DEEP_MAP.ar.md` | 71,167 | 905 | `42BCFF65…C5355827` |
| `docs/FINAL-REPORT.ar.md` | 18,687 | 279 | `122CCA30…2F786B89` |
| `README.md` | 23,924 | 380 | `7EC7B041…862C573F` |

> أرقام `CORRECTIONS.md` وبصمته قيست **قبل إضافة هذا الجدول**؛ إضافته غيّرتها.
> الملف لا يستطيع احتواء بصمته النهائية (§9.40، §9.41). أرقام الملفات الخمسة الأخرى نهائية.

**الدرس:** الطابور ليس تعقيدًا، هو المكان الوحيد الذي يمكن فيه أن تقول «لم يصل» بلا أن
تفقد القرار. لو أرسلنا من داخل الطلب لكان كل فشل شبكة يعني بلاغًا ضائعًا أو بريدًا مكرَّرًا.
والفرق بين `202` و`200` هنا ليس تفصيلًا: هو الفرق بين «حفظتُ وعدًا» و«سلّمتُ رسالة».

---

## 9.43 — 17 نوفمبر 2026 — `MIME-Version`/`Content-Type` وإعادة التسمية إلى «جامعتي»

### 1. العيب: رسالة بلا هوية محتوى

الرسائل الخارجة من `build_message_with` كانت تحمل `Content-Transfer-Encoding: base64`
**بلا** `MIME-Version` و**بلا** `Content-Type`. هذا ليس نقصًا تجميليًا: RFC 2045 يجعل
الافتراضي `text/plain; charset=us-ascii`، فالعميل الصارم يفكّ الـbase64 ثم يفسّر البايتات
كـUS-ASCII — أي نص عربي تالف. المنتج كله مبني على نص عربي، فالتسامح مع العملاء ليس ضمانًا.

**الدليل قبل الإصلاح** (رأس الرسالة كما خرج فعلًا إلى SMTP، لا كما نتمنّاه):

```
From: yassine@tira.test
Subject: Restart probe 2026-11-17
To: sink-probe@tira.test
X-Jami3ati-Notification: probe-restart-1
Content-Transfer-Encoding: base64
Date: Tue, 17 Nov 2026 11:22:39 +0000
```

**السبب:** `lettre 0.11.23` — `MessageBuilder::body()` تضبط `Content-Transfer-Encoding`
فقط (`src/message/mod.rs:458`)، ولا تضبط الرأسين أبدًا.

**الإصلاح** (`crates/tira-smtp/src/client.rs`): الرأسان يُكتبان دائمًا عبر مسار
`raw_header` نفسه الذي يمرّ منه باقي الرؤوس، بعد التحقق من قيمتها الثابتة:

```
MIME-Version: 1.0
Content-Type: text/plain; charset=utf-8
```

وأُضيف حاجز `is_reserved_content_header` يرفض أي محاولة من المستدعي لتجاوز
`MIME-Version` أو `Content-Type` أو `Content-Transfer-Encoding` — لأن رأسًا يعلن ترميزًا
يخالف بايتاته أسوأ من غيابه.

**الاختبارات (+2):** `tira-smtp` lib من 13 إلى **15**:

| الاختبار | ما يثبته |
|---|---|
| `declares_mime_version_and_utf8_charset_so_arabic_survives_the_wire` | الرأسان موجودان، **ثم** يُفكّ الـbase64 ويُقارن بالنص العربي الأصلي — الحضور وحده لا يكفي |
| `rejects_attempts_to_override_the_content_headers` | أربع صيغ (بما فيها `content-type` بحروف صغيرة) تُرفض بـ`MailError::Message` |

`base64` أُضيف كـ`dev-dependency` فقط (`base64.workspace = true`)؛ لا اعتماد جديد في الإنتاج.

### 2. البوابات الخمس (بعد الإصلاح وإعادة التسمية)

| البوابة | الأمر | النتيجة |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | ✅ exit `0` |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ exit `0`، 0 تحذير |
| 3 | `cargo check --workspace --all-targets` | ✅ exit `0` |
| 4 | `.\scripts\dev.ps1 test-db` | ✅ **271 passed / 0 failed / 0 ignored**، 33 هدفًا، exit `0` |
| 5 | اتساق الوثائق | ✅ **0 مسار مكسور** (16/16)، 0 BOM، 0 UTF-8 غير صالح |

**المحاولة الأولى للبوابة 4 سقطت — والسبب بنيوي لا اختباري:**
`error: failed to remove file ...\target\debug\tira-api.exe — Accès refusé. (os error 5)`.
`tira-api.exe` (PID 8612) كان ما زال حيًّا ويقفل الملف التنفيذي. أُوقف، ثم أُعيدت البوابة
فنجحت. ملاحظة تشغيلية: `Get-NetTCPConnection` على هذا الجهاز أعطى نتيجة سالبة كاذبة
للمنافذ الحيّة؛ الفحص الصادق هو `Get-Process` + محاولة فتح الملف، أو اتصال TCP مباشر.

**البوابة 5 — النمط القديم** (`250/250 | 250 اختبار | الـ48 | 48 على PostgreSQL | 17554 | 3269 | 540`)
طابق **26** موضعًا في الملفات الستة، منها **6 خارج `CORRECTIONS.md`** فقط — وهي نفس الستة
المؤرَّخة/المعلَّقة في §9.40 (`README` 330 و362، `DEEP_MAP` 449، `FINAL-REPORT` 186 و195 و196).
`CORRECTIONS.md` نفسه يستشهد بالنمط تاريخيًا، فلا يُحتسب.

### 3. إعادة التسمية: «مرصد جامعتي / Jami3ati Observatory» ← «جامعتي / Jami3ati»

الاسم صار **جامعتي** عربيًا و**Jami3ati** لاتينيًا. التسمية الفرعية بقيت وصفًا لا اسمًا
(«مرصد مواطن للبيانات المفتوحة في التعليم العالي» / «Observatoire citoyen des données
ouvertes…»)، لأنها جملة تعريف لا علامة.

| الموضع | قبل | بعد |
|---|---|---|
| `docs/CANDIDATURE-Jami3ati-Observatory.md` | اسم الملف + العنوان + حقلا الاسم (AR/FR) | `docs/CANDIDATURE-Jami3ati.md` |
| `docs/OBSERVATORY-DEEP-PLAN.ar.md` | اسم الملف + العنوان + 3 إشارات في المتن | `docs/JAMI3ATI-DEEP-PLAN.ar.md` |
| `docs/FINAL-REPORT.ar.md` | سطر 235 + مسار الخطة في سطر 279 | «جامعتي» + `JAMI3ATI-DEEP-PLAN.ar.md` |
| `docs/DEEP_MAP.ar.md` | «العدد الحالي بعد §9.40: 269/269» | «بعد §9.43: 271/271» |
| `crates/tira-api/src/send.rs:407` | «the Observatory contract» | «the Jami3ati contract» |
| `crates/tira-api/tests/outbound.rs:3` | «pin the Observatory contract» | «pin the Jami3ati contract» |
| `crates/tira-store/migrations/0010_outbound_api.sql:1` | «(Observatory integration)» | «(Jami3ati integration)» |
| Jami3ati `app/core/config.py:12`، `app/core/tokens.py:1` | «Observatory delivery layer/loop» | «Jami3ati delivery layer/loop» |

**المراجع التاريخية لم تُعَد كتابتها.** الأقسام §9.40–§9.42 في هذا الملف تذكر اسمي
الملفين القديمين، وهذا صحيح: هي تسجّل ما كان موجودًا يومها. إعادة كتابتها كانت ستجعل
السجل يكذب. من يبحث اليوم يجد الملفين بالاسمين الجديدين، وهذا القسم هو الجسر.

**ما لم يتغيّر عمدًا:** «مرصد» بقيت كلمة وصفية في التسمية الفرعية وفي عنوان الخطة
(«مرصد مواطن…»)، ومسار النسخة الاحتياطية المؤرّخ في الخطة (سطر 470) بقي كما هو.

### 4. ملفات الوثائق بعد التعديل (مقاسة)

| الملف | البايتات | الأسطر | SHA-256 (طرفان) |
|---|---:|---:|---|
| `docs/DEEP_MAP.ar.md` | 71,167 | 905 | `87B691D3…7296359A` |
| `docs/FINAL-REPORT.ar.md` | 18,675 | 279 | `547D8EFE…55906AFE` |
| `docs/CANDIDATURE-Jami3ati.md` | 16,374 | 214 | `EF084BA7…0618611E` |
| `docs/JAMI3ATI-DEEP-PLAN.ar.md` | 33,462 | 514 | `71D1C1E5…754ACF41` |
| `README.md` | 23,924 | 380 | `7EC7B041…862C573F` (بلا تغيير) |
| `docs/CORRECTIONS.md` | 301,906 | 3,721 | `4AE078AF…53C35330` (قبل إضافة هذا القسم) |

> كالعادة: أرقام `CORRECTIONS.md` قيست قبل إضافة هذا القسم؛ إضافته غيّرتها. الملف لا
> يستطيع احتواء بصمته النهائية (§9.40، §9.41).

### 5. النسخة الاحتياطية والتراجع

الملفات التسعة قبل التعديل في
`%TEMP%\opencode\rename-mime-pre-20261117-123527\` (بأسماء مسطّحة `__` بدل `\`):
`client.rs`، `tira-smtp/Cargo.toml`، `send.rs`، `outbound.rs`، `0010_outbound_api.sql`،
والملفان الوثائقيان القديمان، و`FINAL-REPORT.ar.md`، و`CORRECTIONS.md`.
التراجع عن الإصلاح = حذف سطري `raw_header` الثابتين وحاجز الأسماء المحجوزة، وإعادة الملفات
من النسخة. إعادة التسمية عكسية بـ`Move-Item` وحده.

### 6. ما لم يُفعَل (بصراحة)

- **لم يُقس الإصلاح على عميل بريد حقيقي** (Gmail/Outlook): الدليل هو رأس الرسالة على
  السلك + فكّ base64 في الاختبار. هذا يثبت البنية، لا سلوك كل عميل.
- **`List-Unsubscribe` ما زال غائبًا** في العرض المحلي عمدًا: `JAMI3ATI_PUBLIC_BASE_URL`
  يبدأ بـ`http://`، وTira يقبل فقط `<https://…>` أو `<mailto:…>`.
- **الأسماء السبعة النائبة في ملف الترشّح ما زالت نائبة** (`[الاسم الكامل]` / `[Nom complet]`):
  هذه مدخلات بشرية لا أستطيع اختراعها، وهي الحاجز الوحيد أمام الإرسال.

**الدرس:** الرأس الذي لا تكتبه لا يراه أحد حتى يفسد. الـbase64 جعل الرسالة «تبدو»
سليمة في السجل، والخطأ كله كان في سطرين غائبين. الاختبار الذي ينتهي عند
`raw.contains("MIME-Version")` كان يكفي ليُخفي الخطأ الحقيقي؛ الذي كشفه هو **فكّ الترميز
ومقارنة الناتج بالنص المُدخل**.

## 9.44 — 17 نوفمبر 2026 — وصل واجهة Jami3ati بالـAPI الحقيقي (كتابة حقيقية، لا بيانات وهمية)

### 1. العيب: واجهة تتكلّم مع نفسها

نسخة الواجهة (`:3000`) كانت كاملة المظهر وصفر الحقيقة: زرّ الاشتراك يحفظ في `localStorage`،
ورقم البلاغ يُولَّد في المتصفّح بصيغة `JAM-######`، وقسم الإعلانات يعرض `initialAnnouncements`
من `src/data/seedData.ts`، وشارة «موثق رسميًا» تُطبع على بيانات لم يرها خادم قط. عرض كهذا
ينجح في كل نقرة ويفشل في كل ادّعاء: لا صفّ في القاعدة، لا رسالة خارجة، ولا رابط تحقّق.

**القرار (باختيار المستخدم، الخيار 2):** يبقى كتالوج الجامعات/المؤسسات من `seedData.ts`
**معلَنًا كبيانات عرض**، وتصبح مسارات الكتابة والأرشيف العام حقيقية:
`POST /subscriptions`، `POST /reports`، `GET /notifications`، `GET /institutions`.

### 2. طبقة HTTP واحدة — `src/lib/api.ts` (ملف جديد)

- `API_BASE_URL` = `VITE_API_BASE_URL` إن وُجد، وإلا `/api/v1` على أصل الصفحة نفسها.
- `ApiError` يحمل `status` و`isNetworkError` (الحالة `0` = فشل نقل، والمتصفّح يخفي السبب:
  CORS أو DNS أو رفض اتصال — الواجهة تقول «تعذّر الاتصال» ولا تخمّن أيّها).
- `AbortController` بمهلة 12 ثانية، وتحليل `detail` في FastAPI سواء كان نصًّا أو مصفوفة
  تحقّق (`[{msg: …}]`) وتجميعها بـ` · `.
- دوال مصنّفة: `subscribe`، `listNotifications`، `listInstitutions`، `submitReport`، وأنواع
  `*Record` تطابق نماذج Pydantic حقلًا بحقل. لا حقل مُختلق.

### 3. proxy بدل CORS — `vite.config.ts`

الواجهة تنادي `/api/v1` على أصلها؛ Vite يمرّرها إلى `http://127.0.0.1:8000` في **`server`**
و**`preview`** معًا (وإلا نُجح العرض في التطوير وفشل في `dist/`). الهدف قابل للتغيير بـ
`VITE_API_PROXY_TARGET`. الفائدة العملية: هاتف على نفس الشبكة يفتح `http://192.168.1.184:3000`
ويعمل، لأنه يكلّم Vite لا `127.0.0.1` الخاص بهذا الجهاز. `VITE_API_BASE_URL` بقي للاستضافة
الساكنة التي لا proxy أمامها، ومذكور في `.env.example`.

### 4. التغييرات الصادقة في الواجهة

| الموضع | قبل | بعد |
|---|---|---|
| `AcademicAlertsSubscription.tsx` | بريد **أو هاتف/SMS** + `localStorage` + «تم التفعيل» | بريد فقط (لا SMS في الخادم)، `POST /subscriptions`، حالات `idle\|submitting\|sent\|error`، مرجع حقيقي `sub-…`، ونصّ صريح: «ما زال pending» + تنويه أن البريد يصل إلى SMTP sink محلي |
| نفس الملف | قائمتا تخصّص/مدينة ثابتتان | `select` مؤسسات حقيقي من `GET /institutions`؛ عند فشله يبقى «كل المؤسسات» ويُقال ذلك |
| `AnnouncementsSection.tsx` | prop من `seedData` + شارة «موثق رسميًا» | يجلب `GET /notifications` بنفسه (`loading\|ready\|error`)، يعرض `id` و`verified_at` واسم مضيف `source_url`، ويصرّح: «لا نعرض بيانات تجريبية بدلًا من الأرشيف» |
| `ReportModal.tsx` | تذكرة `JAM-######` من المتصفّح، إرسال متزامن | `await onSubmitReport(...)`، دوّارة `Loader2` وتعطيل الزر، وعرض `stored.id` — أي معرّف الخادم |
| `App.tsx` | `handleAddReport` يبني صفًّا محليًا | ينادي `submitReport` ثم `mapStoredReport` ويدرج ما أعاده الخادم فقط؛ حُذف `announcements` من الحالة |
| `.env.example` | — | توثيق `VITE_API_PROXY_TARGET` و`VITE_API_BASE_URL` |

لا سقوط إلى بيانات العرض عند الفشل: حالة الخطأ تعرض رسالة وزرّ إعادة محاولة فقط.

### 5. الأدلة المقاسة

| الفحص | الأمر/الإجراء | النتيجة |
|---|---|---|
| الأنواع | `npm run lint` (`tsc --noEmit`) | ✅ exit `0` |
| البناء | `npm run build` | ✅ exit `0`؛ `dist/assets/index-B1-fprGw.js` 740,180 B (gzip 179.21 kB)، CSS 57,166 B |
| الـproxy | `GET http://127.0.0.1:3000/api/v1/notifications` | ✅ `200`، 490 B، `application/json` |
| الأرشيف | فتح `http://127.0.0.1:3000/?wire=1` | ✅ بطاقة واحدة `ntf-d74e8506d52d`، الشارة «1 إشعارًا منشورًا»، `dir=rtl`، **0** خطأ في console |

### 6. الحلقة الكاملة: اشتراك ← بريد ← تأكيد ← `active`

1. الواجهة: `ui.wire.20261117@example.ma` ← ظهر «تحقّق من بريدك» و«الاشتراك ما زال pending»
   والمرجع `sub-172310c6c84b`.
2. القاعدة: `sub-172310c6c84b | pending | confirmed_at = NULL`.
3. الطابور: `dlv-13f0750fe252 | confirm | … | accepted | attempts=1 | tira_message_id` موجود.
4. الـsink: `007-yassine_at_tira.test.eml` — 1,235 B، `MIME-Version: 1.0` ✅،
   `Content-Type: text/plain; charset=utf-8` ✅، رابط واحد فقط:
   `…/subscriptions/confirm?id=sub-172310c6c84b&token=NB42…` (التوكن مقصوص هنا عمدًا).
5. فتح الرابط في المتصفّح ← `{"id":"sub-172310c6c84b","status":"active"}`.
6. القاعدة بعدها: `active`، `confirmed_at` مضبوط.

أي: التفعيل لم يُمنح من الواجهة، بل من فتح رابط وقّعه الخادم ومرّ عبر Tira.

### 7. البلاغ: الرقم المعروض هو رقم الخادم

المحاولة الأولى أنشأت `rep-32817b` (القاعدة تُثبته) لكن المودال أُغلق قبل قراءة النص،
فلم أعتبره دليلًا على العرض. أُعيد الاختبار مع مراقبة DOM من داخل الصفحة:

- ظهرت حالة «جارٍ الإرسال…» فعلًا، ثم لوحة النجاح بعد **2,337 ms** من `requestSubmit`.
- النص المعروض حرفيًا: «رقم تتبع البلاغ: **rep-fd5525**» + «سُجّل البلاغ بحالة pending».
- القاعدة: `rep-fd5525 | general | pending | https://www.ensias.ma/avis-2026 | 2026-11-17 12:33:54`.
- بريد المشرف: `dlv-908a7e3fed3b | moderator | yassine@tira.test | accepted | attempts=1`.
- الـsink: `009-yassine_at_tira.test.eml` — 1,141 B، `MIME` ✅، `CT` ✅، ويحوي `rep-fd5525`
  ورابط المصدر نفسه.

أعداد قاعدة العرض بعد الجلسة: `subscriptions=2`، `notifications=1`،
`notification_deliveries=5`، `inaccuracy_reports=2`.

### 8. الملفات وبصماتها (SHA-256، مقاسة)

| الملف | SHA-256 (طرفان) |
|---|---|
| `src/lib/api.ts` (جديد) | `DCA84AD0…F6212433` |
| `src/components/AcademicAlertsSubscription.tsx` | `E788A118…47552BBF` |
| `src/components/AnnouncementsSection.tsx` | `1FE9EBE5…C0B7DA9C` |
| `src/components/ReportModal.tsx` | `2CB17A0B…54D9E99E` |
| `src/App.tsx` | `1C8904F7…34740A48` |
| `vite.config.ts` | `0D8BCFAE…61F1F483` |

النسخة الاحتياطية قبل الوصل: `%TEMP%\opencode\frontend-wiring-pre-20261117-132141\`
(6 ملفات بأسماء مسطّحة `__`؛ `api.ts` لم يكن موجودًا). التراجع = إعادة الملفات الستة وحذف
`src/lib/api.ts`.

### 9. ما لم يُفعَل (بصراحة)

- **الجامعات والمؤسسات ما زالت من `seedData.ts`** (معلَنة كبيانات عرض). ما صار حقيقيًا هو
  الكتابة والأرشيف، لا الكتالوج.
- **`initialAnnouncements` ما زال مُصدَّرًا في `seedData.ts` ولا يستورده أحد**: تصدير ميت،
  حذفه تغيير منفصل لم أخلطه بهذا القسم.
- **`PATCH /reports/{id}/status` بلا مصادقة** في الخادم، وبوابة المشرف في الواجهة ما زالت
  محليّة ولم تُوصل. فجوة أمنية معروفة، خارج نطاق هذه الجلسة.
- **لا اختبارات واجهة آلية**: التحقّق كان حيًّا (متصفّح + قاعدة + sink). هذا دليل سلوك،
  لا حماية من انحدار.
- **تحذير حجم الحزمة** (740 kB > 500 kB) قائم؛ يحتاج `dynamic import`، ولم يُعالَج.
- **البوابات الخمس لا تغطّي هذا التغيير**: هي بوابات مستودع Tira، والتعديل في مستودع
  Jami3ati. لا أزعم أنها شهدت على شيء هنا.

**الدرس:** انتظاري على نصّ «رقم البلاغ» بينما المكوّن يطبع «رقم تتبع البلاغ:» أضاع دورة
كاملة وجعلني أُغلق مودالًا ناجحًا وأظنّه فاشلًا. الدليل لم يكن غائبًا — كان اسمًا آخر.
الآن أقرأ النصّ من الصفحة نفسها وأقارن المعروض بالمخزَّن، لا بما توقّعتُ أن يُطبع.

## 9.45 — 17 نوفمبر 2026 — إتمام نشر 271 في `README` (تصحيح جانبي كشفته البوابة 5)

### 1. كيف ظهر

مخرجات البوابة 5 في نهاية §9.44 طابعت سطر الإحصاء: `README.md: 269=3 271=0`. أي أن
`README` — وهو وثيقة حالة حيّة لا سجلًّا مؤرَّخًا — ما زال يعلن **269** اختبارًا بعد أن
قاس §9.43 **271**. §9.43 صحّح `CANDIDATURE-Jami3ati.md` وحده، فبقي `README` متأخرًا
باختبارين. ادّعاء رقمي خاطئ في أول ملف يقرأه المحكّم ليس تفصيلًا.

### 2. القياس قبل التعديل (لا ترقيع رقم واحد)

قيست الأرقام كلها بسكربت يمرّ على المستودع (بلا `target/` و`node_modules/`) ويعدّ
الأسطر غير الفارغة، لأن إصلاح MIME أضاف كودًا لا اختبارات فقط:

| المقياس | القيمة المقيسة | ما كان في `README` |
|---|---:|---:|
| اختبارات (`dev.ps1 test-db`) | **271** (0 failed، 0 ignored، 33 هدفًا) | 269 |
| ملفات `*.rs` | 56 | 56 |
| أسطر Rust غير الفارغة | **18,843** | 18,744 |
| ملفات اختبارات التكامل | 13 | (غير مذكور في هذا القسم) |
| أسطر اختبارات التكامل غير الفارغة | 3,575 | (غير مذكور في هذا القسم) |
| ترحيلات SQL | 10 | 10 |
| أسطر SQL غير الفارغة | 575 | 575 |
| مسارات HTTP (`.route(`) | 24 | 24 |
| Crates | 8 | (غير مذكور في هذا القسم) |

الفرق `+2` اختبارًا و`+99` سطر Rust هو بالضبط اختبارا الترميز وحاجز
`is_reserved_content_header` في `crates/tira-smtp/src/client.rs`.

### 3. ما عُدّل في `README.md`

| السطر | قبل | بعد |
|---|---|---|
| 101 | `# 269 اختباراً` | `# 271 اختباراً` |
| 357 | `**269/269**` | `**271/271**` |
| 360 | `**+18 اختبارًا** … 2 ترويسات في client.rs` | `**+20 اختبارًا** … 4 ترويسات في client.rs` |
| 363 | `**18,744** سطر Rust` | `**18,843** سطر Rust` |
| 365–368 | — | تنويه مؤرَّخ يشرح مصدر `+2`/`+99` وأن جدول 15 نوفمبر بقي كما هو |

### 4. ما بقي متعمَّدًا بلا مساس (وسببه)

- **`docs/FINAL-REPORT.ar.md:269`** يقول `269/269`: يقع داخل §1–§9، والملف نفسه يعلن في
  سطره 9 أن هذه الأقسام «سجل مؤرَّخ ليوم 12 نوفمبر ولم تُعَد كتابتها». تعديله كان سيجعل
  التقرير يدّعي أن يوم 12 نوفمبر قاس 271. لا.
- **`docs/DEEP_MAP.ar.md`** يحمل `269` في خمسة مواضع (سطره 19، 727، 826، 849، 900) و
  `3,269` سطر اختبارات في 12 ملفًا — مقابل 3,575 في 13 ملفًا مقيسة اليوم. رأسه يقول
  «آخر قياس: 7 نوفمبر 2026»، لكن §9.43 حدّث سطره 450 إلى `271/271`، فصار الملف متناقضًا
  داخليًا. تحديثه يحتاج **جولة قياس كاملة** لخريطة من 900 سطر، لا تبديل رقم؛ أجّلتها
  صراحةً بدل أن أُصلح خمسة أرقام وأترك الباقي يكذب. هذا دين مقيس، لا منسيّ.
- **`docs/JAMI3ATI-DEEP-PLAN.ar.md:418`** يقول «269 اختبارًا قائمًا … (كانت 251: +18)»:
  جملة تصف لحظة كتابة الخطة. تُركت كسجل، والجسر هو §9.43 وهذا القسم.
- **`README.md:375`** («لا عزل مستأجرين على مستوى القاعدة RLS») ما زال قائمًا: إصلاحه
  يحتاج قياسًا مقابل `crates/tira-store/tests/rls.rs` و`0004_rls.sql`، وهو بند منفصل
  أُجّل باتفاق (بعد الإرسال).

### 5. البوابة 5 بعد التعديل

أُعيد تشغيل `%TEMP%\opencode\gate5.ps1` على الحالة النهائية: **exit `0`** — 0 مسار
`README` مكسور (16/16)، 0 BOM، 0 UTF-8 غير صالح. نمط الادّعاءات القديمة طابق 27 موضعًا،
منها **6 خارج `CORRECTIONS.md`** فقط (نفس الستة المؤرَّخة في §9.40 و§9.43)، والباقي
اقتباسات تاريخية داخل هذا السجل.

---

## 9.46 — 17 نوفمبر 2026 — البنود الأربعة المؤجَّلة، وعيب خامس كشفه التحقق الحيّ

### 1. نطاق هذا القسم

بعد §9.44 بقيت أربعة بنود مؤجَّلة صراحةً: (1) ثلاثة إصلاحات عرض في Tira (تأكيد
الإرسال، رأس الوارد، لوحة التحقق)، (2) أرقام `DEEP_MAP` المتقادمة، (3) ادّعاء RLS
في `README`، (4) `PATCH /reports/{id}/status` بلا مصادقة في Jami3ati. أُنجزت
الأربعة، وكشف التحقق الحيّ في المتصفح **عيبًا خامسًا** لم يكن في القائمة: نموذج
التحرير كان يُصدر `<textarea>` بلا وسم إغلاق، فيبتلع زر «إرسال» — أي أن الإرسال من
الواجهة كان **مستحيلًا** لا «غير مُختبر». أُصلح أيضًا.

القاعدة المتّبعة: لا ادّعاء قبل القياس. كل رقم أدناه من DOM حقيقي أو من مخرجات
بوابة، لا من قراءة الشيفرة.

### 2. القياس قبل الإصلاح (ما رآه المتصفح فعلًا)

| ما قيس | قبل | الأداة |
|---|---|---|
| ارتفاع `.section-head` | **44px** (يسحبه `.btn` بـ`min-height:44px`) | `getBoundingClientRect` |
| `.section-head h1` | 28px | `getComputedStyle` |
| نص `.sidebar__foot` | «مزامنة آمنة» · «جلسة آمنة · قابلة للإلغاء» | `innerText` |
| عنوان بطاقة القارئ | «التحقق البريدي» | DOM |
| تسميات الفحوص | «سليم» / «تحذير» / «فشل» | DOM |
| `<textarea>` في التحرير | بلا `</textarea>`؛ قيمته تبدأ بـ`</div><div class="form-actions">…` | `fetch` للـHTML الخام |
| زر الإرسال في DOM | **غير موجود** (ابتلعه الـ`textarea`) | `querySelector('form[action$="/send"] button[type=submit]')` = `null` |

العيب الخامس ليس تجميليًا: `spellcheck="false";` في ماود يعني «عنصر بلا محتوى»،
و`textarea` ليس عنصرًا باطنًا (void)، فالمتصفح يقرأ كل ما بعده نصًّا داخل الحقل.
لذلك لم يكن في الصفحة زر إرسال أصلًا، وأي محاولة «إرسال» من الواجهة كانت مستحيلة.

### 3. تفسير «الإصلاح 2» المختار (وسببه المقيس)

الصياغة الأصلية («رأس وارد ضخم» + «شارة كاذبة») تحتمل أكثر من قراءة، فحُسمت
بالقياس لا بالذوق:

- **الرأس الضخم** = `.section-head` بشريط القائمة، لا `.topbar` (56px، مقصود) ولا
  صفوف القائمة (≥64px، مقصودة). ارتفاعه 44px كان يأتي كله من زر «تحديث» بارتفاع
  44px، أي أن عنصر تحكم واحدًا كان يحدّد ارتفاع عنوان الصفحة.
- **الشارة الكاذبة** = `.sidebar__foot`: «مزامنة آمنة» تدّعي مزامنة غير موجودة
  (Tira لا يزامن شيئًا؛ البريد يُستقبل عبر SMTP)، و«جلسة آمنة» حكم لم يُقس بأي
  اختبار. استُبدلتا بحقيقتين قابلتين للتحقق، لا بنعت آخر.

### 4. ما تغيّر

**أ) تأكيد الإرسال (`tira-api/src/send.rs`، `pages.rs`، `lib.rs`)**

`SentNotices`: مخزن في الذاكرة، `Mutex<HashMap<Uuid, SentNoticeEntry>>`، مدة 600
ثانية، سقف 64 إدخالًا (يُطرد الأقدم)، وكل إشعار مربوط بـ`mailbox_id`. عند نجاح
الإرسال يُصدر معرّفًا عشوائيًا ويُعيد `303` إلى `/mail/{id}?sent=<uuid>`.
`inbox_page` يقبل `InboxQuery { sent }`، ولا يعرض شيئًا إلا إذا كان المعرّف
**صادرًا من هذه العملية ولهذا الصندوق**. اللافتة تقول ما جرى فقط: قُبلت الرسالة
من المرحّل (250)، مع المستلم والموضوع والوقت و`host:port`، وتنصّ على أنها ليست
إثبات وصول وأنه لا يوجد مجلد «مُرسَل» لأن المخطط لا يحفظ نسخة مرسلة.

**ب) رأس الوارد والتذييل (`tira-frontend/assets/app.css`، `components.rs`)**

`.section-head`: `gap` 12، `padding: 0`، `h1` 20px (18px ≤1100px)، `meta` 11.5px،
وزر الإجراءات `min-height: 34px` بحشو 10px وخط 12.5px. `.sidebar__foot` صار:
«لا مزامنة خارجية — البريد يُستقبل عبر SMTP» و«الجلسة قابلة للإلغاء من الملف
الشخصي». اختبار `the_sidebar_does_not_claim_a_sync_that_does_not_exist` يرفض
عودة العبارتين القديمتين.

**ج) بطاقة سجلات النطاق (`components.rs`، `app.css`)**

العنوان «سجلات نطاق المرسل»، والتسميات `pass→موجود`، `warn→تنبيه`،
`fail→غير مستوفٍ` (الفشل يبقى فشلًا، لا يُلطَّف). أُضيف `.reader-auth__scope`:
«فحوص سجلات DNS المنشورة لهذا النطاق، لا نتيجة المصادقة لهذه الرسالة: لا يُفحص
توقيع DKIM ولا نتيجة SPF/DMARC الفعلية عند الاستلام.» وحالة الغياب:
«تعذّر قراءة سجلات DNS المخزّنة لهذا النطاق؛ لا يُعرض أي حكم بدلًا من ذلك.»
اختبار جديد `a_missing_record_is_not_dressed_up_as_a_pass` يثبّت ذلك.

**د) Jami3ati: حاجز إدارة البلاغات**

`backend/app/core/admin.py::require_admin` مشترك: مقارنة بزمن ثابت، و`503` إن لم
يُضبط الرمز (فشل مُغلَق، لا «الجميع مشرف»). `PATCH /reports/{id}/status` صار
يتطلب `X-Jami3ati-Admin`، والترتيب مقصود: **401 قبل 404** حتى لا يستطيع مجهول
سبر وجود المعرّفات. `notifications.py` استُبدل فيه الحاجز المحلي بالمشترك.
`backend/tests/test_reports_auth.py` (6 اختبارات) يثبت 201 للتقديم العام، و401
بلا رمز/برمز خاطئ، و200 بالرمز الصحيح، و503 عند غياب الإعداد.

**هـ) عيب الـ`textarea` (`components.rs`)**

`textarea … { (body) }` بدل `… ;` + `(body)`، مع تعليق يشرح الفخ. اختبار
`compose_screen_renders_draft_and_error` صار يتحقق من الترتيب: `name="body"` قبل
`</textarea>` قبل آخر `type="submit"`، ومن أن المسودة **داخل** العنصر.

### 5. الدليل الحيّ (بعد الإصلاح)

| ما تحقّق | النتيجة المقيسة |
|---|---|
| ارتفاع `.section-head` | **34px** (كان 44) — عرض نافذة 856px |
| `.sidebar__foot` | السطران الصادقان حرفيًا، بلا «مزامنة آمنة»/«جلسة آمنة» |
| بطاقة `tira.test` | «سجلات نطاق المرسل» + 7 فحوص بتسمية «موجود» + سطر النطاق (41px، 12px/21px) |
| بطاقة `uca.ma` (بلا أدلة) | «تعذّر قراءة سجلات DNS المخزّنة…» — صفر فحوص، صفر حكم |
| إرسال من الواجهة | `303` → `/mail/3333…?sent=8e07ec6b-6b7e-4fa9-8957-601f70349cc8` |
| اللافتة | `role="status"`، إلى `aicha@uca.ma`، الموضوع، 2026-11-17 14:24، المرحّل `127.0.0.1:1025` |
| الـsink | `010-yassine_at_tira.test.eml`، 457 بايت: `X-Envelope-To: aicha@uca.ma`، `MIME-Version: 1.0`، `Content-Type: text/plain; charset=utf-8`، والجسم base64 يفكّ إلى نص الرسالة حرفيًا |
| معرّف مزوَّر | `?sent=11111111-…` → **لا لافتة** |
| قيمة غير UUID | `?sent=not-a-uuid` → **لا لافتة** |
| إعادة تحميل بلا معرّف | **لا لافتة** |

### 6. البوابات الخمس (بالترتيب الحرفي)

| # | الأمر | النتيجة |
|---|---|---|
| 1 | `cargo fmt --all -- --check` | ✅ exit `0` |
| 2 | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ exit `0`، صفر تحذير |
| 3 | `cargo check --workspace --all-targets` | ✅ exit `0` |
| 4 | `.\scripts\dev.ps1 test-db` | ✅ **276 passed / 0 failed / 0 ignored**، 33 هدفًا، exit `0` |
| 5 | `%TEMP%\opencode\gate5.ps1` | ✅ exit `0` — `README_PATHS=18 MISSING=0`، `BAD_UTF8=0 BOM=0` |

الزيادة `+5` اختبارات: 3 في `tira-frontend` (لافتة الإرسال، تذييل صادق، تسمية
سجلات النطاق) و2 في `tira-api` (ربط الإشعار بالصندوق، سقف المخزن). اختبارات
قاعدة البيانات بقيت **56**.

### 7. الأرقام المقيسة بعد الإصلاح

| المقياس | قبل | بعد |
|---|---:|---:|
| اختبارات | 271 | **276** |
| منها `tira-frontend` | 27 | **31** |
| أسطر Rust غير الفارغة (56 ملفًا) | 18,843 | **19,126** |
| أسطر اختبارات التكامل (13 ملفًا) | 3,575 | 3,575 |
| `app.css` | 46,441 بايت / `app-css-d7cd0dfb47440eb6` | **51,532 بايت / `app-css-d7fb1f3644f8140f`** |
| `tira-api.exe` (debug) | 164,589,642 بايت = 156.96 MiB | **165,751,325 بايت = 158.07 MiB** (بلا overlay) |
| ترحيلات SQL / أسطرها | 10 / 575 | 10 / 575 |
| مسارات HTTP | 24 | 24 |

`DEEP_MAP.ar.md` حُدِّث في هذه الجولة: رأس «آخر قياس» إلى 17 نوفمبر، و`18744` →
`19126` (موضعان)، و`269` → `276` (خمسة مواضع)، و`27` → `31` للـ`tira-frontend`،
وثلاثة صفوف CSS، وحجم الـbinary، وسطر `§E` أُلحق بتصحيح تسمية لا وظيفة (النص
التاريخي بقي كما هو). `DEMO-SCRIPT.ar.md` المشهد 8 صار «سجلات نطاق المرسل» مع
وصف اللافتة الجديدة، و`README.md` 101/357 وُحِّدا إلى 276 مع تنويه مؤرَّخ.

### 8. الملفات والبصمات (SHA-256)

| الملف | بايت | SHA-256 |
|---|---:|---|
| `crates/tira-frontend/src/components.rs` | 64,186 | `943154BB1CE6ABFCE5924F1D487F8ABA867D605ED1B4F2C1A92D768E91698716` |
| `crates/tira-frontend/src/lib.rs` | 2,020 | `A987FDAA1ABD7BF5FADD1D93841F779F99E52FC5C30DBD85F70AFA72FC526479` |
| `crates/tira-frontend/assets/app.css` | 51,532 | `ABD964D8B59458D09EF8A9A6A34F93A03C809AA24238E3BDF22BBF7E052AA7AC` |
| `crates/tira-api/src/send.rs` | 49,717 | `EBEB1EA8C5B9B3D8F281970E1702FA9CB22DD4D450AA244B5F3C8AC8F35DF6F8` |
| `crates/tira-api/src/pages.rs` | 57,863 | `A174096DADC4EF809ACF2D31291B3F2B57A86AD3C8175B94963A6561F4E5D2AB` |
| `crates/tira-api/src/lib.rs` | 20,495 | `8C6AEA1E52D2D7C0330799CAA4196946825008977CD1E5863DE665EE35FC0A35` |
| `README.md` | 26,009 | `C7EC1023304699D62F899EB4129805AEE25AE1AFAC2E7689CA59EE4EC8C9F018` |
| `docs/DEEP_MAP.ar.md` | 72,058 | `89669BBD9A8B5D598388BEB4D9DCF8F83EF6D3A0B9043BBFDDCCA9BA5070BA82` |
| `docs/DEMO-SCRIPT.ar.md` | 7,734 | `5784044B10661AC7A5D7EAFF93BA2866D0041F4B9F2E8EAFF7C57DF30F251EF0` |
| Jami3ati `backend/app/core/admin.py` (جديد) | 861 | `FD3A2DF004D3DDAE789EDE07D611BBD8328423B4960EC1A3F6A2413B27D99636` |
| Jami3ati `backend/app/api/routes/reports.py` | 3,484 | `AEC29E2C09479EDAE160E27A1C9BC940D65492A1AB93FABB3026FF3703173D0D` |
| Jami3ati `backend/app/api/routes/notifications.py` | 4,252 | `4EE33E3AEFB6405EB157A864819E3C2AF0E4C0ED413DCF848146EC354F91C5B8` |
| Jami3ati `backend/tests/test_reports_auth.py` (جديد) | 2,943 | `D8992CF4C5FD0152935E9A66963A3501B6BD376E04253BF26741C8F2E6EFD3D3` |

نسخة الرجوع قبل الدفعة: `%TEMP%\opencode\fourfix-pre-20261117-135722\` (12 ملفًا
مُسطَّحًا). ملفات §9.46 نفسها (هذا السجل و`CORRECTIONS`) خارج النسخة بالطبع.

### 9. ما بقي متعمَّدًا (دَين مقيس، لا منسيّ)

- **`GET /api/v1/reports` عام ويعيد `reporter_email`.** لم يُغيَّر في هذه الدفعة:
  تغييره يكسر واجهة لوحة الإشراف التي تقرأه، ويحتاج قرارًا في المخطط
  (`InaccuracyReportResponse` يرث `InaccuracyReportCreate`). البند مفتوح ومُسجَّل.
- **لا مجلد «مُرسَل».** الإشعار يقول ذلك صراحةً؛ إضافة المجلد تحتاج ترحيلًا
  وجدولًا، لا لافتة.
- **الإشعارات في الذاكرة لكل عملية.** إعادة تشغيل `tira-api` تُسقطها (وهذا سلوك
  مقصود: لا ادّعاء دائم لحدث لحظي)، وتعدد العمليات يعني أن الإشعار قد لا يُقرأ
  من العملية الأخرى. مقبول للعرض، لا للإنتاج.
- **`DEEP_MAP §0`** يحمل صفوفًا لم تُعَد قياسها اليوم (اعتماديات مباشرة 21،
  `Cargo.lock` 242، سكربتات PowerShell 19، سجلات DNS). لم أبدّلها لأن قياسها
  خارج نطاق هذه الدفعة؛ تُركت كما هي بدل تحديث جزئي يوهم بالاكتمال.
- **`FINAL-REPORT.ar.md` و`JAMI3ATI-DEEP-PLAN.ar.md`** يحملان `269`/`250` وسجلات
  أدقّ: ملفان مؤرَّخان (12 نوفمبر ووقت كتابة الخطة)، وتعديلهما يجعل الماضي يدّعي
  قياسًا لم يقع.
- **البوابة 5 تطابق 27 موضعًا** من نمط الادّعاءات القديمة: كلها اقتباسات تاريخية
  داخل `CORRECTIONS.md` أو الملفين المؤرَّخين أعلاه، وصفر مسار مكسور، وصفر
  تشويه ترميز.
- **نموذج الطلب** ما زال ينتظر أسماء الفريق: `gate5` يطابق 8 أسطر بها
  `[الاسم الكامل]` / `[Nom complet]` في `docs/CANDIDATURE-Jami3ati.md`
  (7 أسطر) وسطر واحد في هذا السجل يوثّقها.

### 10. تصحيح ذاتي: تشغيل البوابة 5 على الحالة النهائية

أُعيد تشغيل `%TEMP%\opencode\gate5.ps1` **بعد** إلحاق هذا القسم، لأن التشغيل السابق
جرى قبل الإلحاق. النتيجة: **exit `0`**.

| المقياس | النتيجة |
|---|---|
| `STALE_MATCHES` | **27** — بلا تغيير عن §9.45 |
| `README_PATHS` | **18**، `MISSING=0` |
| الترميز | `BAD_UTF8=0`، `BOM=0` |
| `PLACEHOLDERS` | **9** |

تصحيح لِما ورد في البند الأخير من §9 أعلاه: العدد المقيس **9** لا 8. السبب أن سطر
§9 نفسه يذكر العنصرين الناقصين، فيُحتسب هو أيضًا. التوزيع: **سبعة** أسطر في
`docs/CANDIDATURE-Jami3ati.md` — وهي وحدها ما يجب ملؤه قبل الجمعة — وسطران في هذا
السجل يوثّقان النقص دون أن يملآه (أحدهما في §9.40). لا يُملأ عنصر في سجل التصحيحات
بدل نموذج الطلب.

وعدّاد §5 يطابق الرقم 271 في موضع واحد بـ`README.md` وموضع واحد بـ`DEEP_MAP.ar.md`:
كلاهما **إحالة مؤرَّخة** لا ادّعاء حالة — الأولى «كان 271» داخل تنويه 17 نوفمبر،
والثانية «بعد §9.43: 271/271» داخل قسم مؤرَّخ. الحالة الحالية المعلنة في الملفين
هي **276**، والبوابة لا تعتبر الإحالتين ادّعاءً متقادمًا (نمط الادّعاءات القديمة
لا يشملهما).

## 9.47 - 17 نوفمبر 2026 - أسماء الفريق، تصحيح الأرقام، تصحيح ادعاء التغطية، ونشر المستودع الخاص

### 1. ما جرى في هذه الجلسة

طُلب ملء أسماء الفريق ثم رفع المشروع إلى GitHub. الأسماء المعطاة خمسة: Sami Benali،
Ismail Benali، Elmoufid Hamza، Makhloufi Aymane، Bentatou Othman. القرار الصريح:
**ثلاثة صفوف فقط**، فوُزّعت الأدوار الثلاثة الموجودة كما هي دون اختراع دور رابع أو
خامس: Sami Benali (قيادة تقنية/معمارية)، Ismail Benali (واجهة أمامية)، Elmoufid Hamza
(تحقّق من المصادر). الاسمان الآخران لم يُذكرا في النموذج، لأن الجدول لا يحتمل دورًا
بلا مسؤولية موثّقة.

### 2. أرقام متقادمة في النموذج

| الموضع | قبل | بعد | عدد المواضع |
|---|---|---|---|
| اختبارات Tira في AR | 271 | **276** | 3 |
| اختبارات Tira في FR | 271 | **276** | 1 |
| اختبارات Jami3ati في AR | 51 | **57** | 2 |
| اختبارات Jami3ati في FR | 51 | **57** | 2 |

القياس المرجعي: بوابة `dev.ps1 test-db` → **276 passed / 0 failed / 0 ignored**،
وبوابة Jami3ati → **57 passed**. أُضيف أيضًا وصف حاجز الإدارة (`401` قبل `404`)
في الوصفين، لأنه صار سلوكًا مقيسًا ومختبَرًا.

### 3. ادعاء «4 مؤسسات متحقَّق منها» — لم يصمد للقياس

الادعاء كان في خمسة مواضع: AR §5، AR §10، FR §5، FR §10، وسطر قائمة التحقق.
قياس 17 نوفمبر 2026:

| المصدر | الكيانات | التفصيل |
|---|---|---|
| `src/data/seedData.ts` | **12** | 3 جامعات (2 `verified`، 1 `needs_review`) + 9 مؤسسات (8 `verified`، 1 `demo`) = **10 `verified`** |
| قاعدة `jami3ati_demo` | **3** | جامعتان + مؤسسة واحدة، كلها `verified` |
| `backend/app/seed/demo_data.json` | **3** | مطابق للقاعدة |

إذن «4» لم يكن عدد الكتالوج (12) ولا عدد القاعدة (3)؛ كان تعداد أربعة مصادر
مسمّاة. صُحّح النص ليقول الرقمين معًا ويفصل بينهما، بدل رقم واحد يصلح لسياق
ولا يصلح لآخر. أربعة `data_status` إضافية في الملف تعود إلى ثلاثة بلاغات
(`initialAnnouncements`)، لا إلى كيانات، ولذلك لم تُحتسب.

### 4. البوابة 5 بعد التعديلات

`%TEMP%\opencode\gate5.ps1` → **exit `0`**:

| المقياس | النتيجة |
|---|---|
| `STALE_MATCHES` | **27** (بلا تغيير) |
| `README_PATHS` | **18**، `MISSING=0` |
| الترميز | `BAD_UTF8=0`، `BOM=0` |
| `PLACEHOLDERS` | **2** — كلاهما إحالة تاريخية في هذا السجل (§9.40 و§9.46) |

### 5. نشر المستودع الخاص

- Git محمول: **MinGit 2.56.1** في `%TEMP%\opencode\mingit` (لا تثبيت نظامي).
- Credential Manager: **GCM 3.0.2**، بمخزن `dpapi` (مشفّر بمفتاح المستخدم).
- المستودع: **`Tarvokh/jami3ati`**، `private=true`، الفرع الافتراضي `main`.
- الـcommit: **`fa5d7c2`** — 263 ملفًا، 61,187 إدراجًا، 7.42 MiB.
  المؤلف: `Sami Benali <357124961+Tarvokh@users.noreply.github.com>`.
- الدفع: 329 object، حزمة 5.49 MiB، `* [new branch] main -> main`.
- ما استُبعد: `target/` (11.9 GB)، `node_modules/` (208 MB)، `data/`، `.env*`،
  الأرشيفات، `idea.txt` (يحوي بيانات دخول تجريبية)، و`screenshot mail.ma/` (35.9 MB).

### 6. سبب تعليق محاولات الدفع السابقة

`git push` كان يُشغَّل بلا `GIT_TERMINAL_PROMPT=0` وبلا مخزن اعتماد دائم. GCM
يفتح المتصفح لكل عملية `credential fill` ثم لا يحتفظ بالتوكن، فينتظر الفرع
إدخالًا على `stdin` بلا نهاية. الحل المُطبَّق: مخزن `dpapi` + تعطيل المطالبة
+ تمرير التوكن في عنوان الدفع لعملية واحدة، ثم حذف ملفات الاعتماد المؤقتة.
لم يُكتب أي توكن في `.git/config`.

### 7. ما بقي قبل الإرسال

- [ ] تعبئة استمارة الهاكاثون (لا حقل لاسم المشروع؛ الاسم في بداية «Idée de projet»).
- [ ] لصق رابط المستودع أو لقطة شاشة الواجهة.
- [ ] إعادة التحقق من الأرقام يوم الإرسال.
- [ ] الإرسال قبل **الجمعة 20 نوفمبر 2026، 16:30** بتوقيت المغرب.

## 9.48 - 17 نوفمبر 2026 - تصحيح مسار Credential Helper وتثبيت حالة الـremote

### 1. الخلل المقيس

كُتب `credential.helper` بمسار Windows خام:

```
C:\Users\HSYSTEM\AppData\Local\Temp\opencode\gcm\gcm.exe
```

عند عملية `store` يمرّره git إلى shell، فتُبتلع الـbackslashes ويصبح
`C:UsersHSYSTEM...gcm.exe`، ويُطبع:

```
gcm.exe store: line 1: C:UsersHSYSTEMAppDataLocalTempopencodegcmgcm.exe: command not found
```

المخزن نفسه نجح رغم الرسالة، لكن الإعداد كان هشًّا ولا يصحّ الاعتماد عليه.

### 2. الإصلاح والقياس

القيمة الجديدة: `!C:/Users/HSYSTEM/AppData/Local/Temp/opencode/gcm/gcm.exe`
(شرطة مائلة أمامية، وبادئة `!` لتنفيذ shell بلا تفسير backslash).

| المقياس | النتيجة |
|---|---|
| `credential fill` غير تفاعلي | **499 ms**، التوكن موجود، `stderr` فارغ |
| ملف المخزن | `C:\Users\HSYSTEM\.gcm\dpapi_store\git\https\github.com\Tarvokh.credential` — **399 بايت**، 22:03:18 |
| `cmdkey /list` | لا يعرض شيئًا — GCM يستخدم مخزنه الخاص المشفّر بـDPAPI، لا Windows Credential Manager |

### 3. حالة الـremote المقيسة

| المقياس | القيمة |
|---|---|
| `remote_main` | **`7b018d3`** (يطابق `HEAD` المحلي) |
| عدد الملفات | **263**، `truncated=false` |
| مسارات محظورة (`target/`، `node_modules/`، `.env`) | **0** |
| `private` | **true** |

- الـcommit الأول (لقطة النموذج): **`fa5d7c2`**.
- الـcommit الحامل لـ§9.47: **`7b018d3`** — ملفان، **+82/−5**.

### 4. ملاحظة على المتانة

مسار GCM يقع داخل `%TEMP%\opencode`؛ إذا نُظّف المجلد المؤقت يلزم إعادة تفويض
واحد عبر المتصفح. لا شيء من هذا يدخل المستودع: `.git/config` محلي وغير متعقَّب،
ولا يُكتب أي توكن في أي ملف داخل شجرة العمل.
