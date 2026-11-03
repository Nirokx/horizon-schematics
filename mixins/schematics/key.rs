// While playing: M opens the menu, Page Up and Page Down move the layer.
fn schematics_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    crate::game::schematics_bridge::key(game, key, pressed);
}
