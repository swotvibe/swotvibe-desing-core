# سجل أدوات المشروع

**الحالة:** مرجع توثيقي للأدوات المعتمدة في المشروع، بإصداراتها المقفلة وتراخيصها ومصادرها الرسمية.  
**تاريخ التحقق:** 2026-10-06  
**مصدر الحقيقة:** ملفات القفل (`Cargo.lock` و`package-lock.json`) و`rust-toolchain.toml`، لا ملفات الإعداد. النطاقات في `Cargo.toml` و`package.json` تصف النوايا؛ القفل يصف ما يُبنى به فعلاً.

## 1. قواعد هذا السجل

1. **الإصدار المقفل هو المعتمد.** أي رقم هنا مقروء من ملف قفل بتاريخ التحقق أعلاه. ترقية تعني تحديث القفل وإعادة التحقق، لا تعديل نطاق.
2. **كل أداة لها مسؤولية واحدة موثقة.** لا تُضاف أداة «لأنها مفيدة»؛ تُضاف لأنها تملك مسؤولية لا يملكها غيرها، ويُسجل سببها.
3. **الترخيص يُفحص قبل الاعتماد.** القائمة المسموحة في `deny.toml`: `MIT`, `Apache-2.0`, `Unicode-3.0`, `Zlib`, `BSD-2-Clause`, `BSD-3-Clause`. ترخيص خارجها يحتاج قراراً صريحاً.
4. **الميزات المفعّلة تُوثق.** ميزة مفعّلة بلا حاجة هي سطح هجوم وحجم ثنائي زائد؛ ميزة معطّلة بلا سبب موثق مفاجأة مستقبلية.
5. **MSRV يُسجل ولا يُعلن.** أعلى MSRV بين الاعتمادات يحدد الحد الأدنى الفعلي للبناء؛ إعلان MSRV رسمي للمشروع قرار منفصل لم يُحسم.

## 2. سلسلة أدوات Rust

| الأداة | الإصدار | المصدر | الدور |
|---|---|---|---|
| Rust | `1.99.0` | `rust-toolchain.toml` | المترجم المعتمد للمشروع؛ يختاره Rustup عند دخول مجلد المستودع |
| cargo | `1.99.0` | مع سلسلة الأدوات | البناء وإدارة الاعتمادات |
| rustfmt | مع سلسلة الأدوات | مكوّن معلن | التنسيق؛ الإعداد في `rustfmt.toml` (`max_width = 100`, `newline_style = "Unix"`) |
| clippy | مع سلسلة الأدوات | مكوّن معلن | التحليل الساكن؛ يُشغَّل بـ`-D warnings` في CI |
| cargo-deny | `0.20.2` | مثبت محلياً | فحص التراخيص والتنبيهات والحظر (`deny.toml`) |
| الهدف | `x86_64-pc-windows-msvc` | `rust-toolchain.toml` | هدف البناء المحلي؛ مصفوفة CI تضيف Linux وmacOS |

**MSRV الفعلي:** أعلى MSRV بين الاعتمادات المقفلة هو `1.90` (عائلة Tauri). أي بيئة بناة أقدم من ذلك ستفشل، وهذا قيد فعلي لا إعلان دعم.

## 3. أدوات النواة (crates)

### 3.1 نموذج المستند والبنية

