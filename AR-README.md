<!-- يا ليل لازم اكتب HTML -->
<div dir="rtl">

# بو الدرايش
بو الدرايش هو مولف (compositor) شخصي مكتوب بلغة رست

# الفلسفة العامة
المولف هذا مصنوع لي انا بالتحديد، فأغلب المزايا هي متركزة على طريقة عملي بشكل كبير، لكن اذا كانت عندك اقتراحات او تبي تساهم حياك، ممكن تفتح issue او PR</p>

رغم هذا، بو الدرايش قد يكون اساس مب شين اذا تبي تستعمله مثل [dwm/dwl](https://github.com/djpohly/dwl) بإنك تعدل عليه بالترقيع

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

بعدها افتح مدير التسجيل و شغل بو الدرايش

#### أو تقدر بكل بساطة تشغله من الـtty لأن wayland ابسط من x11 من الناحية هذي

<br/>
ملاحظة: اذا فيه مصطلحات عربية اقدر استخدمها علمني (سواء عن طريق issue او PR)، لأني مهتم اخلي اللغة العربية لها استخدام في الامور التقنية مثل هذي

</div>
