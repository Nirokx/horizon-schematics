// The world is drawn: the placements' ghosts and boxes go over it.
fn schematics_drawn(world: &mut crate::gfx::native_render::WorldDrawn<'_>) {
    crate::schematics_ghost::draw(world);
}
