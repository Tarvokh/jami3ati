# تقرير الحركة والتغذية الراجعة والوصولية — Tira Mail

**التاريخ:** 8 نوفمبر 2026 · **الوحدة:** §9.15 في [`CORRECTIONS.md`](CORRECTIONS.md)
**النطاق:** `tira-frontend` + `tira-api` (مسار العرض فقط، لا منطق بريد)

---

## 1. لماذا هذه الوحدة

اتجاه «هدوء» (§9.10) أعطى الواجهة شكلاً ثابتاً: ألوان، خطوط، تخطيط. لكن الشكل
الثابت وحده لا يكفي لمنتج يريد أن يكون «سلساً وقوياً وبلا أخطاء خفية». كانت هناك
ثلاث فجوات صامتة لا يراها الفحص البصري السريع:

1. **حركة لا يحكمها تفضيل المستخدم.** عائلة `AnimatedIcon` كانت تستخدم `<animate>`
   و`<animateTransform>` (SMIL) داخل SVG. هذه الحركة **لا تخضع** لـ
   `prefers-reduced-motion` في CSS إطلاقاً — فمن يطلب تقليل الحركة (حساسية vestibular،
   أو بطء جهاز) كان يرى الأيقونات تتحرك رغم طلبه. هذا خطأ وصولية خفيّ لا يُكشف
   إلا لمن يعرف أن SMIL خارج ولاية CSS.

2. **مؤشر تحميل ميت.** قواعد `.htmx-indicator` كانت معرّفة لكن لا عنصر في الصفحة
   يحملها، وكان `HTMX_CONFIG` يضبط `includeIndicatorStyles:false` (لأن حقن HTMX
   لـ`<style>` تكسره CSP). فالنتيجة: لا تغذية راجعة بصرية أثناء التنقّل المعزّز،
   والمستخدم لا يعرف أن الصفحة تُحمَّل.

3. **لا تغذية راجعة لقارئ الشاشة.** CSS وحده لا يستطيع تحريك منطقة حيّة (live region):
   تبديل `display` ليس تغييراً في DOM يعلنه المتصفح. فمستخدم قارئ الشاشة كان يتنقّل
   بلا أي إعلان عن «جارٍ التحميل» أو «تم» أو «فشل».

بالإضافة إلى ذلك: لا نظام حركة قابل للتطور (مدد ومنحنيات مبعثرة كأرقام سحرية)،
ولا كشف دخول منسّق للمحتوى.

## 2. ماذا فعلت — المبادئ لا الترقيع

قررت **الإزالة من المنبع** حيثما أمكن، لا الترقيع فوق العيب:

### 2.1 إزالة SMIL بالكامل (icons.rs)

حُذفت عائلة `AnimatedIcon` enum وimpl و`pub fn animated()` وثوابت `LINE_MD_*`
السبعة (كلها SMIL). بقيت `Icon` (27 شكلاً) وثوابت `LUCIDE_*` بايتاً ببايت. كل
الحركة الآن في `app.css` حيث يحكمها `prefers-reduced-motion` فعلاً. علامتا التجارية
في `components.rs` و`pages.rs` صارتا تستخدم `Icon::Mail.svg()` بدل
`AnimatedIcon::Email.svg()`.

اختباران جديدان يقفلان هذا invariant: كل أيقونة SVG مكتفية ذاتياً زخرفية
(`aria-hidden="true"`، بلا `xlink`)، و**لا أيقونة تشحن حركة SMIL غير قابلة للحكم**
(لا `<animate`، لا `<animateTransform`، لا `<set`، لا `repeatCount`، لا `<script`).
أي انحدار يعيد SMIL يكسر الاختبار فوراً.

### 2.2 نظام حركة CSS قابل للتطور (app.css)

أُضيفت tokens في `:root` بدل الأرقام السحرية:

```
--ease-out / --ease-in-out   (منحنيات ثابتة)
--dur-1..4                   (120/200/320/480ms)
--reveal-y: 8px / --stagger: 55ms
```

