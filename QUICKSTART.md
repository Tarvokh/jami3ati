# QUICKSTART — تشغيل جامعتي من نسخة نظيفة

> هذا الدليل مقيس فعليًا على نسخة نظيفة من `Tarvokh/jami3ati` بتاريخ
> **19 نوفمبر 2026**. الأوامر مكتوبة لـWindows PowerShell؛ على Linux/macOS
> استبدل `.\.venv\Scripts\` بـ`.venv/bin/`.

## المتطلبات

| الأداة | النسخة المقيسة |
|---|---|
| Python | 3.12 |
| Node.js | 22 |
| PostgreSQL | 16 |
| Rust (اختياري، لـTira Mail) | stable |

## 1) قاعدة البيانات

أنشئ قاعدتين. اسم قاعدة الاختبار **يجب** أن يحتوي `test` (حارس في
`backend/tests/conftest.py` يرفض غير ذلك):

```powershell
createdb -h 127.0.0.1 -p 55432 -U postgres jami3ati_demo
createdb -h 127.0.0.1 -p 55432 -U postgres jami3ati_test
```

> إن كان Postgres لديك على منفذ أو مستخدم آخر، غيّر `DATABASE_URL` في الخطوات
> التالية. الجداول تُنشأ تلقائيًا عند أول تشغيل (`Base.metadata.create_all`).

## 2) الخادم (FastAPI)

```powershell
cd ".\-Jami3ati-main\-Jami3ati-main\backend"
python -m venv .venv
.\.venv\Scripts\Activate.ps1
pip install -r requirements.txt

$env:DATABASE_URL="postgresql://postgres@127.0.0.1:55432/jami3ati_demo"
$env:JAMI3ATI_HMAC_SECRET="demo-secret"
$env:JAMI3ATI_ADMIN_TOKEN="demo-admin"
$env:JAMI3ATI_PUBLIC_BASE_URL="http://127.0.0.1:8000"

uvicorn app.main:app --host 127.0.0.1 --port 8000
```

في نافذة ثانية (نفس البيئة):

```powershell
cd ".\-Jami3ati-main\-Jami3ati-main\backend"
.\.venv\Scripts\Activate.ps1
$env:DATABASE_URL="postgresql://postgres@127.0.0.1:55432/jami3ati_demo"

# يسجّل مصدرًا رسميًا واحدًا ويحفظ أول لقطة (خط أساس)
python -m app.seed.seed_sources --poll
```

## 3) الواجهة (React + Vite)

```powershell
cd ".\-Jami3ati-main\-Jami3ati-main"
npm ci
npm run dev        # http://127.0.0.1:3000
```

## 4) الاختبارات

```powershell
cd ".\-Jami3ati-main\-Jami3ati-main\backend"
$env:DATABASE_URL="postgresql://postgres@127.0.0.1:55432/jami3ati_test"
python -m pytest
# المقيس: 72 passed
```

الواجهة:

```powershell
cd ".\-Jami3ati-main\-Jami3ati-main"
npm run lint       # tsc --noEmit
npm run build
```

## 5) Tira Mail (المحرّك، اختياري)

```powershell
cd mail.ma
$env:TIRA_REQUIRE_DATABASE="1"
.\scripts\dev.ps1 test-db     # المقيس: 302 passed، 33 target
cargo run -p tira-api         # http://127.0.0.1:8080
```

> إن منعت سياسة التنفيذ ملفات `.ps1`:
> `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\dev.ps1 test-db`.

> **Windows + بناء Rust من الصفر:** يحتاج C toolchain على `PATH` (MSVC Build
> Tools أو MinGW، مثل `C:\tools\winlibs\bin`). بدونه يفشل `cc-rs` برسالة
> `failed to find tool "gcc.exe"`. الـworkspace المبني مسبقًا لا يحتاجه.

## ما لا يحدث تلقائيًا

- **لا بريد يُرسل** بدون `TIRA_OUTBOUND_TOKEN` و`TIRA_MAILBOX_ID`؛ صفوف
  `notification_deliveries` تبقى `pending`.
- **لا نشر آلي**: كل واقعة تحتاج اعتمادًا بشريًا قبل `/n/{id}` أو الإرسال.
- **لا تسجيل عام**: العرض يستخدم صندوقًا مُهيّأً مسبقًا.

## English summary

Measured from a clean clone of `Tarvokh/jami3ati` on 19 November 2026:
`pip install -r requirements.txt` then `pytest` → **72 passed**;
`npm ci` → 256 packages, `npm run lint` and `npm run build` → exit 0;
Tira `dev.ps1 test-db` → **302 passed**, 33 targets. Tables are created on
import. No email is sent without Tira credentials; deliveries stay `pending`
until a human approves a fact. A fresh Windows Rust build needs a C toolchain
on `PATH` (MSVC Build Tools or MinGW); the 302 measurement used a prebuilt
workspace.
