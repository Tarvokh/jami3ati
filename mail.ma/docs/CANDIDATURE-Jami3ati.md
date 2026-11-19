# الترشّح — جامعتي / Jami3ati

> ملف جاهز للنسخ في استمارة الهاكاثون.
> **آخر أجل للتقديم:** الجمعة 20 نوفمبر 2026 — 16:30
> **الحدث:** 21–22 ديسمبر 2026، ENSIAS، الرباط
> **تاريخ تحرير الملف:** 17 نوفمبر 2026

---

# النسخة العربية

## 1. الهوية

| الحقل | القيمة |
|---|---|
| اسم المشروع (AR) | **جامعتي** |
| اسم المشروع (FR) | **Jami3ati** |
| التسمية الفرعية | مرصد مواطن للبيانات المفتوحة في التعليم العالي |
| الرسالة في سطر | **نحوّل البيانات المفتوحة إلى تنبيهات قابلة للتحقق.** |
| المجال | Open Data · Civic Tech · Education |

## 2. المشكلة

المعلومات الرسمية عن التعليم العالي في المغرب — مواعيد الترشيح، شروط الولوج، الرسوم، نتائج التوجيه — منشورة على مواقع مؤسسية متفرقة، بلا إشعار، وبلا أرشيف.

النتيجة ثلاثية:

1. **الطالب يكتشف التغيير بعد فوات الأجل** — لا يوجد أي push، فقط زيارة يدوية متكررة.
2. **لا يستطيع إثبات ما كان مكتوبًا بالأمس** — الصفحة تُعدَّل في مكانها، بلا نسخة مؤرّخة.
3. **المعلومة موجودة، لكنها غير قابلة للاستعمال في اللحظة التي تهمّ.**

المعلومة المفتوحة لا تنفع إن لم تصل في وقتها، ومع دليل على مصدرها.

## 3. الحل — حلقتان على بنية واحدة

**الحلقة 1 — البلاغ (reactive).** طالب يبلّغ عن معلومة منتهية أو مشكوك فيها، مع `source_proof_url` إلزامي، ومنسّق يتحقق ويغيّر الحالة (`verified` / `needs_review` / `demo`). كل تغيير حالة يُقيَّد في `audit_mirror`.

**الحلقة 2 — التنبيه (proactive).** عامل دوري (`poller`) يزور صفحات المصادر الرسمية، يكشف التغيير، ينشر «واقعة» (fact) جديدة بـ `source_url` + `verified_at`، ثم يرسل بريدًا للمشتركين.

**البنية المشتركة:** `outbox worker` واحد، `mail_relay` واحد، `subscriptions` + double opt-in، `audit_mirror`.

## 4. الضمانتان — منفصلتان ولا تُخلطان

| الضمانة | ما تثبته | كيف |
|---|---|---|
| DKIM / SPF / DMARC | أن البريد صادر من نطاقنا فعلًا | توقيع على مستوى الـ MTA |
| `source_url` + `verified_at` + `notification_id` | أن المعلومة مأخوذة من مصدر رسمي في تاريخ محدّد | سلسلة نسب في قاعدة البيانات + صفحة تحقّق عامة `/n/{id}` |

نحن **لا** نقول «موقّع من الوزارة». نقول: «هذه المعلومة مأخوذة من هذا الرابط في هذا التاريخ، وهذا البريد صادر من نطاقنا». الفرق بين الادّعاء والدليل.

## 5. البيانات المستخدمة

- **data.gov.ma** — مجموعات البيانات المتعلقة بالتعليم العالي.
- **enssup.gov.ma** — البلاغات والتوجيهات الرسمية.
- **المواقع المؤسسية** لـ UM5، ENSIAS، UCA، FST-Guéliz — أربعة مصادر كلّها `verified`. الكتالوج التجريبي (`src/data/seedData.ts`) يعرض **12 كيانًا**: 3 جامعات (2 `verified`، 1 `needs_review`) و 9 مؤسسات (8 `verified`، 1 `demo` موسومة بوضوح). قاعدة العرض `jami3ati_demo` تحتوي **3 كيانات** فقط (جامعتان + مؤسسة واحدة)، كلها `verified`.

**قيد مُعلَن:** لا يوجد API رسمي بعد. لذلك نعتمد استقصاءً دوريًا (periodic polling) محترمًا لـ `robots.txt` وبمعدّل منخفض — لا scraping واسع.

## 6. ما هو مبنيّ اليوم / ما سيُبنى قبل 21 ديسمبر

**مبنيّ ومقاس اليوم:**

