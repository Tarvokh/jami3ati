# سجل التدقيق الأمني الشامل — Tira Mail

**التاريخ:** 8 نوفمبر 2026 · **الطريقة:** أربعة تدقيقات قراءة-فقط متوازية (لا تعديل)،
كل finding بدليل `file:line`. **الحالة:** مسودة قرار — لم يُصلح شيء بعد.
**المرجع الملزم:** [`CORRECTIONS.md`](CORRECTIONS.md) · **الخريطة:** [`DEEP_MAP.ar.md`](DEEP_MAP.ar.md)

> honesty: «صفر ثغرات» لا يُضمن رياضياً لمنتج حي. هذا السجل يثبت ما فُحص، وما
> أُكّد سليماً، وما هو عيب حقيقي قابل للإصلاح، وما هو حدّ خارجي لا يُنجز بلا
> وصول/قرار منك. لا خلط بين هذه الأربعة.

---

## 0. الملخّص بالشدّة

| الشدّة | العدد | أبرزها |
|---|---|---|
| **High** | 10 | كوكي بلا Secure، logout لا يبطل الجلسة، إرهاق قرص بblob يتيم، DoS في DATA، OTP قابل لإعادة الاستخدام، stderr يسرّب السرّ، CI أخضر بلا تغطية تخزين، نسخة المحرك غير مدعومة، كلمة مرور القاعدة في argv، إضعاف مخزن الثقة |
| **Medium** | ~22 | معاملات Argon2 DoS، لا HSTS، لا Cache-Control للصفحات المصادَقة، تعداد حسابات عبر 429، OTP غير موصول، endpoint أدلة بلا مصادقة، I/O حاجب على runtime، مفتاح DKIM لدور الويب، blob لا يتحقق من السلامة، فجوة RLS لdomains/messages، حقن مرشّح --where، تسريب --database-url، قطع البثّ غير قابل للكشف، صلاحيات إخراج tira-seal، مسار عكسي صفري مرفوض، إسقاط إعدادات Rspamd، BOM في DNS |
| **Low** | ~15 | CSRF بالرأس فقط، تدوير كلمة المرور يبقي رموز المستأجر، /healthz يكشف، NoTls للقاعدة، timeouts للpool، فهرس.login، ترتيب أقفال، path_for، down-migrations، CI بلا --locked/audit، سكريبتات إثبات بكلمة مرور مكتوبة |

---

## 1. High — مؤكّدة وقابلة للإصلاح

### H-01 · كوكي الجلسة بلا `Secure` افتراضياً
`tira-api/src/pages.rs:411` — `TIRA_COOKIE_SECURE == "1"` اختياري. بلا remember،
كوكي 7 أيام يُرسل over any plain-HTTP ⇒ MITM/مورد http محقون يسرّب اعتماداً
طويل الأمد. `clear_session_redirect:429` لا يضبط Secure أيضاً.
**الإصلاح:** Secure افتراضياً، plain-HTTP opt-out صريح (`TIRA_COOKIE_INSECURE=1`)،
أو اشتقاق من scheme؛ بادئة `__Host-`.

### H-02 · logout لا يبطل الجلسة
`pages.rs:403-405` — `logout()` يمسح الكوكي فقط؛ صفّ `api_tokens` يبقى حيّاً 7 أيام
(`SESSION_TTL_DAYS:51`). رمز مُلتقَط قبل تسجيل الخروج يظل يعمل. الآلة موجودة
(`revoke_mailbox_session`, `identity.rs:233`).
**الإصلاح:** logout يأخذ PageAuth+State ويبطل `auth.0.token_id` قبل المسح.

