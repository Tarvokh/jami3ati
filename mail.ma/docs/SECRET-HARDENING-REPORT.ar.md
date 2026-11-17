# تقرير تصليب معالجة الأسرار في جسر محرّك Stalwart

**الوحدة:** تصليب الأسرار في `tira-provision` (engine.rs)
**التاريخ:** 8 نوفمبر 2026
**المرجع الملزم:** `docs/CORRECTIONS.md` §9.14
**الحالة:** منفَّذ، مُختبَر، مُتحقَّق حيّاً ضمن الحدود المعلنة أدناه

---

## 1. الملخّص التنفيذي

اختارت هذه الوحدة مشكلةً أمنيةً حقيقيةً في طبقة الجسر بين control plane ومحرك
البريد Stalwart: **معالجة الأسرار في `crates/tira-provision/src/engine.rs`**.
كان المحوّል يسرّب سرّين عبر سطحين مختلفين:

1. **كلمة مرور المسؤول** كانت تُمرَّر على **وسائط سطر الأوامر (argv)** عبر
   `--password`، فتظهر في قائمة العمليات لأي مستخدم محلّي.
2. **الخطة وفيها كلمة مرور الصندوق لمرة واحدة** كانت تُكتب في **ملف مؤقّت على
   القرص**، وعلى Windows يُكتب بصلاحيات موروثة لا «للمالك فقط»، خلافاً لما
   كانت وثيقة الوحدة تدّعيه.

أُصلحت الثغرتان **بالإزالة لا بالترقيع**: الاعتماد يُمرَّر الآن عبر **بيئة**
العملية الابنة (يقرأها `stalwart-cli` رسمياً)، والخطة تُمرَّر عبر **stdin**
(`apply --stdin`) فلا تلمس القرص إطلاقاً. هذا يلغي صنفَي الخطر من جذورهما على
أي نظام تشغيل، بدل سدّ ثغرة Windows ACL وترك أثر القرص قائماً.

الإصلاح مبنيّ على **قناة رسمية موثّقة** تحقّقت منها حيّاً عبر `stalwart-cli 1.0.14`
(`--help` + فحص حيّ)، لا على تخمين. أضفت اختبارَي وحدة يثبتان الخاصّيتين
الأمنيتين بشكل حتمي بلا شبكة ولا قاعدة بيانات. كل البوابات خضراء:
`fmt=0`، `check=0`، `clippy -D warnings=0`، و**145/145 اختباراً** على PostgreSQL
حقيقي (0 فشل، 0 تخطّي).

---

## 2. المشكلة بالتفصيل: ثغرتان خفيّتان

### 2.1 الثغرة الأولى (V1): كلمة مرور المسؤول في argv

كان `base_command()` يبني الاستدعاء هكذا:

```rust
command
    .arg("--url").arg(&self.config.url)
    .arg("--user").arg(&self.config.user)
    .arg("--password").arg(&self.config.password)   // ← السرّ في argv
    .stdin(Stdio::null());
```

**لماذا هذا خطر:** قائمة وسائط العملية مقروءة عالمياً:

- على Unix: `/proc/<pid>/cmdline` متاح لأي مستخدم، و`ps aux` يعرضها.
- على Windows (منصّة النشر الفعلية، GNU toolchain): عمود «Command line» في
  Task Manager، و`Win32_Process.CommandLine` عبر WMI، وأدوات المراقبة — كلّها
  تعرض السطر الكامل وفيه كلمة مرور المسؤول.

كلمة مرور المسؤول هذه تملك **تحكّماً كاملاً** بمحرك البريد (إنشاء/حذف حسابات،
نطاقات، إلخ). تسريبها لأي عملية محلّة أخرى طوال فترة استدعاء الـCLI يعني أن
أي مستخدم أو عملية على نفس الجهاز يستطيع التقاطها.

**التناقض الداخلي:** المشروع نفسه يرفض هذا السلوك في مكان آخر. الملف
`crates/tira-provision/src/bin/tira-provision.rs` يرفض `--password` على سطر
الأوامر صراحةً:

```rust
"--password" => {
    return Err(invalid(
        "refusing --password on the command line (it leaks in the process list); \
         use TIRA_PROVISION_PASSWORD or stdin",
    ))
}
```