| الحزمة | الإصدار المقفل | الترخيص | MSRV | المسؤولية | لماذا هذه |
|---|---|---|---|---|---|
| `serde` | `1.0.229` | MIT OR Apache-2.0 | 1.56 | التسلسل في `format` فقط؛ نموذج runtime في `core` لا يشتقه | المعيار الفعلي في منظومة Rust، وميزة `derive` تُستخدم في DTOs المحفوظة حصراً |
| `serde_json` | `1.0.151` | MIT OR Apache-2.0 | 1.71 | محتوى JSON وحدود القراءة | تكامل مباشر مع `serde`، ودعم `arbitrary_precision` غير مفعّل عمداً |
| `thiserror` | `2.0.21` | MIT OR Apache-2.0 | 1.77 | اشتقاق أنواع الأخطاء | يقلل نمط `Display`/`Error` المتكرر دون سلوك خفي |
| `uuid` | `1.27.0` | Apache-2.0 OR MIT | 1.89 | هوية العقد والصفحات والأصول | ميزات `v4` و`std` فقط؛ `v7` غير مفعّلة لأن الهوية عشوائية مقصودة |
| `sha2` | `0.10.9` | MIT OR Apache-2.0 | — | بصمات الخطوط والمراجع الحتمية | تنفيذ RustCrypto المرجعي؛ الإصدار `0.11` موجود لكن القفل يثبّت `0.10` |

### 3.2 التخطيط

| الحزمة | الإصدار المقفل | الترخيص | MSRV | المسؤولية | الميزات المفعّلة |
|---|---|---|---|---|---|
| `taffy` | `=0.14.0` | MIT | 1.71 | محرك التخطيط المؤقت (ADR-0007) | `std`, `taffy_tree`, `flexbox`, `block_layout`, `content_size` |

- ينفذ خوارزميات CSS Block وFlexbox وGrid بأمانة للمواصفة، فتوثيق MDN يترجم إليه مباشرة.
- مثبّت بإصدار دقيق (`=`) لأن تحديثاً صغيراً قد يحرّك قياساً أو صورة مرجعية.
- **Grid مفعّل كخاصية لكنه غير مستخدم في دلالات المنتج بعد**؛ تفعيله جاء مع الحزمة الافتراضية للشجرة، وتقييد الـfeatures أدقّ عمل مستقبلي.
- القرار النهائي (`DEC-LAYOUT`: مقارنة مع Yoga) مفتوح.

### 3.3 النص

| الحزمة | الإصدار المقفل | الترخيص | MSRV | المسؤولية |
|---|---|---|---|---|
| `parley` | `=0.11.1` | Apache-2.0 OR MIT | 1.88 | التشكيل وقياس النص وكسر الأسطر (ADR-0006) |
| `skrifa` | `=0.44.0` | MIT OR Apache-2.0 | 1.85 | قراءة محارف TrueType/OpenType واستخراج الحدود |
| `fontique` | `0.11.1` (عابر) | Apache-2.0 OR MIT | — | تعداد الخطوط والبدائل داخل Parley |

- مكدس Parley الداخلي: **Fontique** (تعداد وبدائل) و**HarfRust** (تشكيل، منفذ HarfBuzz إلى Rust) و**Skrifa** (قراءة الخطوط) و**ICU4X** (بيانات Unicode).
- ميزات Parley: `std` فقط؛ ميزات `accesskit` و`wasm` غير مفعّلة.
- Skrifa مثبّت على `0.44.0` بينما الأحدث المنشور `0.48.0`؛ الترقية تُجرى مع إعادة توليد المراجع ومراجعة بصرية لأن حدود المحارف تتغير بين الإصدارات.
- القرار النهائي (`DEC-TEXT-AR`: IME والتحديد والخطوط البديلة) مفتوح.

### 3.4 الرسم

| الحزمة | الإصدار المقفل | الترخيص | MSRV | المسؤولية |
|---|---|---|---|---|
| `vello_cpu` | `=0.3.0` | Apache-2.0 OR MIT | 1.89 | التنقيط المرجعي (ADR-0008) |
| `kurbo` | `0.13.1` (عابر) | Apache-2.0 OR MIT | 1.85 | الهندسة والمنحنيات عند حدّ الرسم |
| `peniko` | `0.6.1` (عابر) | Apache-2.0 OR MIT | 1.85 | الألوان والفرش |
| `png` | `0.18.1` | MIT OR Apache-2.0 | 1.73 | ترميز وفك ترميز PNG |
| `resvg` | `=0.48.1` | Apache-2.0 OR MIT | 1.85.0 | تنقيط SVG للأصول |

