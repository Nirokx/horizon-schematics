// The mod's row in Options > Mods, with its keys.
fn schematics_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use crate::game::schematics_bridge as m;
    mods.push(crate::game::native_hooks::NativeMod {
        id: m::ID.into(),
        title: format!("{} {}", m::NAME, m::VERSION),
        description: m::DESCRIPTION.into(),
        keys: vec![
            [m::KEY_MENU.into(), "Schematics menu".into(), m::DEFAULT_MENU.into()],
            [m::KEY_LAYER_UP.into(), "Render layer up".into(), m::DEFAULT_LAYER_UP.into()],
            [m::KEY_LAYER_DOWN.into(), "Render layer down".into(), m::DEFAULT_LAYER_DOWN.into()],
        ],
    });
}
