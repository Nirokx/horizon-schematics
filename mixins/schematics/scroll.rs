// The wheel over the menu scrolls its list.
fn schematics_scroll(game: &crate::game::Game<'_>, scroll: [f64; 2], canceled: &mut bool) {
    crate::game::schematics_bridge::scroll(game, scroll, canceled);
}