- **دخول منسّق:** `.content > *` و`.auth__inner > *` تكشف بـ`reveal-up` (شفافية +
  إزاحة رأسية `var(--reveal-y)`) مع تأخير متدرّج `nth-child`. الإزاحة **رأسية**
  لا أفقية، فهي محايدة في RTL. `animation-fill-mode: backwards` يمسك حالة البداية
  خلال التأخير ثم **لا يترك transform** بعدها — وهذا مقصود: أي transform دائم على
  الأب يكسر `position:sticky` لأبناء `.reader-bar`/`.rail`.
- **تفاعلات دقيقة:** `.btn:active` ينزل 1px، وانتقالات `.icon-btn`/`.row-link`/
  `.folder`/`.skip-link`/الحقول صارت تستخدم الـtokens. أُضيف `.row-link:focus-visible`
  بمساواة hover (لا يفقد مستخدم لوحة المفاتيح التغذية الراجعة).
- **مؤشر حقيقي:** `.htmx-indicator` يستريح `opacity:0; visibility:hidden` (لا
  `display:none`، ليبقى الانتقال ناعماً ومحكوماً بتقليل الحركة)، ويظهر عند
  `.htmx-request`. `.nav-indicator` حبّة ثابتة أعلى المحتوى (`position:fixed`) فيها
  `.spinner` دوّار ونص «جارٍ التحميل…».

### 2.3 تغذية راجعة وصولية صادقة (nav-a11y.js + layout.rs)

الطريق الوحيد الصادق لمنطقة حيّة بلا تزييف هو **سكريبت تحسين تدريجي صغير مستضاف
ذاتياً**. `layout.rs` صار يشحن:

- `div#nav-indicator.htmx-indicator.nav-indicator[aria-hidden=true]` (الحبّة +
  spinner) — `aria-hidden` لأن إعلانها لقارئ الشاشة سيكون مزدوجاً مع المنطقة الحيّة.
- `p#nav-status.sr-only[role=status][aria-live=polite]` — منطقة حيّة دائمة.
- `body hx-indicator="#nav-indicator"` (موروث لكل التنقّل المعزّز).
- `<script src="/assets/nav-a11y.js" defer>`.

`nav-a11y.js` (59 سطراً، IIFE، `"use strict"`) يستمع على `document` لأحداث HTMX
ويعيد استعلام العناصر كل حدث (لأن `body.innerHTML` يُبدَّل عند التعزيز، فالمستمع
على `document` هو الوحيد الذي ينجو):

| الحدث | الفعل |
|---|---|
| `htmx:beforeRequest` | `aria-busy=true` على `#main` + «جارٍ التحميل…» |
| `htmx:afterSettle` | `aria-busy=false` + `document.title` |
| `htmx:responseError` | `aria-busy=false` + «تعذّر تحميل الصفحة. أعد المحاولة.» |
| `htmx:sendError` | `aria-busy=false` + «تعذّر الاتصال بالخادم. أعد المحاولة.» |

**تحسين تدريجي:** بلا JS، الروابط تعمل تنقّلاً كاملاً والصفحات تُحمَّل من الخادم؛
المنطقة الحيّة تحسين إضافي لا اعتماد. السكريبت مستضاف ذاتياً (لا CDN، لا مورد
خارجي)، وCSP `script-src 'self'` يسمح به بلا `unsafe-inline`.

### 2.4 تقليل الحركة يحكم كل شيء

كتلة `@media (prefers-reduced-motion: reduce)` في ذيل `app.css`:

```
*, *::before, *::after {
  animation-duration: 0.001ms !important;
  animation-delay: 0ms !important;
  animation-iteration-count: 1 !important;
  transition-duration: 0.001ms !important;
  transition-delay: 0ms !important;
  scroll-behavior: auto !important;
}
.spinner { animation: none !important; }
```

فالـspinner يصير حلقة ثابتة، والنص «جارٍ التحميل…» وإعلان المنطقة الحيّة ما زالا
يحملان الحالة. لا حركة تفلت من التفضيل بعد الآن.

## 3. الملفات المتغيّرة (9)

