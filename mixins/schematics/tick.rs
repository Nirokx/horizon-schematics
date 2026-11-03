// Each tick of the window: a paste on a server sends its commands.
fn schematics_tick(game: &mut crate::game::Game<'_>) {
    crate::game::schematics_bridge::client_tick(game);
}
