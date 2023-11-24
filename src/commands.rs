pub enum Command<'a> {
    SwitchVT(i32),
    Spawn(&'a str),
    Quit,
    None,
}
