// The selected placement, what is left of it and the paste, top left.
fn schematics_hud(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    corners: &mut horizon_client::extras_hud::Corners,
) {
    crate::game::schematics_bridge::hud(game, batch, corners);
}