- **Tira Mail** (البنية التحتية): Rust workspace، 8 crates، 10 migrations، نمط `outbox`، توقيع وتحقق DKIM، CSP صارم (`default-src 'self'`، `frame-ancestors 'none'`)، RTL-first، ويعمل بلا JavaScript. **276 اختبارًا ناجحًا، 0 فاشل** (مقاس في 17 نوفمبر 2026).
- **Jami3ati** (المنتج): React 19 + Vite، FastAPI، PostgreSQL 16 مع RLS. `POST /reports` يرفض أي بلاغ بلا `source_proof_url` يبدأ بـ `http`. `PATCH /reports/{id}/status` مع أثر تدقيقي وحاجز إدارة (401 قبل 404).
- واجهة قراءة بثلاثة أعمدة (sidebar | list | reader) تعمل بلا JavaScript، مع تبويب `data-view` كبديل عمود واحد على الشاشات الضيقة.
- **عقد التكامل مُنفَّذ ومُختبَر:** `POST /api/v1/outbound` في Tira يقبل الإرسال بـ `202 Accepted` (قبول ≠ تسليم)، مع `idempotency_key` إلزامي فريد لكل مستأجر، ومجموعة ترويسات مغلقة (`X-Jami3ati-Notification`، `List-Unsubscribe`، `Reply-To`) — 8 اختبارات تكامل على PostgreSQL حقيقي.
- **الطرف الثاني من الحلقة 1 مبنيّ ومقاس (17 نوفمبر 2026):** `subscriptions` مع double opt-in، `notifications`، و`notification_deliveries` (طابور `outbox`)، وعامل إرسال يعيد المحاولة بتباعد متزايد (`30s → 900s`) ويسجّل `dead` عند استنفاد المحاولات. **57 اختبارًا ناجحًا، 0 فاشل**، وبرهان حيّ عبر لغتين: Tira قبلت الحمولة بـ`202` وأعادت **نفس المعرّف** عند إعادة الإرسال بنفس `idempotency_key`، ورفضت بـ`400` مفتاحًا غير صالح و`List-Unsubscribe` غير `https`. **قبول ≠ تسليم:** لا ندّعي وصول بريد لم يُقَس.

**مُخطَّط ومُلتزَم به قبل العرض:**

| الفترة | المخرَج |
|---|---|
| 24–30 نوفمبر | `subscriptions` + double opt-in، `outbox worker` مشترك، `mail_relay` وقالب بريد أساسي |
| 1–7 ديسمبر | `fact_history` + endpoint نشر تغيير، قالب بريد التنبيه، صفحة `/n/{id}` |
| 8–20 ديسمبر | إصلاحات Tira، بروفة كاملة، قياس |

> **تحديث 17 نوفمبر 2026:** صف **24–30 نوفمبر** أُنجز قبل موعده (`subscriptions` + double opt-in + `outbox worker`، 57 اختبارًا). ما بقي منه: قالب البريد النهائي. الصفوف الأخرى كما هي.

## 7. الأثر — أهداف قابلة للقياس، لا أرقام مُدّعاة

- تقليص زمن اكتشاف تغيير رسمي من «أسابيع أو لا شيء» إلى **أقل من 24 ساعة** (هدف يُقاس على الـ poller).
- **أرشيف عام قابل للاستشهاد**: كل معلومة لها نسخة مؤرّخة ورابط دائم.
- **قابلية التحقق للجميع**: أي شخص يستطيع فتح `/n/{id}` والتأكد من المصدر والتاريخ دون حساب.
- **قابلية التوسّع الأفقي**: إضافة مؤسسة = إدخال مصدر جديد في جدول، لا كود جديد.

## 8. الخصوصية والالتزام القانوني

بريد إلكتروني فقط في النسخة التجريبية (لا SMS، لا هاتف). موافقة صريحة مزدوجة (double opt-in)، مدة حفظ محدّدة، رابط إلغاء اشتراك في كل بريد، وحد أدنى من البيانات الشخصية — وفق **loi 09-08 / CNDP**.

## 9. الفريق

| الدور | الاسم | المسؤولية |
|---|---|---|
| قيادة تقنية / معمارية | Sami Benali | Rust، Tira Mail، التكامل، قاعدة البيانات |
| واجهة أمامية | Ismail Benali | React 19، إتاحة الوصول، RTL |
| تحقّق من المصادر | Elmoufid Hamza | مراجعة المؤسسات، تصنيف `verified` / `needs_review` |

> الأسماء أعلاه هي الفريق الفعلي: ثلاثة أعضاء، لكل دور شاغله. لا دور مضاف بلا شاغل.
>
> **لا يحمل هذا الجدول أي تاريخ ميلاد، ولا يُضاف إليه.** الحد الأدنى من المعطيات: الاسم والدور فقط.

## 10. ما لا ندّعيه