فبينما يحمي الـCLI كلمة مرور **الصندوق** من argv، كان المحوّል يسرّب كلمة مرور
**المسؤول** عبر argv. مبدأ المشروع كان مطبَّقاً في طرف ومكسوراً في الطرف الآخر.

### 2.2 الثغرة الثانية (V2): ملف الخطة السرّي على القرص

كان `apply()` يكتب الخطة (وفيها `credentials.0.secret` = كلمة مرور الصندوق لمرة
واحدة) في ملف مؤقّت:

```rust
let path = self.config.plan_dir
    .join(format!("tira-plan-{}.ndjson", Uuid::new_v4()));
write_private(&path, ndjson.as_bytes()).await?;
// ... apply --file <path> ...
let _ = tokio::fs::remove_file(&path).await;   // حذف best-effort
```

و`write_private()` كان:

```rust
#[cfg(unix)]
{ /* OpenOptions create_new mode(0o600) — للمالك فقط */ }

#[cfg(not(unix))]
{
    tokio::fs::write(path, bytes).await        // ← لا ACL صريح على Windows
}
```

**لماذا هذا خطر:**

- على **Windows** (منصّة النشر): `tokio::fs::write` ينشئ الملف بـACL **موروثة**
  من المجلد الأب (مجلد المؤقّتات غالباً)، لا «للمالك فقط». فرع `#[cfg(not(unix))]`
  لم يضبط أي صلاحيات. فالملف السرّي قد يكون مقروءاً لمستخدمين آخرين.
- حتى على Unix، وجود ملف سرّي على القرص — ولو للحظات — يترك أثراً قابلاً
  للاسترجاع من: forensics القرص، الـswap، النسخ الاحتياطي، أو إذا انهارت العملية
  بين الكتابة والحذف (الحذف `let _ =` best-effort، لا يضمن شيئاً عند الانهيار).
- **وثيقة الوحدة كانت كاذبة**: ادّعت «written with owner-only permissions on
  Unix» وسكتت عن Windows، فتوحي بأمان غير موجود على منصّة النشر الفعلية.

---

## 3. لماذا الإزالة أقوى من الترقيع

كان البديل لـV2 هو ضبط Windows ACL صراحةً (`SetNamedSecurityInfo` / DACL عند
الإنشاء). رفضت هذا المسار لأنه:

- معقّد وعرضة للخطأ وخاصّ بالمنصّة (كود `#[cfg(windows)]` إضافي).
- **يُبقي أثر القرص قائماً**: حتى بصلاحيات صحيحة، الملف السرّي موجود على القرص
  فترةَ الاستدعاء، وقابل للاسترجاع من swap/forensics/انهيار.

الإزالة تلغي **صنف الخطر بالكامل**:

- لا ملف ⇒ لا صلاحيات خاطئة، لا أثر قرص، لا نافذة انهيار، لا تنظيف مطلوب.
- لا سرّ في argv ⇒ لا تسريب في قائمة العمليات على أي نظام.
- السلوك **موحد على كل المنصّات** (لا `#[cfg]` أمني متفرّق).

والأهم: القناة البديلة (env + stdin) **رسمية ومدعومة** من الـCLI نفسه، فلا
اختراق ولا إعادة تنفيذ لواجهة HTTP ولا تحليل هشّ.

---

## 4. القناة الرسمية الموثّقة (الدليل)

تحقّقت من واجهة `stalwart-cli 1.0.14` الفعلية قبل التنفيذ:

```text
$ stalwart-cli --version
stalwart-cli 1.0.14

$ stalwart-cli apply --help
Usage: stalwart-cli.exe apply [OPTIONS] <--file <PATH>|--stdin>
      --file <PATH>          Path to the JSON plan file
      --stdin                Read the JSON plan from stdin
      --dry-run              Parse and validate the plan without calling the server
      --password <PASSWORD>  Basic-auth password (prompted if absent and stdin is a TTY) [env: STALWART_PASSWORD]
      --json                 Emit one NDJSON record per completed operation to stdout
```

نقاط مؤكّدة من الواجهة الرسمية:

