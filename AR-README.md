<!-- يا ليل لازم اكتب HTML -->
<div dir="rtl">

# بوالدرايش
بوالدرايش هو مولف (compositor) شخصي مكتوب بلغة رست

# الفلسفة العامة
المولف هذا مصنوع لي انا بالتحديد، فأغلب المزايا هي متركزة على طريقة عملي بشكل كبير، لكن اذا كانت عندك اقتراحات او تبي تساهم حياك، ممكن تفتح issue او PR</p>

رغم هذا، بوالدرايش قد يكون اساس مب شين اذا تبي تستعمله مثل [dwm/dwl](https://github.com/djpohly/dwl) بإنك تعدل عليه بالترقيع

# البناء

عشان تبني المشروع تقدر تستعمل cargo

<div dir="ltr">

```bash
# debug build
cargo build

# release build (وقت ما تبي تستعمله ابنيه كذا)
cargo build --release
```
</div>

# التشغيل

ملف التنفيذ المجمع في النهاية هو بإسم `buddaraysh`

بعدها يمديك تضيفه لمدير العرض/مدير التسجيل حقك

<div dir="ltr">

```
[Desktop Entry]
Name=Buddaraysh
Comment=Buddaraysh
Exec=path/to/buddaraysh/buddaraysh
TryExec=path/to/buddaraysh/buddaraysh
Type=Application
```
</div>

بعدها افتح مدير التسجيل و شغل بوالدرايش

#### أو تقدر بكل بساطة تشغله من الـtty لأن wayland ابسط من x11 من الناحية هذي

<br/>

# متغيرات البيئة (Environment variables)

| المتغير                    | وصف                               | مثال                | القيمة الافتراضية             |
| -------------------------- | --------------------------------- | ------------------- | ----------------------------- |
| XCURSOR_THEME              | تحديد سمة المؤشر                  | Adwaita             | "default"                     |
| XCURSOR_SIZE               | تحديد حجم المؤشر                  | 32                  | 24                            |
| BUD_DRM_DEVICE             | تحديد جهاز الـDRM                 | /dev/dri/renderD128 | كرت الشاشة الرئيسي            |
| BUD_NO_VULKAN              | ابطال vulkan                      | yes/1/true/y        | تفعيل vulkan                  |
| BUD_LOG                    | تحديد مستوى التسجيل               | trace/info/debug    | مستوى debug                   |
| BUD_BACKEND                | تحديد الواجهة الخلفية لبوالدرايش  | winit/udev          | udev                          |
| BUD_DISABLE_DRM_COMPOSITOR | ابطال مولف الـDRM                 | yes/1/true/y        | تفعيل الـDRM                  |


ملاحظة: اذا فيه مصطلحات عربية اقدر استخدمها علمني (سواء عن طريق issue او PR)، لأني مهتم اخلي اللغة العربية لها استخدام في الامور التقنية مثل هذي
</div>