| الملف | التغيير |
|---|---|
| `tira-frontend/src/icons.rs` | إزالة `AnimatedIcon` + SMIL؛ اختباران يقفلان invariant |
| `tira-frontend/src/components.rs` | علامتا التجارية → `Icon::Mail` |
| `tira-frontend/src/pages.rs` | علامة دخول → `Icon::Mail` |
| `tira-frontend/assets/app.css` | tokens + keyframes + دخول + تفاعلات + مؤشر + تقليل الحركة |
| `tira-frontend/src/layout.rs` | مؤشر + منطقة حيّة + `hx-indicator` + سكريبت؛ تأكيدات اختبار |
| `tira-frontend/assets/nav-a11y.js` | **جديد** — محسّن وصولي مستضاف ذاتياً |
| `tira-frontend/src/assets.rs` | تضمين السكريبت + `NAV_A11Y_ETAG` + اختبار |
| `tira-api/src/lib.rs` | مسار `/assets/nav-a11y.js` + تحديث وثيقة CSP |
| `tira-api/src/pages.rs` | معالج `asset_nav_a11y` (content-type + cache + ETag) |

## 4. التحقق

### 4.1 البوابات

- `cargo fmt --all -- --check` = 0
- `cargo clippy --workspace --all-targets -D warnings` = 0
- **147/147 اختباراً** على PostgreSQL حقيقي (`127.0.0.1:55432/tira_test`، 0 فشل،
  0 تخطّي). 147 = 145 السابقة + 1 (icons) + 1 (assets).

### 4.2 التحقق الحيّ (متصفح، خادم `tira-api` على `127.0.0.1:8080`)

| الفحص | النتيجة |
|---|---|
| `/healthz` · `/login` | 200 |
| `/assets/nav-a11y.js` | 200، `application/javascript; charset=utf-8`، `max-age=300`، ETag `"nav-a11y-…"` |
| CSP | `default-src 'self'` … `script-src 'self'` … بلا `unsafe-inline` (لم تتغيّر) |
| رسائل console | **لا شيء** (لا خرق CSP، لا مورد خارجي) |
| السكريبتات في الصفحة | `htmx.min.js` + `nav-a11y.js` فقط (مستضافان ذاتياً) |
| عناصر SMIL (`animate/animateTransform/set`) | **0** |
| عناصر `[style]` inline | **0** |
| `#nav-indicator` | `position:fixed`، يستريح `opacity:0/visibility:hidden`، `aria-hidden=true` |
| المؤشر عند `.htmx-request` (بعد قمع الانتقال) | `opacity:1`، `visibility:visible`، `translateY(0)` |
| `#nav-status` | `role=status`، `aria-live=polite`، `sr-only` |
| `.spinner` | `animation: spin 0.7s` |
| دخول المحتوى | `reveal-up 0.32s`، `fill-mode: backwards` |
| wiring المحسّن | `beforeRequest`→busy=true/«جارٍ التحميل…»، `afterSettle`→busy=false/العنوان، `responseError`→رسالة الخطأ |
| نموذج الدخول | `method=post action=/login hx-boost=false` (يعمل بلا JS) |
| كتلة تقليل الحركة | موجودة: مدد ~0، تأخير 0، `.spinner{animation:none}` |

**ملاحظة منهجية صادقة:** سطح المتصفح داخل التطبيق كان مخفياً (`visibilityState=hidden`)
فعلّق rendering الانتقالات؛ القراءة اللحظية لـ`opacity` بعد إضافة الصنف أعادت 0
(إطار بداية الانتقال). تحقّقت من القيمة المستهدفة بقمع الانتقال عبر CSSOM
(`element.style.transition='none'`، مسموح تحت CSP) فأعادت 1/visible/translateY(0)،
ومن قواعد الـstylesheet المطابِقة مباشرة. فالقواعد صحيحة؛ الصفر كان أثر توقيت
لا عيباً.

## 5. حدود صادقة

- **لم أختبر حركة فعلية بالعين** (لقطة شاشة) لأن سطح المتصفح المخفي لا يوفّر
  viewport مرئياً؛ تحقّقت بنيوياً وحسابياً (computed styles + قواعد + wiring).
- **لا git** (عامل الحافلة 1، لقطات zip). نقطة الرجوع: لقطة §9.14.
- هذه الوحدة **مسار عرض فقط**: لا تمسّ SMTP/IMAP/التخزين/المصادقة. فجوات
  الإطلاق الخارجية (شهادة إنتاج، DNS عام، سمعة IP، Rspamd وقت التشغيل) خارج
  نطاقها ومعلنة في `DEEP_MAP.ar.md` §5/§14.