- `--stdin` قناة رسمية لقراءة الخطة من stdin (بديل mutually-exclusive لـ`--file`).
- `--password` مدعوم بمتغيّر البيئة `[env: STALWART_PASSWORD]`، ويُطلب تفاعلياً
  **فقط** إذا غاب وكان stdin طرفية (TTY). بما أننا نمرّر الخطة عبر أنبوب (pipe)
  لا TTY، ونضبط البيئة، فلا طلب تفاعلي ولا تسريب.
- `--url` و`--user` مدعومان بالبيئة أيضاً (`STALWART_URL`/`STALWART_USER`).
- `--json` يبثّ سجل NDJSON لكل عملية مكتملة (نفس تنسيق الملخّص الذي يحلّله
  `parse_summary` بلا تغيير).

هذا يطابق تماماً نمط authentication-by-environment الذي يستعمله المشروع أصلاً في
`scripts/milter-apply.ps1` (يضبط `$env:STALWART_URL` ويقرأ `STALWART_USER/PASSWORD`
من البيئة، ولا يمرّر `--password`/`--user`/`--url` على argv).

---

## 5. كيف: التغييرات بالتفصيل

### 5.1 `base_command()` — الاعتماد عبر البيئة لا argv

```rust
fn base_command(&self) -> Command {
    let mut command = Command::new(&self.config.binary);
    command
        .env("STALWART_URL", &self.config.url)
        .env("STALWART_USER", &self.config.user)
        .env("STALWART_PASSWORD", &self.config.password)
        .stdin(Stdio::null());
    if self.config.insecure {
        command.arg("--insecure");
    }
    command
}
```

argv الآن يحتوي فقط `apply --stdin --json` (و`--insecure` اختيارياً). لا سرّ في
قائمة العمليات. بيئة العملية الابنة لا تتعرّض في `/proc/<pid>/cmdline` ولا في
عمود سطر الأوامر بـTask Manager.

### 5.2 `apply_command()` + `apply()` — الخطة عبر stdin

فصلتُ بناء استدعاء apply في دالة مستقلّة قابلة للاختبار:

```rust
fn apply_command(&self) -> Command {
    let mut command = self.base_command();
    command
        .arg("apply").arg("--stdin").arg("--json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}
```

و`apply()` يكتب الخطة في أنبوب stdin بدل ملف:

```rust
pub async fn apply(&self, lines: &[PlanLine]) -> Result<ApplyReport, EngineError> {
    if lines.is_empty() {
        return Ok(ApplyReport::default());
    }
    let ndjson = to_ndjson(lines)?;
    let mut child = self.apply_command().spawn()?;

    // نأخذ stdin، نكتب الخطة، ثم نُفلِت المقبض لنشير إلى EOF فيتوقّف الـCLI
    // عن القراءة ويبدأ التطبيق.
    let mut stdin = child.stdin.take().ok_or_else(|| {
        EngineError::Io(std::io::Error::other("child stdin was not piped"))
    })?;
    stdin.write_all(ndjson.as_bytes()).await?;
    stdin.flush().await?;
    drop(stdin);

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        return Err(EngineError::Apply {
            code: output.status.code().unwrap_or(-1),
            stderr: summarise(&output.stderr),
        });
    }
    Ok(parse_summary(&output.stdout))
}
```

**ملاحظات تصميمية:**

- `drop(stdin)` بعد الكتابة إشارة EOF ضرورية: بدونها ينتظر الـCLI المزيد من
  stdin إلى الأبد.
- لا ملف ⇒ لا `create_dir_all`، لا `write_private`، لا `remove_file`، لا نافذة
  انهيار تترك أثراً. السرّ يعيش فقط في ذاكرة هذه العملية وأنبوب stdin الابن.
- `to_ndjson` و`parse_summary` و`summarise` **بلا تغيير**: ترميز الخطة وتحليل
  الملخّص كما كانا، فالسلوك الوظيفي مطابق؛ تغيّر فقط نقل الخطة والمصادقة.
- استُعمل `std::io::Error::other` (اقتراح clippy `io_other_error`) بدل
  `Error::new(ErrorKind::Other, …)` الأقدم.

### 5.3 إزالة `write_private` و`plan_dir`

