# demo-fixture — السيناريو الاحتياطي القابل للاستنساخ

تُستخدم هذه الصفحة فقط إذا لم يتغيّر المصدر الرسمي أثناء العرض. هي ليست
مصدرًا رسميًا، ويجب قول ذلك صراحةً على المنصة.

| الملف | الدور |
|---|---|
| `index.html` | الحالة الحالية: «أجل الترشيح: 15 ديسمبر 2026» |
| `bump_date.py` | يقدّم التاريخ خمسة أيام ويترك بقية الصفحة كما هي |

## الاستخدام

من جذر المستودع:

```powershell
$work = Join-Path $env:TEMP "jami3ati-demo"
New-Item -ItemType Directory -Force $work | Out-Null
Copy-Item ".\-Jami3ati-main\-Jami3ati-main\demo-fixture\index.html" $work -Force

python -m http.server 8099 --directory $work   # نافذة منفصلة
python ".\-Jami3ati-main\-Jami3ati-main\demo-fixture\bump_date.py" $work
```

ثم استقصِ المصدر عبر الـAPI. كل نداء لـ`bump_date.py` ينتج فرقًا من سطر
واحد: `15 → 20 → 25 → 30 ديسمبر`.

## لماذا سكربت بدل كتابة HTML؟

- **`<meta charset="utf-8">`** موجود في الصفحة. بدونه يعرض المتصفح بايتات
  UTF-8 كـWindows-1252 فيظهر `Ø¥Ø¹Ù„Ø§Ù†` بدل «إعلان».
- **العنوان لا يتغيّر**، فيبقى الفرق سطرًا واحدًا يسهل قراءته على الجمهور.
- لا يُلصق نص عربي في PowerShell 5.1، وهو ما يُفسد الترميز عادةً.
