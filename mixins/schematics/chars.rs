// Characters typed into the menu's name field.
fn schematics_chars(game: &crate::game::Game<'_>, c: char, canceled: &mut bool) {
    crate::game::schematics_bridge::typed(game, c, canceled);
}