- `vello_cpu` بميزات `std` و`f32_pipeline`؛ ميزة `text` (glifo) **غير مفعّلة** عمداً: المحارف تُرسم بحدود من عقد النص، فمسار تشكيل واحد للقياس والرسم.
- ميزة `f32_pipeline` هي المسار الأدق والأبطأ؛ مقصودة لأن M0 صورة مرجعية لا حلقة معاينة.
- `resvg` بلا ميزات افتراضية: لا نصوص ولا موارد خارجية ولا صور نقطية مدمجة في SVG.
- القرار النهائي (`DEC-RENDERER`: Vello GPU مقابل Skia مقابل CPU) مفتوح.

### 3.5 الملفات والحاوية

| الحزمة | الإصدار المقفل | الترخيص | MSRV | المسؤولية |
|---|---|---|---|---|
| `zip` | `=8.6.0` | MIT | 1.88 | حاوية ZIP64 المحدودة (ADR-0003) |
| `tempfile` | `3.27.0` | MIT OR Apache-2.0 | 1.63 | ملفات مرحلية للحفظ والاستبدال في CLI |
| `windows-sys` | `0.61.2` | MIT OR Apache-2.0 | 1.71 | `ReplaceFileW` للحفظ مع حفظ ACL على Windows |

- `zip` بلا ميزات افتراضية: لا `deflate` ولا `bzip2` ولا `zstd` إلا ما يطلبه profile الحاوية.
- مبني على APPNOTE.TXT v6.3.9 من PKWARE.
- `windows-sys` Windows فقط (`[target.'cfg(windows)'.dependencies]`)؛ المنصات الأخرى لا تدفع ثمنه.

## 4. أدوات سطح المكتب (workspace منفصل)

| الحزمة | الإصدار المقفل | الترخيص | MSRV | المسؤولية |
|---|---|---|---|---|
| `tauri` | `2.12.1` | Apache-2.0 OR MIT | 1.90 | النافذة وIPC وحالة التطبيق |
| `tauri-build` | `2.7.1` | Apache-2.0 OR MIT | 1.90 | سكربت البناء وتوليد schemas الصلاحيات |
| `tauri-plugin-dialog` | `2.8.1` | Apache-2.0 OR MIT | 1.90 | حوار فتح/حفظ الملفات الأصلي |
| `wry` | `0.57.0` (عابر) | Apache-2.0 OR MIT | 1.85 | غلاف WebView عبر المنصات |
| `tao` | `0.37.1` (عابر) | Apache-2.0 | 1.85 | حلقة النافذة والأحداث |

- **ميزة `custom-protocol` ليست افتراضية عمداً.** بدونها يضبط Tauri cfg اسمه `dev` ويحمّل المضيف `build.devUrl` حتى في بناء release، فتظهر `ERR_CONNECTION_REFUSED` بلا خادم تطوير. تفعّلها `tauri build` تلقائياً؛ وللبناء اليدوي: `cargo build --release --features custom-protocol`. التحقق: غياب `rustc-cfg=dev` في `target/release/build/swotvibe-desktop-*/output`.
- **WebView2** هو WebView على Windows، مبني على Edge/Chromium، ويتحدث ذاتياً. النسخة المثبتة على جهاز التطوير: `154.0.4258.62`. النسخة والسلوك يعتمدان على جهاز العميل، وهذا قيد موثق لا يُدّعى تجاوزه.
- **الصلاحيات:** capability واحدة (`default`) بصلاحية `core:default` فقط. لا صلاحيات ملفات للواجهة؛ المسارات تُدار في Rust.
- **CSP** معلنة في `tauri.conf.json` وتقيّد `default-src` و`connect-src` و`img-src` و`style-src`.
- `tauri-plugin-fs` يدخل عابراً مع plugin الحوار؛ لا تُمنح صلاحياته للواجهة.

## 5. أدوات الواجهة (npm)

