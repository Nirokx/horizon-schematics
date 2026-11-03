// /schematics (or /sch) and its subcommands.
fn schematics_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    crate::game::schematics_bridge::command(game, command, ran);
}
