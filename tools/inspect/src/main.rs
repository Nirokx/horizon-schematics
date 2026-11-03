use schematics_engine::formats;
use std::path::Path;

fn main() {
    let mut failed = false;
    for arg in std::env::args().skip(1) {
        let path = Path::new(&arg);
        match formats::load(path) {
            Ok(s) => {
                let size = s.size();
                println!(
                    "{}: {} '{}' by '{}', {}x{}x{}, {} blocks, {} regions",
                    path.display(),
                    s.format.label(),
                    s.name,
                    s.author,
                    size[0],
                    size[1],
                    size[2],
                    s.block_count(),
                    s.regions.len()
                );
                for r in &s.regions {
                    let mut states: Vec<String> = r.palette.iter().map(|p| p.to_string()).collect();
                    states.sort();
                    for st in states {
                        println!("    {st}");
                    }
                    for (p, be) in &r.block_entities {
                        println!("    block entity at {p:?}: {}", schematics_engine::snbt::to_snbt(&schematics_engine::nbt::Tag::Compound(be.clone())));
                    }
                }
            }
            Err(e) => {
                failed = true;
                println!("{}: ERROR {e}", path.display());
            }
        }
    }
    std::process::exit(i32::from(failed));
}