| الحزمة | الإصدار المقفل | الترخيص | المسؤولية |
|---|---|---|---|
| `vue` | `3.5.43` | MIT | إطار الواجهة: SFC والتفاعل |
| `typescript` | `6.0.3` | Apache-2.0 | لغة الواجهة؛ مثبت بإصدار دقيق |
| `vite` | `8.3.3` | MIT | خادم التطوير والتغليف |
| `@vitejs/plugin-vue` | `6.0.9` | MIT | تكامل Vue مع Vite |
| `vue-tsc` | `3.3.12` | MIT | فحص أنواع ملفات `.vue` والقوالب |
| `tailwindcss` | `4.3.3` | MIT | التنسيق عبر tokens |
| `@tailwindcss/vite` | `4.3.3` | MIT | تكامل Tailwind مع Vite |
| `@lucide/vue` | `1.52.0` | ISC | الأيقونات |
| `@tauri-apps/api` | `2.12.1` | Apache-2.0 OR MIT | استدعاء أوامر المضيف من الواجهة |
| `vitest` | `5.0.3` | MIT | مشغّل اختبارات الواجهة |
| `@vue/test-utils` | `2.5.1` | MIT | أدوات اختبار مكوّنات Vue |
| `jsdom` | `28.1.0` | MIT | بيئة DOM للاختبارات |
| `@types/node` | `24.19.1` | MIT | أنواع Node لإعدادات Vite |

### 5.1 قيود التوافق الموثقة

| الحزمة | المتطلب | المصدر |
|---|---|---|
| `vite` | Node `^20.19.0 \|\| >=22.12.0` | `engines` في القفل |
| `vitest` | Node `^22.12.0 \|\| ^24.0.0 \|\| >=26.0.0` | `engines` في القفل |
| `jsdom` | Node `^20.19.0 \|\| ^22.12.0 \|\| >=24.0.0` | `engines` في القفل |
| `@vitejs/plugin-vue` | Vite `^5–^8` وVue `^3.2.25` | `peerDependencies` |
| `vue-tsc` | TypeScript `>=5.0.0` | `peerDependencies` |
| `@lucide/vue` | Vue `>=3.0.1` | `peerDependencies` |

**Node المعتمد:** `24.21.0` LTS (Krypton)، مثبت محمولاً في `.tools/node` وغير ملتزم. يحقق أضيق قيد أعلاه (Vitest).

### 5.2 TypeScript 6 مقابل 7 — القرار والدليل

- `typescript@6.0.3` مثبت بإصدار دقيق، وهو خط 6 المستقر.
- خط 7: `7.0.2` مستقر و`7.1` development tag؛ **لا يوجد `7.2` منشور**.
- إعلان فريق TypeScript لـ7.0 يوضح أن واجهة API البرمجية غير متاحة، وأن أدوات اللغات المضمنة (Vue وMDX وAstro وSvelte) تحتاج خط 6 حتى تتوفر واجهة 7.1.
- متتبع Vue Language Tools يوثق فشل `vue-tsc` مع `typescript@7.0.2` من 3.3.8 حتى 3.3.11، والحل المؤقت `typescript-native-bridge`، وPR #6170 مفتوح.
- **الترقية مشروطة لا مجدولة:** عند نشر إصدار 7.x جديد، تُشغَّل مصفوفة كاملة (فحص SFC والقوالب، أدوات المحرر، بناء Vite، lint، الاختبارات) ولا تُعتمد دون نجاحها دون shim غير رسمي.
- ملاحظة هجرة مفعّلة: `baseUrl` مهجور في 6.0 ومزال في 7؛ الإعدادات تستخدم `paths` نسبية إلى `tsconfig.app.json`.

### 5.3 Tailwind CSS 4 — ما يعنيه ذلك

