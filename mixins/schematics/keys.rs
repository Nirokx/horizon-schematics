// Keys over the menu: the name field, and M closes it.
fn schematics_keys(game: &crate::game::Game<'_>, key: &str, canceled: &mut bool) {
    crate::game::schematics_bridge::screen_key(game, key, canceled);
}