- لا ندّعي توقيعًا رقميًا من أي وزارة أو مؤسسة.
- لا ندّعي تغطية وطنية: الكتالوج التجريبي يغطّي **12 كيانًا** (3 جامعات + 9 مؤسسات)، منها **10 `verified`**، وقاعدة العرض `jami3ati_demo` تحتوي **3 كيانات** `verified`. هذا نموذج مقاس، لا تغطية وطنية.
- لا ندّعي أن DKIM سيظهر أخضر على المسرح؛ صفحة `/n/{id}` و`source_url` هما الدليل الأساسي.
- لا نفتح تسجيلًا عامًا قبل الحدث — العرض يستعمل صندوقًا مُهيّأً مسبقًا، لأن فتح signup بلا سمعة نطاق مخاطرة بلا مقابل.

---

# Version française

## 1. Identité

| Champ | Valeur |
|---|---|
| Nom du projet | **Jami3ati** |
| Nom arabe | **جامعتي** |
| Sous-titre | Observatoire citoyen des données ouvertes de l'enseignement supérieur |
| Message en une phrase | **Transformer les données ouvertes en alertes vérifiables.** |
| Domaine | Open Data · Civic Tech · Éducation |

## 2. Problème

Les informations officielles sur l'enseignement supérieur au Maroc — dates de candidature, conditions d'accès, frais, résultats d'orientation — sont publiées sur des sites institutionnels dispersés, sans notification et sans archive.

Conséquences :

1. **L'étudiant découvre le changement après la date limite** : aucune notification push, seulement des visites manuelles répétées.
2. **Il ne peut pas prouver ce qui était écrit hier** : la page est modifiée sur place, sans version datée.
3. **L'information existe, mais elle est inutilisable au moment où elle compte.**

Une donnée ouverte ne sert à rien si elle n'arrive pas à temps, avec une preuve de sa source.

## 3. Solution — deux boucles, une seule infrastructure

**Boucle 1 — Signalement (réactive).** Un étudiant signale une information obsolète ou douteuse, avec `source_proof_url` obligatoire ; un modérateur vérifie et change le statut (`verified` / `needs_review` / `demo`). Chaque changement est journalisé dans `audit_mirror`.

**Boucle 2 — Alerte (proactive).** Un `poller` périodique visite les pages officielles, détecte le changement, publie un « fait » avec `source_url` + `verified_at`, puis envoie un e-mail aux abonnés.

**Infrastructure partagée :** un seul `outbox worker`, un seul `mail_relay`, `subscriptions` + double opt-in, `audit_mirror`.

## 4. Deux garanties distinctes, jamais confondues

| Garantie | Ce qu'elle prouve | Comment |
|---|---|---|
| DKIM / SPF / DMARC | que l'e-mail provient bien de notre domaine | signature au niveau du MTA |
| `source_url` + `verified_at` + `notification_id` | que le fait provient d'une source officielle, à une date donnée | chaîne de provenance en base + page publique `/n/{id}` |

Nous ne disons **pas** « signé par le ministère ». Nous disons : « ce fait provient de ce lien, à cette date ; cet e-mail provient de notre domaine ».

## 5. Données utilisées

- **data.gov.ma** — jeux de données liés à l'enseignement supérieur.
- **enssup.gov.ma** — communiqués et orientations officielles.
- **Sites institutionnels** : UM5, ENSIAS, UCA, FST-Guéliz — quatre sources toutes `verified`. Le catalogue de démonstration (`src/data/seedData.ts`) expose **12 entités** : 3 universités (2 `verified`, 1 `needs_review`) et 9 établissements (8 `verified`, 1 `demo` clairement étiqueté). La base de démonstration `jami3ati_demo` ne contient que **3 entités** (2 universités + 1 établissement), toutes `verified`.

**Limite assumée :** pas d'API officielle à ce jour. Nous utilisons un polling périodique respectueux de `robots.txt` et à faible fréquence — pas de scraping massif.

## 6. État réel / Plan d'ici le 21 décembre

**Déjà construit et mesuré :**