- Tailwind 4 **أداة بناء CSS بذاتها** ولا يُستخدم مع Sass أو Less أو Stylus؛ الاستيراد والتداخل والمتغيرات تُدار داخلياً عبر Lightning CSS.
- دعم المتصفحات: Chrome 111+ وSafari 16.4+ وFirefox 128+. هذا مقبول لأن الواجهة تعمل داخل WebView2 الحديث، لكنه يصبح قيداً إذا أُضيف هدف ويب لعملاء قدماء.
- tokens المشروع معرّفة في `src/assets/main.css` عبر `@theme`؛ الألوان بـ`oklch`.

### 5.4 ما لم يُعتمد عمداً

| الأداة | السبب |
|---|---|
| ESLint و`typescript-eslint` و`eslint-plugin-vue` | لم تُثبت بعد. `typescript-eslint` يعلن نطاق `typescript >=4.8.4 <6.1.0` فيتوافق مع 6.0.3، لكن إضافته تحتاج قرار إعدادات وقواعد، وهو عمل مستقل |
| Reka UI | لا بدائيات وصول مطلوبة بعد؛ حوار الملفات أصلي من النظام |
| Pinia | حالة الجلسة مملوكة مرة واحدة في القشرة |
| `vue-router` | لا مسارات ولا شاشات متعددة بعد |
| مولّد أنواع Rust↔TypeScript (`tauri-specta` أو `ts-rs`) | العقد صغير؛ يُعتمد مولّد عند أن يصبح التكرار مصدر خطأ فعلي، مع proof يغطي enums و`Option` و`Result` وcamelCase |

## 6. أدوات البنية التحتية والتحقق

| الأداة | الإصدار | المكان | الدور |
|---|---|---|---|
| Node.js | `24.21.0` LTS | `.tools/node` (غير ملتزم) | تشغيل أدوات الواجهة |
| npm | `11.19.0` | مع Node | إدارة حزم الواجهة |
| Git | `2.54.0.windows.1` | النظام | التحكم بالإصدارات |
| GitHub CLI | `2.102.0` | `.tools/gh` | عمليات PR والتحقق من CI |
| 7-Zip | `26.03` | `.tools/7zip` | تحقق مستقل من نواتج ZIP |
| Info-ZIP unzip | `6.00` | مع Git for Windows | تحقق مستقل ثانٍ |
| Info-ZIP zipinfo | `3.00` | مع Git for Windows | فحص بنية الأرشيف |
| Python | `3.12.10` | النظام | مولّد fixtures و`zipfile` للتحقق المستقل الثالث |
| WebView2 Runtime | `154.0.4258.62` | النظام | WebView الذي يشغّل الواجهة في النافذة |

**قاعدة `.tools`:** أدوات محمولة لا تُلتزم، ومصادرها الموثقة مع بصمات SHA-256 وقت التنزيل في [development-environment.md](./development-environment.md). لا شيء منها يعدّل PATH الدائم للنظام.

## 7. CI (GitHub Actions)

| المهمة | المشغّل | الخطوات |
|---|---|---|
| `fmt, clippy, test` | `windows-latest`, `ubuntu-latest`, `macos-latest` | توليد fixture والتحقق من عدم تغيّره، rustfmt، clippy بـ`-D warnings`، `cargo test --workspace`، شريحتا M0 صراحةً، fuzz محدود |
| `interface` | `ubuntu-latest`، Node 24 | `npm ci`، `vue-tsc --noEmit`، Vitest، `vite build` |
| `licenses` | `ubuntu-latest` | `cargo deny check licenses advisories bans` |
| `scheduled-bundle-fuzz` | جدولة أسبوعية | fuzz موسّع 500,000 طفرة |

**Actions المستخدمة:** `actions/checkout@v4` و`actions/setup-python@v5` (Python 3.12) و`actions/setup-node@v4` (Node 24 مع cache npm).

**المضيف خارج CI عمداً:** بناؤه يحتاج مكتبات منصة (WebKitGTK على Linux)، ومنصة لم يُبنَ عليها المضيف لا يصح الإبلاغ عنها ناجحة. ما يُفحص له هو التنسيق فقط لأنه لا يحتاج مكتبات.

