# وثائق واجهة برمجة التطبيقات (API Documentation)

## نقاط النهاية المتاحة (RESTful Endpoints)

| المسار | الطريقة | الوصف | الفلاتر المدعومة |
| :--- | :--- | :--- | :--- |
| `/api/v1/health` | `GET` | فحص صحة الخدمة ومبادئ النزاهة | - |
| `/api/v1/universities` | `GET` | استرجاع الجامعات العمومية | `city`, `status`, `search` |
| `/api/v1/universities/{id}` | `GET` | تفاصيل جامعة محددة بالمعرف أو الرمز | - |
| `/api/v1/universities` | `POST` | إضافة جامعة جديدة مع توثيق المصدر | يتطلب `source_url` |
| `/api/v1/institutions` | `GET` | استرجاع الكليات ومدارس المهندسين | `university_id`, `city`, `type`, `status` |
| `/api/v1/institutions/{id}` | `GET` | تفاصيل مؤسسة محددة | - |
| `/api/v1/institutions` | `POST` | إضافة مؤسسة مع ربطها بالجامعة الأم | `university_id`, `source_url` |
| `/api/v1/reports` | `GET` | عرض بلاغات التدقيق الواردة | `status` |
| `/api/v1/reports` | `POST` | تقديم بلاغ تصحيح بمعلومة مع رابط إثبات | `source_proof_url` إلزامي |
| `/api/v1/reports/{id}/status` | `PATCH` | تحديث حالة البلاغ (معتمد / مرفوض) | `status` |
| `/api/v1/sources` | `GET` | سجلّ المصادر الرسمية التي يرصدها المرصد | - |
| `/api/v1/sources` | `POST` | تسجيل مصدر جديد (يتطلب `X-Jami3ati-Admin`) | `url` بصيغة http/https |
| `/api/v1/sources/{id}/poll` | `POST` | تشغيل استقصاء فوري لمصدر (إداري) | - |
| `/api/v1/facts` | `GET` | أرشيف الوقائع المنشورة (المعتمدة فقط) | `status=pending` للإدارة فقط |
| `/api/v1/facts/{id}` | `GET` | تفاصيل واقعة مع اللقطة الكاملة | المعتمدة عامة، غيرها للإدارة |
| `/api/v1/facts/{id}/approve` | `POST` | اعتماد واقعة → إشعار + طابور إرسال (إداري) | `dispatch` |
| `/api/v1/facts/{id}/reject` | `POST` | رفض واقعة دون نشر (إداري) | `note` |
| `/n/{id}` | `GET` | **صفحة الإثبات العامة** (HTML بلا حساب ولا JavaScript) | - |

## حلقة الرصد (Observatory Loop)

```
مصدر مسجَّل → جلب (robots.txt + ETag) → تطبيع النص → بصمة SHA-256
   → مقارنة باللقطة السابقة
       ├─ أول قراءة  → خط أساس (baseline) بلا واقعة
       ├─ لا تغيير   → لا شيء
       └─ تغيير      → واقعة `pending` (غير منشورة)
                         → مراجعة بشرية
                             ├─ اعتماد → إشعار → طابور `notification_deliveries` → Tira
                             │            → صفحة `/n/{id}`
                             └─ رفض   → تبقى محفوظة وغير منشورة
```

**ضمانان منفصلان لا يُخلطان:**
- `source_url` + `verified_at` + `snapshot_hash` = **دليل المحتوى**: النص طابق الصفحة الرسمية في تاريخ القراءة.
- `DKIM/SPF/DMARC` = **دليل النطاق**: الرسالة خرجت من نطاقنا. لا يعني أن الوزارة وقّعت المعلومة.

**قاعدة الصدق:** أول استقصاء يسجّل خط أساس ولا يُنشئ واقعة. صفحة فارغة أو خطأ شبكة يُسجَّل كخطأ، لا كتغيّر. لا شيء يُنشر قبل اعتماد بشري. المصدر غير الرسمي (`is_official=false`) يُنتج إشعارًا بنوع `source_change`، لا `official_change`.

## توثيق تفاعلي
- Swagger UI متاح على: `http://localhost:8000/api/v1/docs`
- ReDoc متاح على: `http://localhost:8000/api/v1/redoc`