- **Tira Mail** (infrastructure) : workspace Rust, 8 crates, 10 migrations, pattern `outbox`, signature et vérification DKIM, CSP strict (`default-src 'self'`, `frame-ancestors 'none'`), RTL-first, fonctionne sans JavaScript. **276 tests passent, 0 échec** (mesuré le 17 novembre 2026).
- **Jami3ati** (produit) : React 19 + Vite, FastAPI, PostgreSQL 16 avec RLS. `POST /reports` rejette tout signalement sans `source_proof_url` commençant par `http`. `PATCH /reports/{id}/status` avec piste d'audit et garde d'administration (401 avant 404).
- Interface de lecture à trois colonnes (sidebar | list | reader) sans JavaScript, avec repli mono-colonne via `data-view`.
- **Contrat d'intégration implémenté et testé :** `POST /api/v1/outbound` dans Tira accepte l'envoi en `202 Accepted` (acceptation ≠ livraison), avec `idempotency_key` obligatoire et unique par tenant, et un ensemble fermé d'en-têtes (`X-Jami3ati-Notification`, `List-Unsubscribe`, `Reply-To`) — 8 tests d'intégration sur PostgreSQL réel.
- **Second côté de la boucle 1 construit et mesuré (17 novembre 2026) :** `subscriptions` avec double opt-in, `notifications`, et `notification_deliveries` (file `outbox`), plus un worker qui réessaie avec un backoff croissant (`30s → 900s`) et marque `dead` après épuisement. **57 tests passent, 0 échec**, et preuve vivante inter-langages : Tira a accepté la charge utile en `202` et renvoyé **le même identifiant** lors d'un renvoi avec la même `idempotency_key`, et a refusé en `400` une clé invalide et un `List-Unsubscribe` non `https`. **Acceptation ≠ livraison :** nous ne prétendons aucune arrivée non mesurée.

**Planifié et engagé :**

| Période | Livrable |
|---|---|
| 24–30 nov. | `subscriptions` + double opt-in, `outbox worker` partagé, `mail_relay` + template de base |
| 1–7 déc. | `fact_history` + endpoint de publication, template d'alerte, page `/n/{id}` |
| 8–20 déc. | correctifs Tira, répétition générale, mesures |

> **Mise à jour du 17 novembre 2026 :** la ligne **24–30 nov.** est livrée en avance (`subscriptions` + double opt-in + `outbox worker`, 57 tests). Reste de cette ligne : le template d'e-mail final. Les autres lignes sont inchangées.

## 7. Impact — des objectifs mesurables, pas des chiffres inventés

- Réduire le délai de détection d'un changement officiel de « des semaines, ou jamais » à **moins de 24 heures** (objectif mesuré sur le poller).
- **Archive publique citable** : chaque fait possède une version datée et un lien permanent.
- **Vérifiabilité pour tous** : n'importe qui peut ouvrir `/n/{id}` et vérifier source et date, sans compte.
- **Extensibilité horizontale** : ajouter un établissement = ajouter une source en base, sans nouveau code.

## 8. Confidentialité et conformité

E-mail uniquement dans la version pilote (pas de SMS, pas de téléphone). Consentement explicite double (double opt-in), durée de conservation définie, lien de désinscription dans chaque message, minimisation des données — conformément à la **loi 09-08 / CNDP**.

## 9. Équipe

| Rôle | Nom | Responsabilité |
|---|---|---|
| Lead technique / architecture | Sami Benali | Rust, Tira Mail, intégration, base de données |
| Front-end | Ismail Benali | React 19, accessibilité, RTL |
| Vérification des sources | Elmoufid Hamza | Revue des établissements, statuts `verified` / `needs_review` |

> Les noms ci-dessus sont l'équipe réelle : trois membres, un titulaire par rôle. Aucun rôle ajouté sans titulaire.
>
> **Ce tableau ne contient aucune date de naissance, et il ne doit pas en contenir.** Minimisation : nom et rôle uniquement.

## 10. Ce que nous ne prétendons pas

- Aucune signature numérique d'un ministère ou d'un établissement.
- Aucune couverture nationale : le catalogue de démonstration couvre **12 entités** (3 universités + 9 établissements), dont **10 `verified`** ; la base `jami3ati_demo` en contient **3**. C'est un échantillon mesuré, pas une couverture.
- Aucune promesse de DKIM « vert » sur scène ; `/n/{id}` et `source_url` sont la preuve principale.
- Pas d'inscription publique avant l'événement : la démo utilise une boîte pré-provisionnée.

---

## قائمة التحقق قبل الإرسال

- [x] استبدال الأقواس بأسماء الفريق في الجدولين (AR وFR): Sami Benali ، Ismail Benali ، Elmoufid Hamza.
- [ ] التأكد من صيغة حقول الاستمارة (عدد الكلمات / الحقول الإلزامية).
- [ ] إرفاق رابط المستودع أو لقطة شاشة للواجهة (إن طُلب).
- [ ] التحقق من أن كل رقم مذكور أعلاه ما زال مقاسًا يوم الإرسال (276 اختبارًا Tira، 57 اختبارًا Jami3ati، 12 كيانًا في الكتالوج منها 10 `verified`).
- [ ] الإرسال قبل **الجمعة 20 نوفمبر 2026، 16:30** بتوقيت المغرب.
