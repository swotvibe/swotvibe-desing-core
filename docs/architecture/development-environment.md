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

## حالة المستودع الحالية

أنشئت هيكلية المشروع: workspace في `Cargo.toml` يضم ست حزم Rust تحت `crates/`. شريحة النواة الحالية منفذة في `swotvibe-core` و`swotvibe-format`؛ لا تعني اكتمال مسار M0 البصري المحدد في المواصفة.

| الحزمة | المسار | المسؤولية |
|---|---|---|
| `swotvibe-core` | `crates/core` | نموذج المستند والمعرفات والأوامر والمعاملات والتاريخ والتحقق والمحرك |
| `swotvibe-format` | `crates/format` | DTOs والترحيل وJSON وZIP64 محدود وحمولات الأصول |
| `swotvibe-layout` | `crates/layout` | عقد محول التخطيط |
| `swotvibe-text` | `crates/text` | عقد قياس وتشكيل النص |
| `swotvibe-render` | `crates/render` | عقد استخراج المشهد للرسم |
| `swotvibe-tools` | `crates/tools` | أدوات Headless وCLI |

اتجاه التبعية: الحزم الأخرى تعتمد على `core`، ولا تعتمد `core` على أي منها.

### التبعيات الخارجية المعتمدة

| الاعتماد المباشر | الإصدار في manifests | الاستخدام |
|---|---|---|
| `uuid`, `thiserror`, `serde`, `serde_json` | راجع `Cargo.toml` و`Cargo.lock` | هوية، أخطاء، DTOs، ومحتوى JSON |
| `zip` | `=8.6.0`, بلا ميزات افتراضية | profile ZIP64 المحدود في `format` |
| `tempfile` | `3.27.0` | ملفات مؤقتة للحفظ والاستبدال في CLI |
| `windows-sys` | `0.61` (Windows فقط) | `ReplaceFileW` لحفظ ACL أثناء الاستبدال |
| `sha2` | `0.10.9` (اختبارات فقط) | بصمات الحتمية وملفات corpus |

هذه قائمة الاعتمادات المباشرة فقط؛ `Cargo.lock` يسجل الإصدارات العابرة المحلولة، و`cargo deny` يتحقق من التراخيص والتنبيهات. تُستخدم `serde` في `format` فقط؛ نموذج runtime في `core` لا يشتق التسلسل.

### أدوات التحقق الخارجية على هذا الجهاز

توجد أدوات الاختبار في `.tools/` داخل المشروع، وهو مجلد محلي مستثنى من Git؛ لم يتغير PATH الدائم للنظام. تم التحقق من نواتج ZIP على Windows باستخدام Python `zipfile` وInfo-ZIP (`unzip 6.00`, `zipinfo 3.00` من Git for Windows) و7-Zip Extra console `7za` إصدار `26.03`. كما ثُبت GitHub CLI `2.102.0` محلياً لاستخدامه عند توفر remote ومصادقة GitHub. ملفات الأدوات المحمولة لا تُضمّن في المستودع؛ مصادرها [7-Zip الرسمية](https://www.7-zip.org/download.html) و[إصدار GitHub CLI](https://github.com/cli/cli/releases/tag/v2.102.0)، وطوبقت بصمات SHA-256 مع بيانات الإصدارات وقت التنزيل.

لتكرار تحقق ZIP الخارجي في PowerShell من جذر المشروع:

```powershell
$gitRoot = Split-Path (Split-Path (Get-Command git).Source)
$env:PATH = "$PWD\.tools\7zip;$gitRoot\usr\bin;$env:PATH"
$env:SWOTVIBE_REQUIRE_EXTERNAL_ZIP_TOOLS = "1"
cargo test --workspace
```

## حالة بوابة M0

شريحة النواة الرأسية منفذة ومختبرة في `swotvibe-core` و`swotvibe-format`:

- بنية المستند، الحفظ، وإعادة الفتح عبر DTO مخطط بإصدار صريح: لا فقد دلالي (اختبار ثبات البايتات).
- ترتيب/نقل في معاملة واحدة، ثم Undo/Redo، ثم round-trip بعد الحفظ: كلها تمر.
- الدفعة الفاشلة لا تترك أثراً: المراجعة والتاريخ والبنية تبقى كما هي.

اختبار الشريحة في `crates/format/tests/m0_vertical_slice.rs`. هذا لا يعني اجتياز بوابة M0 كاملة في §12.2؛ فالتخطيط والرسم ومقارنة الصورة المرجعية لم تنفذ.

### ما لم يثبت بعد

لا توجد بعد محولات تخطيط أو نص أو رسم منفذة (`DEC-LAYOUT`, `DEC-TEXT-AR`, `DEC-RENDERER` معلّقة)، لذا خطوات التخطيط والرسم وgolden في §12.2 لم تُنفذ. codec ZIP64 وCLI منفذان. نجح GitHub Actions على Windows وLinux وmacOS؛ ونجحت على Linux قارئات Python/Info-ZIP/7-Zip وfuzz موسع من 500,000 طفرة في التشغيل [37270260436](https://github.com/swotvibe/swotvibe-desing-core/actions/runs/37270260436). تبقى ملفات منتج حقيقية لتحديد حدود الموارد؛ لذا لم تنشر حدود افتراضية أو امتداد ملف ثابت. estimator ذاكرة Undo غير منفذ. `rust-version` الحالي يحدد 1.99.0 للبناء داخل المشروع؛ سياسة MSRV ودعم المنصات للإصدارات المنشورة لم تعتمدا بعد.
