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

## توثيق تفاعلي
- Swagger UI متاح على: `http://localhost:8000/api/v1/docs`
- ReDoc متاح على: `http://localhost:8000/api/v1/redoc`
