# Tira Mail — Rspamd (اختياري، غير مفعّل افتراضياً)

هذا المجلد **ليس** جزءاً من المسار الافتراضي. الافتراضي في Tira Mail هو مصفّي
Stalwart المدمج + SPF/DKIM/DMARC، لأن ذلك يعمل اليوم ومُثبت. Rspamd يُضاف عندما
يبرره الحجم أو نوع الهجمات، لا لأنه "يبدو احترافياً".

## ما يضيفه Rspamd فعلاً

| القدرة | Stalwart المدمج | Rspamd |
|---|---|---|
| SPF / DKIM / DMARC | نعم (محرّك Stalwart) | نعم (وحدة مستقلة) |
| قواعد محتوى | محدودة | Hyperscan + قواعد Lua |
| Bayes | لا | نعم (يحتاج Redis أو SQLite) |
| DNSBL / SURBL | جزئي | نعم، مع تحكّم في الأوزان |
| Fuzzy hashing | لا | نعم (يحتاج Redis) |
| مكافحة فيروسات | لا | عبر `antivirus` + ClamAV |
| Greylisting | لا | نعم (يحتاج Redis) |

الثمن: عملية إضافية، Redis غالباً، ونقطة فشل جديدة بين SMTP والقرص.

## البنية

```
المرسل → Stalwart :25
            │  Milter (TCP 127.0.0.1:11332)
            ▼
         Rspamd (rspamd_proxy, self_scan)
            │  DATA + connect/mail/rcpt
            ▼
      القرار: accept / add header / reject
```

- Rspamd يستمع على **loopback فقط**. بروتوكول Milter بلا مصادقة ولا تشفير؛
  فتح 11332 على الشبكة يعني فتح ماسح بريد لأي أحد.
- Stalwart هو من يوقّع DKIM. `dkim_signing.conf` معطّل عمداً.
- المراحل المطلوبة ليست `data` وحدها: بدون `connect` لا يعرف Rspamd عنوان IP
  العميل الحقيقي، وبدون `mail` لا يعرف المُرسل الظرفي (envelope)، فيصبح فحص
  SPF بلا معنى. لذلك خطة `deploy/stalwart/milter.ndjson` تشترك في
  `connect, mail, rcpt, data`.

## التركيب (Linux، عقدة واحدة)

الطريقة الموصى بها — سكربت التثبيت (ينسخ كل الإعدادات، يتحقق، ثم يفشل مغلقاً):

```bash
sudo deploy/rspamd/install.sh            # ينسخ + configtest، بلا إعادة تشغيل
sudo deploy/rspamd/install.sh --restart  # يعيد التشغيل بعد نجاح الفحص فقط
```

`install.sh` ينسخ **محتوى `local.d/` بالكامل** (`*.inc` و`*.conf` معاً، وملف
`redis.conf.disabled` المعطّل عمداً بامتداده) و`maps/`، ثم يتحقق أن الملفات
الثلاثة `.conf` (`actions.conf` و`dkim_signing.conf` و`multimap.conf`) وصلت
فعلاً، ثم يفشل مغلقاً إن لم يجتز `rspamadm configtest` (لا يعيد التشغيل حينها،
فيحتفظ Rspamd العامل بإعداداته السابقة). هذا يصلح **M-19**: النسخ اليدوي لـ`*.inc`
وحده كان يُسقط الثلاثة `.conf` فتبقى الإعدادات عاطلة — لا عتبات صريحة، ولا حسم
لتعطيل DKIM، وخرائط multimap الظرفية (envelope) غير موصولة فيصبح التحكم بالمرسل
الظرفي بلا أثر.

الخطوات اليدوية المكافئة (إن لم تستخدم السكربت):

1. ثبّت إصداراً محدّداً من Rspamd (لا `latest`) وثبّته في صورة/حزمة مُدارة.
2. انسخ محتوى `local.d/` بالكامل إلى `/etc/rspamd/local.d/` — `*.inc` و`*.conf`
   معاً (لا `*.inc` وحده). ملف `redis.conf.disabled` يبقى معطّلاً حتى تُعيد
   تسميته إلى `redis.conf` بعد تثبيت Redis.
3. انسخ `maps/` إلى `/etc/rspamd/maps/` واضبط الملكية `rspamd:rspamd`.
4. تحقّق أن `actions.conf` و`dkim_signing.conf` و`multimap.conf` موجودة في
   `/etc/rspamd/local.d/`، ثم `rspamadm configtest`؛ **لا تُعد التشغيل إلا إذا
   نجح الفحص** (`systemctl restart rspamd`).
5. تحقّق أن 11332 يستمع على loopback فقط:
   `ss -lntp | grep 11332` يجب أن يُظهر `127.0.0.1:11332` لا `0.0.0.0`.
6. شغّل `scripts/milter-apply.ps1 -Enable` (يتحقق من TCP أولاً، ثم يطبّق
   الخطة بـ `--dry-run` قبل التطبيق الحقيقي).
7. أرسل رسالة GTUBE وتحقّق من رأس `X-Spam` ومن السجل.

## قرار الفشل: fail-closed أم fail-open؟

الخطة الافتراضية `tempFailOnError = true`، أي إذا مات Rspamd تُرفض الرسالة
مؤقتاً (4xx) ويعيد المرسل المحاولة. هذا يحمي من تسرّب بريد غير مفحوص، لكنه
يحوّل عطل Rspamd إلى تأخير بريد.

للتجربة الأولى استخدم `scripts/milter-apply.ps1 -Enable -FailOpen` ثم انتقل
إلى fail-closed بعد أسبوع بلا حوادث. لا تترك fail-open في الإنتاج الدائم:
أي عطل صامت يصبح ثغرة.

## ما لم يُثبت هنا

- لم يُشغَّل Rspamd على هذه العقدة (لا حزمة، لا Redis)، فالإعدادات مراجعة
  وثائق فقط وليست مُختبرة وقت التشغيل. أما `install.sh` ففُحص نحوياً (`bash -n`)
  واختُبر وظيفياً بـ`rspamadm` بديل (stub): النسخ الكامل، وحارس وجود الثلاثة
  `.conf`، والفشل المغلق عند رسوب `configtest` وعند غياب الأداة — كلها مؤكّدة.
  ما لم يُختبر هو دلالات إعدادات Rspamd الحقيقية وقت التشغيل.
- لم يُقس أداء المراحل الأربع تحت حمل حقيقي.
- قواعد Bayes لا تُنشأ من فراغ: تحتاج corpus مُصنّفاً من بريد المؤسسة نفسه.

## المراجع

- Milter في Rspamd: <https://docs.rspamd.com/workers/rspamd_proxy>
- العتبات: <https://docs.rspamd.com/configuration/metrics>
- القوائم: <https://docs.rspamd.com/modules/multimap>
- الخيارات العامة: <https://docs.rspamd.com/configuration/options>
