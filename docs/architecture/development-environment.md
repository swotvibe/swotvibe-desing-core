# بيئة التطوير

## النطاق

هذه الإعدادات مخصصة لمجلد هذا المشروع. يحدد `rust-toolchain.toml` إصدار Rust عند العمل من جذر المستودع، ويقترح مجلد `.vscode` إضافة Rust Analyzer في VS Code. يحتفظ Rustup بالأدوات في مجلد المستخدم ويشارك مخبأ Cargo بين المشاريع؛ أما إصدار المترجم ومكونات المشروع فتحدد هنا.

لا يوجد مترجم افتراضي عام في Rustup على هذا الجهاز؛ يختار Rustup النسخة المثبتة عند دخول هذا المشروع فقط. بعد التثبيت أعد تشغيل VS Code كي ترث الطرفية والإضافة مسار Rustup المضاف حديثاً إلى PATH.

## الأدوات المعتمدة حالياً

| الأداة | الإصدار أو الهدف | الغرض |
|---|---|---|
| Rustup | 1.29.1 | إدارة سلاسل أدوات Rust |
| Rust | 1.99.0 | إصدار مثبت للمشروع في `rust-toolchain.toml` |
| المكونات | `rustfmt`, `clippy` | تنسيق الشيفرة وتحليلها الساكن |
| هدف Rust | `x86_64-pc-windows-msvc` | البناء الأصلي على Windows x64 عبر MSVC |
| Visual Studio Build Tools | 2022، workload `Microsoft.VisualStudio.Workload.VCTools` | رابط MSVC وWindows SDK اللازمان لبناء Rust على Windows |
| VS Code | إضافة `rust-lang.rust-analyzer` موصى بها للمجلد | إكمال الشيفرة والتشخيص والتنقل |

تُثبّت أدوات Visual Studio المشتركة خارج مجلد المشروع في `D:\DevTools\SwotvibeDesignBuildTools` لتجنب ملء قرص النظام أو خلط ملفاتها مع الشيفرة. هذا اعتماد بناء على مستوى الجهاز، بينما اختيار إصدار Rust خاص بهذا المشروع.

## طريقة التفعيل

1. افتح مجلد المشروع نفسه في VS Code، لا مجلد `docs` وحده.
2. وافق على تثبيت الإضافة الموصى بها `rust-analyzer` إذا لم تكن موجودة.
3. افتح طرفية جديدة من جذر المشروع. يقرأ Rustup `rust-toolchain.toml` ويختار Rust 1.99.0. في أول تشغيل قد ينزل سلسلة الأدوات ومكوناتها.
4. بعد إنشاء crate، تحقق باستخدام `rustc --version`, `cargo --version`, `rustfmt --version`, و`cargo clippy --version`.

## أوامر التحقق من الهيكل

تعمل كل الأوامر التالية من **جذر المستودع** حيث يطبّق `rust-toolchain.toml`:

```sh
cargo build --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check licenses advisories bans
cargo run -p swotvibe-tools
```

### تشغيل بوابة M0 وحدها

```sh
cargo test -p swotvibe-tools --test m0_vertical_slice
```

يفتح مستند العينة، ويتحقق منه، ويحفظه ويعيد فتحه، ثم يخطّطه ويرسمه ويقارن تقرير layout والصورة PNG بمرجعين مقفلين في `tests/golden`. المرجع يُعاد توليده — بعد مراجعة بشرية للتغيير — بالأمر:

```sh
$env:SWOTVIBE_UPDATE_GOLDEN = "1"; cargo test -p swotvibe-tools --test m0_vertical_slice
```

على نظام مختلف، تُضاف مراجع خاصة بالمنصة باسم `tests/golden/m0-sample.<windows|linux|macos>.png` بدل توسيع tolerance البكسل. راجع [tests/golden/README.md](../../tests/golden/README.md).

### اختبارات المحولات وحدها

```sh
cargo test -p swotvibe-core -p swotvibe-format -p swotvibe-text -p swotvibe-layout -p swotvibe-render
```

تقرأ هذه الاختبارات خطوط المستودع المثبتة من `assets/fonts` وتُسجّلها صراحةً؛ لا تعتمد على خطوط النظام، لذا تعطي النتيجة نفسها على أي جهاز.

## حالة المستودع الحالية

أنشئت هيكلية المشروع: workspace في `Cargo.toml` يضم سبع حزم Rust تحت `crates/`. المسار الرأسي M0 منفذ من النموذج حتى الصورة المرجعية؛ ويبقى اختيار المحولات النهائي مفتوحاً وقرارات `DEC-*` معلّقة. وأضيفت طبقة تطبيق `swotvibe-app` ومضيف سطح مكتب تحت `apps/desktop` خارج مساحة العمل.