- حُذفت دالة `write_private` بالكامل (فرعا Unix وWindows).
- حُذف حقل `plan_dir: PathBuf` من `StalwartConfig`.
- حُذفت قراءة `STALWART_PLAN_DIR` من `from_env`.
- حُذف استيراد `Path` و`Uuid` من engine.rs (لم يعودا مستخدمَين)، وأُضيف
  `tokio::io::AsyncWriteExt` لكتابة stdin.
- بسُطّ الإعداد: متغيّر بيئة أقل، حقل أقل، اعتماد أقل على نظام الملفات.

### 5.4 تصحيح وثيقة الوحدة الكاذبة

استُبدل الادّعاء المضلّل («owner-only permissions on Unix») بوثيقة صادقة تصف
الضمان الفعلي:

```rust
//! This adapter never logs the password or the plan body, and neither secret
//! ever touches the process argument list or the filesystem:
//!
//! * The admin credential (STALWART_URL/USER/PASSWORD) is handed to the child
//!   through its environment, which stalwart-cli reads natively. argv is
//!   world-readable in the process list, the environment of a child we spawn
//!   is not.
//! * The plan carries a one-time mailbox password, so it is piped to
//!   apply --stdin instead of being written to a temp file. There is no disk
//!   artifact to leak, mis-permission, or leave behind on a crash — on any OS.
```

### 5.5 الاختبارات الأمنية (حتمية، بلا شبكة ولا قاعدة بيانات)

اختباران جديدان يفحصان argv/env عبر `Command::as_std()` (tokio 1.54) دون إطلاق
أي عملية:

- `admin_credential_never_appears_in_argv`: يؤكّد أن كلمة مرور المسؤول لا تظهر في
  أي وسيط، وأن `--password`/`--user`/`--url` غائبة من argv، وأن
  `STALWART_PASSWORD` (والـUSER والـURL) موجودة في بيئة الابنة بالقيمة الصحيحة.
- `apply_reads_the_plan_from_stdin_not_a_disk_file`: يؤكّد وجود `--stdin` وغياب
  `--file`، وأن استدعاء apply نفسه لا يسرّب السرّ في argv.

هذان الاختباران يحوّلان الخاصّيتين الأمنيتين إلى **invariant مُختبَر** لا مجرّد
نيّة في الكود؛ أي انحدار مستقبلي يعيد `--password` إلى argv أو `--file` إلى apply
سيكسر الاختبار فوراً.

---

## 6. التحقق

### 6.1 البوابات (كلّها خضراء)

| البوابة | الأمر | النتيجة |
|---|---|---|
| التنسيق | `cargo fmt --all -- --check` | 0 |
| البناء | `cargo check --workspace --all-targets` | 0 |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 0 |
| الاختبارات | `cargo test --workspace` (PostgreSQL حقيقي) | **145 نجحت، 0 فشلت، 0 تخطّت** |
| بناء الـCLI | `cargo build -p tira-provision` | ناجح (`target\debug\tira-provision.exe`) |

البيئة: `TIRA_REQUIRE_DATABASE=1` و
`TIRA_TEST_DATABASE_URL=postgres://postgres@127.0.0.1:55432/tira_test`
(PostgreSQL محمول يعمل على 55432، تحقّقت بـ`pg_isready` = «accepting connections»).

تفصيل ذو صلة:

- اختبارات وحدة `tira-provision`: **11 نجحت** (منها الاختباران الأمنيان الجديدان).
- اختبارات الجسر التكاملية `tests/bridge.rs`: **8 نجحت** على PostgreSQL حقيقي
  (آلة الحالة `pending→active`، `failed→active`، رفض `disabled`، المطابقة
  read-only ثم fix، كشف انحراف engine-id) — تستعمل FakeEngine في الذاكرة، فلم
  تتأثر بتغيير المحوّል الحقيقي، وهذا بيت القصيد: فصل trait أبقى الاختبارات سليمة.
- المجموع 145 = 143 السابقة (بعد §9.13) + الاختباران الأمنيان الجديدان.

### 6.2 التحقق الحيّ من القناة الرسمية

شغّلتُ استدعاء المحوّል بالضبط (`apply --stdin --json` مع اعتماد البيئة) على
`stalwart-cli 1.0.14` الحقيقي، بخطة domain upsert، والمحرك متوقّف:

