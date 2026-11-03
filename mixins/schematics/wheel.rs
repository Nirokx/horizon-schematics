// Ctrl and the wheel with a stick nudge the selected placement.
fn schematics_wheel(game: &mut crate::game::Game<'_>, accumulated: [i32; 2], modifiers: u32, canceled: &mut bool) {
    crate::game::schematics_bridge::scrolling(game, accumulated, modifiers, canceled);
}