| الحزمة | المسار | المسؤولية |
|---|---|---|
| `swotvibe-core` | `crates/core` | نموذج المستند والخصائص والهندسة والمعرفات والأوامر والمعاملات والتاريخ والتحقق والمحرك |
| `swotvibe-format` | `crates/format` | DTOs (v2) والترحيل وJSON وZIP64 محدود وحمولات الأصول |
| `swotvibe-layout` | `crates/layout` | عقد محول التخطيط وتنفيذ مؤقت على Taffy 0.14 |
| `swotvibe-text` | `crates/text` | عقد قياس وتشكيل النص وتنفيذ مؤقت على Parley 0.11 + Skrifa 0.44 |
| `swotvibe-render` | `crates/render` | عقد استخراج المشهد للرسم وتنفيذ مؤقت على vello_cpu 0.3 |
| `swotvibe-app` | `crates/app` | طبقة التطبيق: جلسة المحرر، الأوامر، اللقطات، الهندسة المشتقة، المعاينة، والأخطاء المطبوعة |
| `swotvibe-tools` | `crates/tools` | أدوات Headless وCLI |

اتجاه التبعية: الحزم الأخرى تعتمد على `core`، ولا تعتمد `core` على أي منها. ولا تتسرب أنواع Taffy أو Parley أو vello_cpu إلى `core` ولا إلى المخطط الدائم. و`swotvibe-app` تعتمد على `core` و`format` و`layout` و`render`، ولا تعتمد على Tauri أو نافذة أو نظام ملفات.

### واجهة سطح المكتب `apps/desktop`

مستضيف Tauri هو **workspace منفصل** في `apps/desktop/src-tauri`، ومعه الواجهة في `apps/desktop/ui`. أسباب الفصل:

1. يحتاج المضيف مكتبات منصة (WebKitGTK على Linux مثلاً) لا تتوفر في مشغّل CI مجرّد؛ فدمجه في workspace المستودع يجعل `cargo test --workspace` و`cargo clippy --workspace` و`cargo fmt --all` تعتمد على الجهاز.
2. المضيف لا يملك قواعد المستند: كل سلوك قابل للاختبار موجود في `swotvibe-app`.
3. لا يُدّعى دعم منصة قبل بنائها وتشغيلها عليها. جُرّب المضيف على Windows فقط.

| مسار | ما هو | الحالة |
|---|---|---|
| `apps/desktop/src-tauri` | مضيف Tauri: نافذة، حوار ملفات أصلي، أوامر IPC رقيقة فوق `swotvibe-app` | يُبنى ويعمل على Windows؛ بوابة القبول جزئية |
| `apps/desktop/ui` | واجهة Vue 3 وVite مع TypeScript `6.0.3` مقفل | تُبنى؛ 17 اختبار مكوّن ناجح |

### الواجهة والجسر

تسجل [خطة الواجهة وM1](./ui-and-m1-plan.md) القرارات والبوابات وما لم يُثبت بعد، و[ADR-0009](../adr/0009-ui-and-bridge-boundary.md) يثبت حدّ طبقة التطبيق والمضيف. خلاصتها:

- الواجهة لا تستقبل مساراً على القرص، ولا تمنح صلاحيات ملفات: تطلب «افتح» فيعرض Rust الحوار ويقرأ الملف.
- لا يستورد أي مكوّن خدمة؛ القشرة تُنشئ الجلسة مرة واحدة وتوزّعها، وإلا اختلفت لوحة الطبقات ومساحة الرسم حول ما هو محدد.
- في المتصفح تعمل الواجهة نفسها على خدمة عينة لتُراجَع دون مضيف، والقشرة تختار مضيف Tauri عند وجود `__TAURI_INTERNALS__`.
- TypeScript مقفل عند `6.0.3` لأن خط 7 لا يوفر واجهة برمجية وتدعم أدوات Vue المنشورة خط 6 فقط. ترقية 7.x لاحقة ومشروطة، لا برقم إصدار موعود.

### التبعيات الخارجية المعتمدة

