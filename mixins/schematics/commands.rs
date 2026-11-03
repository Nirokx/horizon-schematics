// The commands the chat box completes as the mod's.
fn schematics_commands(commands: &mut Vec<&'static str>) {
    crate::game::schematics_bridge::commands(commands);
}
