// After each level tick of the single player server: a paste or a save.
fn schematics_level(game: &mut crate::game::Game<'_>, dimension: &str) {
    crate::game::schematics_bridge::server_tick(game, dimension);
}