### H-03 · إرهاق قرص بblob يتيم (مرسل خارجي، بلا مصادقة)
`tira-store/src/store.rs:269` — يكتب blob **قبل** المعاملة وفحص الحصة؛ عند
`QuotaExceeded` تُلغى المعاملة ويبقى blob. لا GC. RCPT يقبل صندوقاً ممتلئاً
(`resolve_recipient:477` يفحص الوجود فقط). رسائل فريدة ≤10MiB ⇒ كل واحدة 552
وتترك blob دائماً.
**الإصلاح:** فحص الحصة قبل put + حذف blob الجديد عند rollback + mark-and-sweep GC
على `messages.blob_key`.

### H-04 · DoS في طور DATA (بلا مهلة + buffer غير محدود)
`tira-smtp/src/server.rs:247,356,170` — `read_data` بلا `timeout()`؛ الحارس 300s
على الأوامر فقط. `read_until(b'\n')` يbuffer السطر كاملاً قبل فحص الحد ⇒ سطر 2GB
بلا newline يفجر الذاكرة رغم SIZE 10MiB. trickle يمسك مهمة للأبد. لا حدّ اتصالات.
**الإصلاح:** timeout على read_data، قراءات محدودة (RFC 5321 حدود السطر)، حدّ جلسات.
**قرار معماري مرتبط:** الخادم اليدوي «مرشح للإيقاف» (DEEP_MAP)؛ Stalwart هو المحرك.

### H-05 · OTP قابل لإعادة الاستخدام (ليس single-use)
`tira-auth/src/otp.rs:54-66` — عند النجاح `attempts_left -= 1` فقط؛ يبقى
`code_hash` والعداد >0 ⇒ نفس الرمز يتحقق حتى 5 مرات خلال TTL. لا اختبار نجاح-ثم-إعادة.
**الإصلاح:** صفّر attempts_left (أو امسح code_hash) عند النجاح + حرق ذرّي.
**ملاحظة:** لا أحد يستدعي otp::* بعدُ (كامنة).

### H-06 · stderr الابن غير المُحرَّر يُخزَّن (يكسر invariant «لا يُخزَّن أبداً»)
`tira-provision/src/engine.rs:363-374` يعيد stderr خام (2000 حرف) في
`EngineError::Apply`؛ `service.rs:137-143` يكتبه في PostgreSQL (`engine_error`)،
و`bin:48` يطبعه. إن ردّد CLI سجل NDJSON الخاطئ عند خطأ تحليل، وقعت كلمة مرور
الصندوق لمرة واحدة (`plan.rs:77`) في القاعدة والطرفية.
**الإصلاح:** exit code + صنف خطأ ثابت؛ إن احتُفظ بstderr فحرّر السرّ وأي مقاطع
`secret`/plan قبل مغادرة engine.rs.

### H-07 · CI أخضر بلا تغطية تخزين (open-relay test لا يعمل)
`.github/workflows/ci.yml:26-27` — `cargo test --workspace` بلا Postgres service
وبلا `TIRA_REQUIRE_DATABASE=1` ⇒ كل اختبارات التخزين SKIP (`persist.rs:28-40,181`)،
منها `unknown_recipient_is_refused_at_rcpt` (الإثبات الوحيد أن المعالج ليس open relay).
تعليق الملف يقول CI يضبط العلم؛ لا يضبطه.
**الإصلاح:** postgres service + العلمان.

### H-08 · مخزن بريد المحرك غير مدعوم (حاجز إطلاق)
`scripts/backup.ps1:10-14,285-288` يستثني مخزن Stalwart صراحةً. Stalwart هو مسار
SMTP/IMAP/JMAP الحيّ ⇒ صناديق المستخدمين غير قابلة للاسترجاع من الأرشيف؛ فقط
control plane + blob مدعومان.
**الإصلاح:** لقطة engine-native أو نافذة إيقاف موثّقة + توسيع الdrill. (جزئياً خارجي: المحرك متوقّف.)

### H-09 · كلمة مرور القاعدة في argv
`scripts/backup.ps1:113,126,150,200` و`backup-common.ps1:91` — `--dbname=$DatabaseUrl`
(فيها كلمة المرور) إلى psql/pg_dump؛ أي مستخدم محلّي يقرأ سطر الأوامر.
`milter-apply.ps1:29-31` يتجنب هذا عمداً.
**الإصلاح:** `PGPASSWORD`/`.pgpass` بACL مقيّد + URL بلا كلمة مرور.

