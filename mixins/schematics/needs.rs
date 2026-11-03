// While there are ghosts to draw, the frame keeps the world's depth.
fn schematics_needs(needs: &mut crate::gfx::native_render::RenderNeeds) {
    crate::schematics_ghost::needs(needs);
}