## 8. مصادر التحقق

رُوجعت بتاريخ 2026-10-06. الإصدارات والتراخيص وMSRV مقروءة من `cargo metadata` وملفات القفل؛ صحة المستودعات من GitHub API.

**Rust:**

- [Taffy repository](https://github.com/DioxusLabs/taffy) — ينفذ Block وFlexbox وGrid؛ 3,610 نجمة؛ نشط.
- [Parley repository](https://github.com/linebender/parley) — مكدس Fontique/HarfRust/Skrifa/ICU4X؛ 751 نجمة؛ نشط.
- [Vello repository](https://github.com/linebender/vello) — يصف `vello_cpu` كخيار CPU الناضج و`vello_gpu` لمسار GPU؛ 4,392 نجمة؛ نشط.
- [resvg repository](https://github.com/linebender/resvg) — انتقل ملكيته إلى linebender؛ 4,108 نجوم؛ نشط.
- [zip repository](https://github.com/zip-rs/zip2) — مبني على APPNOTE v6.3.9.
- [Tauri repository](https://github.com/tauri-apps/tauri) — 111,613 نجمة؛ نشط.
- [Tauri features](https://docs.rs/tauri/latest/tauri/) — يوثق `custom-protocol`: «Feature managed by the Tauri CLI. When enabled, Tauri assumes a production environment instead of a development one».
- [Tauri WebView versions](https://v2.tauri.app/reference/webview-versions/) — WebView2 مبني على Edge/Chromium ويتحدث ذاتياً؛ مدعوم على Windows 7+ ومثبت مسبقاً على Windows 11.
- [Tauri CLI reference](https://v2.tauri.app/reference/cli/) — `tauri dev` يستخدم `build.devUrl` ويشغّل `beforeDevCommand`.
- [Tauri dialog plugin](https://v2.tauri.app/plugin/dialog/) — حوارات أصلية؛ يتطلب Rust 1.90+.

**الواجهة:**

- [Vue + TypeScript](https://vuejs.org/guide/typescript/overview.html) — الأدوات الموصى بها لفحص SFC.
- [Vue Language Tools CHANGELOG](https://github.com/vuejs/language-tools/blob/master/CHANGELOG.md) و[issue 6124](https://github.com/vuejs/language-tools/issues/6124) — حالة توافق TypeScript 7.
- [Vite guide](https://vite.dev/guide/) — خادم التطوير والتغليف.
- [Tailwind CSS compatibility](https://tailwindcss.com/docs/compatibility) — متصفحات v4 الدنيا وعدم الاستخدام مع معالجات CSS مسبقة.
- [Vitest guide](https://vitest.dev/guide/) و[Vue Test Utils](https://test-utils.vuejs.org/).
- [Lucide LICENSE](https://raw.githubusercontent.com/lucide-icons/lucide/main/LICENSE) — ISC.
- [Reka UI repository](https://github.com/unovue/reka-ui) — مؤجل؛ 6,859 نجمة؛ نشط.

**البنية التحتية:**

- [cargo-deny](https://embarkstudios.github.io/cargo-deny/) — إعداد `deny.toml`.
- [GitHub Actions marketplace](https://github.com/marketplace?type=actions) — إصدارات actions المستخدمة.

### حدود هذا التحقق

- أرقام النجوم وتواريخ النشاط لقطة بتاريخ التحقق، ولا تثبت جودة أو استمرارية.
- ثلاثة مستودعات رفض الاستعلام أولاً (`resvg` و`skrifa` و`uuid`) وحُلّت بمتابعة التحويلات أو من `cargo metadata`؛ ما وُثق عنها مأخوذ من البيانات المحلية الموثوقة.
- MSRV المسجل هو ما تعلنه الحزم، ولا يعني أن المشروع أعلن سياسة MSRV.
- لم تُفحص الثغرات الأمنية يدوياً في هذه الجولة؛ `cargo deny check advisories` في CI هو الفحص المستمر.