### H-10 · إضعاف دائم لمخزن الثقة (سكريبت تطوير)
`scripts/sign-dev.ps1:65-77` — مفتاح توقيع كود **قابل للتصدير** 5 سنوات يُضاف إلى
`CurrentUser\Root` و`TrustedPublisher`؛ `:175-180` يجرد التواقيع ويضيف overlay
عشوائياً لتجاوز Smart App Control. لا مسار untrust/تنظيف.
**الإصلاح:** حارس «تطوير فقط» + مسار تنظيف؛ لا يُشغَّل على خادم/بناء.

---

## 2. Medium — مختارات قابلة للإصلاح (الكود/البنية)

| ID | الموضع | العيب | الإصلاح |
|---|---|---|---|
| M-01 | `tira-auth/password.rs:25`, `otp.rs:63` | التحقق يثق بمعاملات Argon2 من الhash المخزّن؛ hash عدائي `m=u32::MAX` ⇒ تخصيص TiB ⇒ DoS على login | ارفض m>65536, t>4, p>4 قبل التحقق |
| M-02 | `tira-api/lib.rs:162` | لا HSTS ⇒ SSL-strip مع H-01 | `max-age=31536000; includeSubDomains` عند TLS |
| M-03 | `tira-api/pages.rs:58` (html()) | صفحات مصادَقة بلا Cache-Control/Vary ⇒ قابلة للتخزين heuristic | no_store + `Vary: Cookie` على كل html() |
| M-04 | `pages.rs:287-328` | login throttle check-then-act + لا سقف per-IP/per-email؛ تعداد عبر 429 (معروف⇒429 فوري، مجهول⇒Argon2+401) | increment-before-verify + bucket مستقل عن الوجود |
| M-05 | `tira-api` (لا otp) | OTP موجود لكن غير موصول بlogin (password-only) | وصل التحدي أو وقف الادعاء ب2FA — **قرار منتج** |
| M-06 | `lib.rs:294`→`proof.rs:149` | endpoint أدلة بلا مصادقة يكشف دور DB/bypassrls/حجم التدقيق/هل RLS مكسور + بلا throttle | مصادقة أو loopback-only + cache |
| M-07 | `pages.rs:111,548`, `scoped.rs:296` | I/O ملف حاجب + MIME parse على async runtime (100×256KB/طلب) ⇒ تجويع tokio | spawn_blocking أو سقف snippet |
| M-08 | `migrations/0004_rls.sql:61` | `SELECT ON domains TO tira_app` يشمل `dkim_private_enc` (كامناً) | grants على مستوى العمود أو جدول مفاتيح owner-only |
| M-09 | `tira-store/blob.rs:103,121` | put() يثق بالوجود لا السلامة (bit-rot/pre-plant يسمّم العنوان دائماً)؛ لا fsync قبل rename | verify-or-rewrite + fsync tmp |
| M-010 | `store.rs:272,344` | صفّ تدقيق واحد لtargets[0].tenant_id ⇒ لا أثر في المستأجر B (CNDP) | صفّ لكل مستأجر متميّز |
| M-011 | `tests/rls.rs` + `0001:50` | سياسات domains/messages غير مُختبرة مباشرة؛ لا FK مركّب يضمن تطابق tenant | اختبارات بلا مسند + FK `(mailbox_id,tenant_id)` |
| M-012 | `tira-provision/engine.rs:343` | ردّ المحرك غير مُتحقَّق: counters=0 عند غياب الملخّص، failed>0 مع exit 0 = نجاح | اشترط ملخّصاً، خطأ إن failed>0، created+updated≥lines |
| M-013 | `engine.rs:204,216` | حقن مرشّح `--where name={local_part}` (مسافات/=/quotes تنجو) ⇒ id حساب آخر | فرض `^[a-z0-9._+-]{1,64}$` + إعادة تحقق |
| M-014 | `bin/tira-provision.rs:176` | `--database-url` يسرّب الاعتماد في argv (بينما يرفض --password) | احذف العلم؛ env/stdin فقط |
| M-015 | `engine.rs:81,320` | `STALWART_INSECURE=1` يعطّل تحقق الشهادة؛ default http | ارفض --insecure/non-https إلا loopback |
| M-016 | `tira-vault/lib.rs:211,238` | قطع البثّ غير قابل للكشف؛ verify يعطي طمأنينة كاذبة (أي prefix صحيح) |صادِق final-frame flag + الطول في AAD — **يغيّر الصيغة، قرار** |
| M-017 | `bin/tira-seal.rs:133` | إخراج plaintext ب0644، يقتطع --force قبل المصادقة، يترك partial عند فشل frame | 0600 + temp + rename عند النجاح فقط + unlink عند الخطأ |
| M-018 | `tira-smtp/server.rs:341` | `MAIL FROM:<>` مرفوض (501) ⇒ لا DSN/bounce (خرق RFC 5321، فقد صامت) | اقبل المسار العكسي الصفري |
| M-019 | `deploy/rspamd/README.md:44` | ينسخ `*.inc` فقط؛ 3 `.conf` لا تُثبَّت ⇒ الإعدادات عاطلة؛ multimap envelope قابل للتزوير | صحّح مسار التثبيت؛ gate على SPF/DMARC |
| M-020 | `deploy/dns/*.zone:1`, `*.dns.json:1` | UTF-8 BOM (EF BB BF) يفسد أول اسم DKIM وJSON الصارم | أعد التوليد بلا BOM + فحص CI |
| M-021 | `scripts/backup.ps1:176,304` | manifest.sha256 غير مُصادَق؛ preflight يفحص existence/size لا hash؛ لا offsite/retention/schedule | قوِّ السلامة + جدولة + offsite |
| M-022 | `scripts/milter-apply.ps1:156` | يقرأ الكائن ويطبع PASS بلا مقارنة ⇒ milter معطّل/fail-open يبلغ نجاحاً | أضف assertions |

