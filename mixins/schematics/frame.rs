// Once a frame: the placements of the world being played, their ghost
// meshes, what the menu and commands asked for.
fn schematics_frame(game: &mut crate::game::Game<'_>) {
    crate::game::schematics_bridge::frame(game);
}