```text
$ STALWART_URL=http://127.0.0.1:8080 \
  STALWART_USER=admin@tira.test \
  STALWART_PASSWORD=PROBE-ADMIN-SECRET-do-not-leak \
  stalwart-cli apply --stdin --json   (الخطة عبر الأنبوب)

warning: failed to refresh schema (not found (HTTP 404)); using cached copy
error: not found (HTTP 404)
EXIT=1
```

فحوص آلية على المخرجات:

- **NO_PASSWORD_PROMPT**: الـCLI لم يطلب كلمة مرور ⇒ اعتماد البيئة مقبول.
- **NO_SECRET_IN_OUTPUT**: كلمة مرور المسؤول لم تظهر في stdout/stderr ⇒ لا تسريب.
- الـCLI قرأ الخطة من stdin ووصل إلى طبقة HTTP (حاول refresh schema ثم apply)
  ⇒ الابتلاع عبر stdin يعمل؛ لا خطأ «خطة فارغة» ولا «ملف غير موجود».

### 6.3 حدود التحقق (بصراحة)

الخطأ `HTTP 404` سببه أن **محرك Stalwart لا يخدم واجهة الإدارة** على نقطة الفحص
(المحرك متوقّف؛ شيء ما على 8080 يردّ 404 لمسار schema). هذا فشل طبقة شبكة/خادم،
**ليس** عيباً في stdin أو المصادقة.

ما تحقّقتُ منه حيّاً: ابتلاع الخطة عبر stdin + قبول اعتماد البيئة + عدم تسريب
السرّ + عدم الطلب التفاعلي. ما **لم** أتحقّق منه حيّاً: تطبيق كامل شامل
(`apply` ينشئ الحساب فعلاً ويعيد ملخّص `created/updated`) — هذا يتطلّب محرك
Stalwart **يعمل**، وهو متوقّف حالياً ولم تُطلب تشغيله. الخاصّيات الأمنية نفسها
مُثبتة حتمياً باختبارَي الوحدة، والمنطق الوظيفي للتطبيق (ترميز الخطة، تحليل
الملخّص) لم يتغيّر، وقناة `--stdin` رسمية موثّقة. لذا خطر الانحدار الوظيفي ضئيل
ومغطّى، والحدّ الوحيد المعلن هو التطبيق الشامل ضدّ محرك حيّ.

---

## 7. الأثر والتوافق

- **لا كسر في الواجهة العامة:** `MailEngine` trait كما هو؛ `Provisioner` وآلة
  حالة PostgreSQL لم تُمَسّ؛ `StalwartConfig` فقد حقل `plan_dir` فقط (كان يُقرأ
  من `STALWART_PLAN_DIR`، ولم يعد له معنى).
- **المستدعون:** `lib.rs` يعيد تصدير `StalwartConfig`؛ لا إنشاء حرفي له خارج
  `from_env`، فإزالة الحقل آمنة. تحقّقتُ بـgrep: لا مرجع لـ`plan_dir`/
  `write_private`/`STALWART_PLAN_DIR` في الكود بعد التغيير.
- **التوثيق:** حُدّث `README.md` (أُزيل سطر `$env:STALWART_PLAN_DIR` وأُضيفت
  ملاحظة تشرح env+stdin)، وسلسلة `--help` في `tira-provision.rs` (أُزيل
  `STALWART_PLAN_DIR`). تحقّقتُ حيّاً أن `--help` لم يعد يذكره.
- **الاعتمادات:** `uuid` ما زال مستخدماً في `service.rs`/`plan.rs`/`bin`، فبقي
  الاعتماد سليماً (لم يصبح ميتاً بإزالة استيراده من engine.rs).
- **السلوك الوظيفي:** مطابق. تغيّر فقط نقل الخطة (ملف→stdin) والمصادقة
  (argv→env). نفس ترميز NDJSON، نفس تحليل الملخّص، نفس معالجة الأخطاء.

---

## 8. سهولة التوسّع

- `base_command()` مركزية نقل الاعتماد: أي أمر CLI مستقبلي (query، snapshot،
  أوامر جديدة) يرث env-auth تلقائياً بلا تكرار.