---

## 3. Low — تحسينات (لا تحجب الإطلاق)

CSRF بالرأس فقط (`lib.rs:97`، غير قابل للاستغلال حالياً مع SameSite=Strict)؛
تدوير كلمة المرور يبقي رموز المستأجر (`identity.rs:255`)؛ `/healthz` يكشف SMTP/uptime؛
`NoTls` للقاعدة (`db.rs:36`، localhost pilot)؛ لا timeouts للpool (`db.rs:42`)؛
فهرس login تعبير غير مفهرس (`identity.rs:79`)؛ ترتيب أقفال بترتيب المغلف ⇒ deadlock
(`store.rs:278`)؛ `path_for` لا يفرض hex (`blob.rs:76`، غير قابل للوصول عن بعد)؛
لا down-migrations؛ CI بلا `--locked`/`permissions`/SHA-pinning/`cargo audit`؛
`stalwart-admin.ps1:114` ملف مؤقّت بلا مسح + env باقٍ؛ سكريبتات إثبات
`TrustAllCertificates` + كلمات مرور مكتوبة (`jmap/imap/smtp-proof.ps1`)؛
`IssuedOtp` Debug يكشف code؛ `now_unix` unwrap_or(0) يفشل open؛ لا zeroize.

---

## 4. أُكّد سليماً — لا «تصلحه» (تجنّب over-fixing)

- **عزل المستأجر/الصندوق على كل مسار قراءة:** `SET LOCAL ROLE tira_app` + GUC +
  مسندات صريحة؛ cross-tenant وwrong-scope ⇒ 404؛ مُثبت ب`tests/read.rs`.