| الاعتماد المباشر | الإصدار في manifests | الاستخدام |
|---|---|---|
| `uuid`, `thiserror`, `serde`, `serde_json` | راجع `Cargo.toml` و`Cargo.lock` | هوية، أخطاء، DTOs، ومحتوى JSON |
| `zip` | `=8.6.0`, بلا ميزات افتراضية | profile ZIP64 المحدود في `format` |
| `tempfile` | `3.27.0` | ملفات مؤقتة للحفظ والاستبدال في CLI |
| `windows-sys` | `0.61` (Windows فقط) | `ReplaceFileW` لحفظ ACL أثناء الاستبدال |
| `sha2` | `0.10.9` | بصمات الخطوط والمراجع الحتمية |
| `taffy` | `=0.14.0`, بلا ميزات افتراضية | محول التخطيط المؤقت (ADR-0007) |
| `parley` | `=0.11.1`, بلا ميزات افتراضية (`std` فقط) | تشكيل النص وقياسه وكسر الأسطر (ADR-0006) |
| `skrifa` | `=0.44.0` | استخراج حدود المحارف (ADR-0006) |
| `vello_cpu` | `=0.3.0`, ميزة `f32_pipeline` | التنقيط المرجعي وإخراج PNG (ADR-0008) |
| `png` | `0.18.1` | ترميز وفك ترميز الصورة المرجعية |

هذه قائمة الاعتمادات المباشرة لمساحة العمل فقط؛ `Cargo.lock` يسجل الإصدارات العابرة المحلولة، و`cargo deny` يتحقق من التراخيص والتنبيهات. تُستخدم `serde` في `format` فقط؛ نموذج runtime في `core` لا يشتق التسلسل. الاعتمادات الثلاثة للمحولات مثبتة بإصدار دقيق (`=`)، لأن تحديثاً صغيراً قد يحرّك صورة مرجعية أو قياساً.

مضيف سطح المكتب خارج مساحة العمل وله ملف قفل خاص به؛ اعتماداته المباشرة: `tauri` 2.12 و`tauri-plugin-dialog` 2.8.1 و`tauri-build` 2، إضافة إلى حزم المشروع الثلاث `swotvibe-app` و`swotvibe-core` و`swotvibe-text` عبر مسار نسبي.

### أدوات التحقق الخارجية على هذا الجهاز

