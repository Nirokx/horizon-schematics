// A stick's left and right clicks set the area selection's corners.
fn schematics_tool(game: &mut crate::game::Game<'_>, button: i32, pressed: bool, canceled: &mut bool) {
    crate::game::schematics_bridge::mouse(game, button, pressed, canceled);
}
