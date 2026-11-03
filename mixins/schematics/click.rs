// A click on the menu: the button under the pointer does its thing.
fn schematics_click(
    game: &crate::game::Game<'_>,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
    button: i32,
    canceled: &mut bool,
) {
    // A mod's screen has no container panel: its rect is the whole window.
    let size = [screen.rect[2], screen.rect[3]];
    crate::game::schematics_bridge::click(game, size, screen.mouse, button, canceled);
}