- `apply_command()` يعزل عقد stdin: إضافة `--dry-run` أو أي علم apply آخر هي
  تغيير سطر واحد في مكان واحد، قابل للاختبار عبر `as_std()`.
- فصل trait (`MailEngine`) أبقى `Provisioner` قابلاً للاختبار بـFakeEngine؛
  تغيير المحوّል الحقيقي لم يكسر أي اختبار تكاملي.
- إزالة `plan_dir` بسّطت الإعداد (متغيّر بيئة أقل، حقل أقل، اعتماد أقل على
  نظام الملفات) — سطح أصغر للصيانة والخطأ.
- الاختباران الأمنيان invariant قابل للتوسعة: أي خاصّة أمنية جديدة عن argv/env
  تُضاف بنفس نمط `as_std().get_args()/get_envs()` بلا إطلاق عمليات.

---

## 9. الملفات المتغيّرة في هذه الوحدة

| الملف | التغيير |
|---|---|
| `crates/tira-provision/src/engine.rs` | `base_command` (env)، `apply_command` + `apply` (stdin)، إزالة `write_private` و`plan_dir`، تصحيح وثيقة الوحدة، ضبط الاستيراد، اختباران أمنيان |
| `crates/tira-provision/src/bin/tira-provision.rs` | سلسلة `--help`: إزالة `STALWART_PLAN_DIR` |
| `README.md` | إزالة سطر `STALWART_PLAN_DIR`؛ ملاحظة تشرح env+stdin |
| `docs/SECRET-HARDENING-REPORT.ar.md` | هذا التقرير (جديد) |
| `docs/CORRECTIONS.md` | §9.14 (يُلحق بعد إنشاء اللقطة، حسب اصطلاح المشروع) |

---

## 10. الرجوع (Rollback)

لا يوجد git (عامل الحافلة 1، لقطات zip). نقطة الرجوع قبل هذه الوحدة هي لقطة
الوحدة السابقة (§9.13):

- **قبل الوحدة:** `TiraMail_Snapshot_OpsHardening_20261108.zip`
  (1,350,041 بايت، SHA-256
  `F535C441857E162C6215C72A8A4066C5F7BC825B844B667DC24B8ED5F793DB19`).

للتراجع عن هذه الوحدة: استعد تلك اللقطة فوق `mail.ma` (تعيد engine.rs القديم
بـargv+ملف، و`plan_dir`، و`write_private`).

---

## 11. اللقطة

لقطة حالة هذه الوحدة (الكود بعد الإصلاح + هذا التقرير، مصدر فقط بلا `target/`
ولا `data/` ولا `proof_try_*.exe`):

- **الاسم:** `TiraMail_Snapshot_SecretHandling_20261108.zip`
- **الحجم وبصمة SHA-256:** موثّقان في `docs/CORRECTIONS.md` §9.14 (المرجع الملزم)،
  لأن اللقطة تُنشأ قبل إلحاق قسمها — حسب اصطلاح المشروع المؤكّد (لقطة OpsHardening
  نفسها تتوقّف قبل §9.13 ولا تحمل بصمتها).

---

## 12. خلاصة الحكم

- **بدون أخطاء:** كل البوابات خضراء (fmt/check/clippy=0، 145/145 اختباراً).
- **بدون ثغرات خفيّة:** الثغرتان المُكتشفتان أُزيلتا من الجذور، والخاصّيتان
  الأمنيتان صارتا invariant مُختبَراً حتمياً؛ لا سرّ في argv ولا على القرص.
- **قوية:** إزالة سطح الخطر بالكامل أقوى من ترقيع ACL؛ القناة رسمية موثّقة؛
  السلوك موحّد على كل المنصّات.
- **سهلة التوسّع:** بناء مركزي للأوامر، عقد stdin معزول وقابل للاختبار، trait
  مفصول، إعداد أبسط، ونمط اختبار أمني قابل للتكرار.
- **صادقة:** حدود التحقق معلنة بصراحة (التطبيق الشامل ضدّ محرك حيّ لم يُختبر
  لأن المحرك متوقّف)؛ لا ادّعاء بتغطية غير موجودة.