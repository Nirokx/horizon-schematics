// Draws the mod's own screen, the menu.
fn schematics_screen(
    game: &crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    screen: &crate::game::native_hooks::NativeScreenView<'_>,
) {
    crate::game::schematics_bridge::screen(game, batch, screen.mouse);
}
