# أيقونات Tira Mail

## لماذا لا نستخدم مكتبات الحركة الجاهزة

الواجهة مكتوبة بـ **Rust** (Leptos لاحقاً)، والمكتبات التي اقترحها المستخدم
تعتمد على React أو Vue:

| المصدر | التقنية | الاستخدام عندنا |
|---|---|---|
| `lucide-animated.com` | React + Framer Motion | غير قابل للتضمين مباشرة |
| `animateicons.in` | React | غير قابل للتضمين مباشرة |
| `heroicons-animated.com` | React/Framer | غير قابل للتضمين مباشرة |
| `react-useanimations` | React | غير قابل للتضمين مباشرة |
| `lucide-motion-vue` | Vue | غير قابل للتضمين مباشرة |
| `movingicons.dev` / `itshover.com` | React | غير قابل للتضمين مباشرة |
| **`iconify line-md`** | **SVG + SMIL** | **قابل للتضمين كما هو** |

`line-md` هو الوحيد الذي لا يحتاج JavaScript: حركاته `<animate>` داخل الـ SVG،
وتعمل في المتصفحات الحالية بلا أي إطار عمل.

## ما اعتُمد فعلاً

| العائلة | الرخصة | الملف | الاستخدام |
|---|---|---|---|
| **Lucide** | ISC | `crates/tira-frontend/src/icons.rs` | أيقونات ثابتة: الوارد، الإرسال، الأرشيف، الحذف، الإعدادات، البحث، النجمة، المستخدم، الدرع، الجرس |
| **line-md** (Cyberalien) | Apache-2.0 | نفس الملف | 7 أيقونات متحركة: البريد، الجرس، البحث، الحساب، القائمة، التحميل، السهم |

كل الأيقونات **مضمّنة كنصوص SVG** داخل الـ crate. لا يوجد أي طلب إلى
`api.iconify.design` أو أي CDN: الواجهة تعمل بلا إنترنت، ولا تُسرّب عنوان IP
المستخدم إلى طرف ثالث — وهذا مطلب مرتبط بالقانون 09-08 وCNDP.

## قواعد الاستخدام

1. **الحركة ليست المعلومة الوحيدة.** كل أيقونة متحركة `aria-hidden="true"`،
   والنص المجاور هو الذي يحمل المعنى (مثلاً «جارٍ مزامنة الصندوق…»).
2. **الحركة عند الإدخال فقط، لا في حلقة.** الاستثناء الوحيد هو
   `loading-twotone-loop` أثناء عملية جارية فعلاً.
3. **حجم 1em.** الأيقونة ترث حجم الخط ولون `currentColor`، فلا حاجة إلى
   متغيّر لون خاص لكل أيقونة.
4. **لا أيقونة بلا اسم.** الأزرار التي تحتوي أيقونة فقط يجب أن تحمل
   `aria-label` (انظر `demo_page()`).
5. **RTL أولاً.** الأيقونات الاتجاهية (`ChevronLeft/Right`) تُختار حسب اتجاه
   النص، لا حسب اتجاه الشاشة.

## كيف تضيف أيقونة

```powershell
# ثابتة
curl.exe -s https://api.iconify.design/lucide/flag.svg
# متحركة
curl.exe -s https://api.iconify.design/line-md/flag.svg
```

ألصق الناتج كـ `const` في `icons.rs`، وأضف المتغيّر إلى `enum Icon` أو
`enum AnimatedIcon` وإلى `svg()` وإلى اختبار `every_icon_is_a_self_contained_svg`.
الاختبار يرفض SVG بلا `viewBox` أو بلا `aria-hidden`، ويرفض أيقونة متحركة بلا
`<animate>`.

## المعاينة

```powershell
$env:TIRA_BIND = "127.0.0.1:8090"   # 8080 مشغول بـ Stalwart على جهاز التطوير
.\target\debug\tira-api.exe
# افتح http://127.0.0.1:8090/
```

الصفحة `GET /` تُبنى من `tira_frontend::demo_page()`، وهي نفس الـ HTML الذي
يُختبر في `crates/tira-api/tests/health.rs` — ليست صورة ولا mockup.
