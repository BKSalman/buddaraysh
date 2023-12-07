<p align='right' dir="rtl"><sub>
  عربي؟ <a href='AR-README.md' title='Arabic README'>كمل قراءة بالعربي (لهجة سعودية)</a>.
</sub></p>

# Buddaraysh | بو الدرايش

Buddaraysh is a personal [Wayland](https://wiki.archlinux.org/title/wayland) compositor written in Rust

# General philosophy
This window manager is specifically made for me,
so features are mostly gonna be focused on my own workflow, but if you have good suggestions,
or would just like to contribute, you can open an issue or a PR.

however it could be a not too bad base for a patched compositor like [dwm/dwl](https://github.com/djpohly/dwl)

# Build

to build the window manager you can just build it with cargo

```bash
# debug build
cargo build

# release build (when you want to actually use it)
cargo build --release
```

# Run

the compiled binary name is `buddaraysh`

so you can add the following entry to your display manager/login manager

```
[Desktop Entry]
Name=Buddaraysh
Comment=Buddaraysh
Exec=path/to/buddaraysh/buddaraysh
TryExec=path/to/buddaraysh/buddaraysh
Type=Application
```

then open your display manager and run Buddaraysh

#### or you can manually just launch it from tty since wayland is much simpler than x11 in that sense