- **SQL injection:** كل استعلام parameterized؛ ORDER BY ثابت؛ LIMIT/OFFSET bound.
- **سباق الحصة:** `FOR UPDATE` + `CHECK (used_bytes <= quota_bytes)` ⇒ حتى خطأ app لا يملأ فوق.
- **الرموز:** 32-byte OsRng، SHA-256 فقط، expiry+revocation داخل `tira_resolve_token`،
  deny-all على api_tokens/message_recipients مُثبت ب`rls.rs`.
- **XSS:** Maud يهرّب كل interpolation؛ PreEscaped على Icon::svg() الثابت فقط؛
  الأجسام نص؛ اختبار عنوان عدائي يمر.
- **lettre يتحقق من الشهادات** (Tls::Required، accept_invalid=false، min TLS1.2)؛
  لا header injection (Mailbox parsing + RFC2047).
- **Vault:** salt جديد لكل seal ⇒ لا nonce reuse عبر الأختام؛ nonce عشوائي 96-bit
  أقل بكثير من حدّ 2^32؛ index-as-AAD مُتحقَّق قبل أي plaintext؛ bounds-checks + سقف 64MiB.
- **§9.14 صحيح ومُثبت:** اعتماد عبر env الابن فقط، خطة عبر stdin مع EOF،
  `mask_secret` ورفض `--password` يعملان، serde يمنع حقن NDJSON.
- **catch_unwind يعزل فعلاً**؛ dot-unstuffing صحيح؛ حدّ المستلمين قبل push؛
  لا panic على malformed؛ لا unsafe في smtp/module.
- **النسخ/الاسترجاع سليم بنيوياً:** seal→verify→ثم حذف plaintext؛ trap يزيل الناقص؛
  drill حقيقي (scratch guard، hash+size، count، schema، per-blob re-hash).
- **Rspamd fail-closed صحيح:** loopback-only milter، tempFailOnError:true،
  soft_reject_on_timeout. (تحذير: commandTimeout 10s < task_timeout 15s.)

---

## 5. حدود خارجية — لا تُنجز بلا وصول/قرار منك

(مطابقة ل`DEEP_MAP.ar.md` §14)
- **DNS عام + مسجّل/مزوّد:** لا وصول. توليد المنطقة والتحقق الرياضي ✅؛ النشر ⛔.
- **شهادة إنتاج (ACME):** المحرك بشهادة ذاتية التوقيع. يحتاج نطاقاً عاماً.
- **سمعة IP + تدفئة 6 أسابيع + تقييم Gmail/Outlook:** عالم خارجي، لا كود.
- **Rspamd وقت التشغيل:** لا حزمة ولا Redis هنا؛ الإعدادات جاهزة معطّلة.
- **محامٍ مغربي (قانون 09-08/CNDP):** وثيقة الامتثال تحتاج مراجعة قانونية.
- **قرارات العمل D2/D3:** ترخيص AGPL (الأخطر تجارياً) والتسعير — لك لا لي.
- **خادم/حساب سحابي:** النشر الفعلي يحتاج بنية تحتية منك.

---

## 6. الموجات المقترحة (للتنفيذ بعد التفويض)

- **موجة 1 — أمان High واضح (خطر منخفض، لا يغيّر معمارية):** H-01, H-02, H-03, H-05,
  H-06, H-07, H-09 + M-01, M-02, M-03, M-06, M-08, M-09, M-11. كل وحدة: إصلاح +
  اختبار + بوابة + لقطة + §9.x.
- **موجة 2 — قرار معماري:** H-04/M-18 (الخادم اليدوي: تعطيل افتراضي أم إصلاح؟)،
  M-05 (وصل OTP؟)، M-16 (صيغة vault؟). **تحتاج تفويضك.**
- **موجة 3 — تشغيل/بنية:** H-08, H-10, M-04, M-07, M-10, M-12..M-15, M-17, M-19..M-22.
- **موجة 4 — Low + تقوية CI.**
- **خارجي:** §5 يبقى معلناً، لا يُنفَّذ بلا وصول.