توجد أدوات الاختبار في `.tools/` داخل المشروع، وهو مجلد محلي مستثنى من Git؛ لم يتغير PATH الدائم للنظام. تم التحقق من نواتج ZIP على Windows باستخدام Python `zipfile` وInfo-ZIP (`unzip 6.00`, `zipinfo 3.00` من Git for Windows) و7-Zip Extra console `7za` إصدار `26.03`. كما ثُبت GitHub CLI `2.102.0` محلياً لاستخدامه عند توفر remote ومصادقة GitHub. ملفات الأدوات المحمولة لا تُضمّن في المستودع؛ مصادرها [7-Zip الرسمية](https://www.7-zip.org/download.html) و[إصدار GitHub CLI](https://github.com/cli/cli/releases/tag/v2.102.0)، وطوبقت بصمات SHA-256 مع بيانات الإصدارات وقت التنزيل.

لتكرار تحقق ZIP الخارجي في PowerShell من جذر المشروع:

```powershell
$gitRoot = Split-Path (Split-Path (Get-Command git).Source)
$env:PATH = "$PWD\.tools\7zip;$gitRoot\usr\bin;$env:PATH"
$env:SWOTVIBE_REQUIRE_EXTERNAL_ZIP_TOOLS = "1"
cargo test --workspace
```

## بيئة الواجهة وسطح المكتب

### Node.js المحلي

لا يوجد Node.js مثبت على مستوى النظام. تُستخدم نسخة محمولة محلية في `.tools/node` (مستثنى من Git)، وهي Node `24.21.0` LTS مع npm `11.19.0`. تُشغَّل أوامر الواجهة بجعل مسارها أولاً في `PATH`:

```powershell
$env:PATH = "$PWD\.tools\node\node-v24.21.0-win-x64;$env:PATH"
```

لا يُسجل ذلك في PATH الدائم ولا يُضمّن في المستودع، على غرار أدوات ZIP وgh. متطلبات Vite وVitest المقفلة (`^22.12.0 || ^24.0.0 || >=26.0.0`) تتحقق بهذه النسخة.

### أوامر الواجهة

من `apps/desktop/ui`:

```sh
npm install          # يقرأ package-lock.json
npm run typecheck    # vue-tsc --noEmit مع typescript 6.0.3
npm test             # Vitest + Vue Test Utils
npm run build        # typecheck ثم vite build
npm run dev          # خادم تطوير على 127.0.0.1:5173
```

خادم التطوير مربوط على IPv4 (`127.0.0.1`) صراحةً: الافتراضي قد يُحلّ إلى `::1` على بعض إعدادات Windows، وهو عنوان لا تصل إليه كل أدوات سطح المكتب. راجع [خطة الواجهة وM1](./ui-and-m1-plan.md) للقرارات وبوابات القبول وما لم يُثبت بعد.

### أوامر سطح المكتب

من `apps/desktop/src-tauri`:

```sh
cargo build               # ينتج swotvibe-design.exe
cargo test --lib          # اختبارات المضيف وبدء التشغيل
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo run --bin swotvibe-design
```

المضيف workspace منفصل، لذا لا يغطيه `cargo fmt --all` ولا `cargo clippy --workspace` من جذر المستودع. يُشغَّل فحص التنسيق له في CI على الأنظمة الثلاثة لأنه لا يحتاج مكتبات منصة، بينما يبقى بناؤه خارج المصفوفة حتى قرار دعم المنصات.

المضيف يقرأ الخطوط المثبتة من `assets/fonts` بالبحث من مجلد الحزمة إلى أعلى؛ ويقبل `SWOTVIBE_FONT_DIR` لتجاوز ذلك، وهو ما تستخدمه حزمة موزعة. لا يقرأ أي خط من النظام، لأن ذلك يجعل القياس يعتمد على الجهاز.

أيقونات `icons/` مؤقتة ومولّدة في المستودع (مربع بلون التمييز) لأن البناء يحتاج ملف أيقونة؛ تصميم أيقونة المنتج قرار منفصل.

### حالة سطح المكتب

جُرّب المضيف على Windows فقط: يُبنى ويشغّل نافذة، و4 اختبارات مضيف تنجح. لم يُختبر دعم Linux أو macOS، ولا مسار الحوار الأصلي آلياً، ولا مسار WebDriver. لا يُدّعى أي منها قبل اختباره.

## حالة بوابة M0

المسار الرأسي منفذ ومختبر عبر حدود النظام كاملة:

- بنية المستند، الحفظ، وإعادة الفتح عبر DTO مخطط بإصدار صريح: لا فقد دلالي (اختبار ثبات البايتات)، مع ترحيل v1 → v2.
- إنشاء مستند بصفحة وإطار وأشكال ونص بخطوط مثبتة: `tests/fixtures/m0-sample-v2.json`.
- عينة واجهة محرر عربية 1440×900، مع أصول SVG/PNG مولدة: `tests/fixtures/m0-editor-ui-v2.json`.
- تمرير المستند عبر محول تخطيط واحد (Taffy) وراسم واحد (vello_cpu) خلف العقدين الموثقين.
- إخراج PNG وتقرير layout ومقارنتهما بمرجعين مقفلين بالبصمة: `tests/golden/m0-sample.*`.
- ترتيب/نقل وتغيير خاصية في معاملة واحدة، ثم Undo/Redo، ثم round-trip بعد الحفظ: كلها تمر.
- الدفعة الفاشلة لا تترك أثراً: المراجعة والتاريخ والبنية والهندسة تبقى كما هي.

اختبار المسار الأساسي في `crates/tools/tests/m0_vertical_slice.rs`، واختبار واجهة المحرر في `crates/tools/tests/m0_editor_ui.rs`. الاختباران في مصفوفة CI، ونجحا على Windows وLinux وmacOS في التشغيل [37393874233](https://github.com/swotvibe/swotvibe-desing-core/actions/runs/37393874233). تسجّل بصمتا الصور اعتماداً بصرياً من مالك المشروع للتنسيق واتجاه النص بتاريخ 2026-10-06؛ وإعادة توليد أي مرجع تعيد حقل `reviewed` إلى حالة غير معتمدة حتى تُراجع الصورة الجديدة.

### ما لم يثبت بعد

لا يعني اجتياز المسار اختيار المحولات: `DEC-LAYOUT`, `DEC-TEXT-AR`, `DEC-RENDERER` ما زالت معلّقة، والتنفيذ الحالي مؤقت ومسجل في ADR-0006 إلى ADR-0008 مع حالاته غير المدعومة معروضة كتشخيصات (صور بلا بكسلات، أنماط خطوط مُصنّعة، `hug` بلا محتوى، فرض اتجاه فقرة). لم تُقارن Yoga وVello GPU وSkia على نفس المشاهد. لم تُختبر IME ولا المؤشر والتحديد ولا تخطيط التكلفة على مستندات كبيرة. تبقى ملفات منتج حقيقية لتحديد حدود الموارد؛ لذا لم تنشر حدود افتراضية أو امتداد ملف ثابت. estimator ذاكرة Undo غير منفذ. `rust-version` الحالي يحدد 1.99.0 للبناء داخل المشروع؛ سياسة MSRV ودعم المنصات للإصدارات المنشورة لم تعتمدا بعد. نجح GitHub Actions على Windows وLinux وmacOS؛ ونجحت على Linux قارئات Python/Info-ZIP/7-Zip وfuzz موسع من 500,000 طفرة في التشغيل [37270260436](https://github.com/swotvibe/swotvibe-desing-core/actions/runs/37270260436).
