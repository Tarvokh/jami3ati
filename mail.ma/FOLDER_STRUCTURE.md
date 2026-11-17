# هيكل مجلدات مشروع Tira Mail — MAP TREE FILE

**المشروع:** Tira Mail (سابقاً Barid) — البريد السيادي المغربي
**الإصدار:** نوفمبر 2026

> ⚠️ **تنبيه:** القسم الأول أدناه هو **هيكل مقترح** (Proposed)، وليس ما هو موجود
> فعلاً على القرص. ملفات مثل `storage.rs` و`routes/` و`ws/` و`state/` و`infra/`
> **غير موجودة**. راجع «الهيكل الفعلي اليوم» في نهاية الملف.

## الهيكل الفعلي اليوم (مُتحقَّق منه بالبناء)

```
mail.ma/
├── .github/workflows/ci.yml
├── crates/
│   ├── tira-api/{src/lib.rs, src/main.rs, src/read.rs, tests/health.rs, tests/read.rs}
│   ├── tira-auth/{src/lib.rs, src/password.rs, src/otp.rs}
│   ├── tira-frontend/src/lib.rs
│   ├── tira-module/src/lib.rs
│   ├── tira-smtp/{src/lib.rs, src/server.rs, src/client.rs, src/message.rs, tests/session.rs}
│   ├── tira-store/
│   │   ├── src/{lib.rs, blob.rs, db.rs, model.rs, store.rs}
│   │   ├── migrations/0001_init.sql
│   │   └── tests/{blob.rs, persist.rs}
│   └── tira-vault/src/lib.rs
├── data/                          (وقت التشغيل: blobs + سجلات — غير مُتتبَّع)
├── docs/{CORRECTIONS.md, DEEP_MAP.ar.md, deep-map.html}
├── scripts/
│   ├── dev.ps1                    (check | test | test-db | clippy | fmt | run | clean)
│   ├── pg-dev.ps1                 (PostgreSQL محمول على 55432)
│   ├── imap-proof.ps1             (STARTTLS + LOGIN + APPEND + FETCH + IMAPS)
│   ├── smtp-proof.ps1             (وارد بلا مصادقة + AUTH PLAIN + تحقق IMAP)
│   └── jmap-proof.ps1             (session + Mailbox/get + Email/query + Email/get)
├── Cargo.toml / Cargo.lock
├── README.md
├── BARID_STRATEGIC_REPORT.ar.md   (يحتوي أخطاء — راجع docs/CORRECTIONS.md)
├── BARID_DEEP_PLAN.ar.md          (يحتوي أخطاء — راجع docs/CORRECTIONS.md)
└── FOLDER_STRUCTURE.md            (هذا الملف)
```

كل الـ crates السبعة موجودة وتُبنى وتُختبر فعلياً (45 اختباراً، 7 منها على
PostgreSQL 16 حقيقي).

**خارج المستودع (بيئة الإثبات فقط):** `C:\tools\stalwart\` يحتوي
`stalwart.exe` (1.0.2) و`stalwart-cli.exe` (1.0.14)، ومعه
`run\config.json` (DataStore فقط)، و`run\plan*.ndjson` (خطط إنشاء النطاق
والمستخدم والمستمعين)، و`run\tls\{cert.pem,key.pem}`. لا شيء من هذا داخل
`mail.ma/`، ولا يُنسخ إلى الإنتاج كما هو.

---

## الهيكل المقترح (خطة مستقبلية، غير منفَّذة)

## الهيكل الكامل المقترح لمستودع Tira Mail

```
tira-mail/
├── .github/
│   ├── workflows/
│   │   ├── ci.yml
│   │   └── deploy.yml
│   └── pull_request_template.md
├── crates/                          # مكتبات العمل (Rust workspace)
│   ├── tira-smtp/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── server.rs
│   │   │   └── client.rs
│   │   └── tests/
│   ├── tira-vault/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   └── storage.rs
│   │   └── tests/
│   ├── tira-auth/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   └── otp.rs
│   │   └── tests/
│   ├── tira-api/
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── lib.rs
│   │   │   ├── routes/
│   │   │   ├── ws/
│   │   │   └── state/
│   │   └── tests/
│   ├── tira-frontend/
│   │   ├── src/
│   │   │   ├── components/
│   │   │   ├── pages/
│   │   │   ├── routes.rs
│   │   │   └── ...
│   │   └── tests/
│   └── tira-module/
│       ├── src/
│       │   ├── lib.rs
│       │   └── module.rs
│       └── tests/
├── frontend/                        # (إن وجد) أو tira-frontend
├── infra/
│   ├── docker/
│   │   ├── Dockerfile
│   │   └── docker-compose.yml
│   ├── k8s/
│   └── nginx/
├── docs/
├── scripts/
├── Cargo.toml                       # workspace root
└── docker-compose.yml
```

## ملاحظات الهيكل

- كل crate في `crates/` مستقل (tira-smtp, tira-vault, tira-auth, tira-api, tira-frontend, tira-module).
- `infra/` **مقترح وغير موجود** (docker, k8s, nginx).
- `docs/` للتوثيق.
- `scripts/` لأوامر الصيانة (`dev.ps1`).

## التقسيم المقترح للطبقات

1. SMTP: `tira-smtp`
2. Vault: `tira-vault`
3. Auth: `tira-auth`
4. API: `tira-api`
5. Frontend: `tira-frontend`
6. Module: `tira-module`
