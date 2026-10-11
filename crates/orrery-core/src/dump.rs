use sha2::{Digest, Sha256};

use crate::chronicle::render_chronicle_md;
use crate::world::{SettlementKind, World};

pub struct DumpArtifacts {
    pub chronicle_md: String,
    pub map_txt: String,
    pub state_json: String,
    pub state_hash: String,
}

pub fn build_dump(world: &World) -> Result<DumpArtifacts, serde_json::Error> {
    let chronicle_md = render_chronicle_md(world);
    let map_txt = render_map_ascii(world);
    let state_json = serde_json::to_string_pretty(world)?;
    let state_hash = state_hash(world);
    Ok(DumpArtifacts {
        chronicle_md,
        map_txt,
        state_json,
        state_hash,
    })
}

pub fn state_hash(world: &World) -> String {
    // Hash canonical JSON of world (rng is skipped in serde).
    let bytes = serde_json::to_vec(world).expect("serialize world");
    let digest = Sha256::digest(&bytes);
    hex::encode(digest)
}

pub fn render_map_ascii(world: &World) -> String {
    let w = world.map.width;
    let h = world.map.height;
    let mut grid: Vec<char> = world.map.biomes.iter().map(|b| b.glyph()).collect();

    // Overlay settlements: capital '@', village 'o', ruin 'x'
    // Stable order by civ id
    let mut civs: Vec<_> = world.civs.iter().collect();
    civs.sort_by_key(|c| c.id);
    for civ in civs {
        for s in &civ.settlements {
            let i = (s.y * w + s.x) as usize;
            grid[i] = match s.kind {
                SettlementKind::Capital => {
                    if civ.alive {
                        '@'
                    } else {
                        'X'
                    }
                }
                SettlementKind::Village => {
                    if civ.alive {
                        'o'
                    } else {
                        'x'
                    }
                }
                SettlementKind::Ruin => 'x',
            };
        }
    }

    let mut out = String::new();
    out.push_str(&format!(
        "# map {w}x{h}  ~ocean  ,@capital  o village  x ruin\n"
    ));
    for y in 0..h {
        for x in 0..w {
            out.push(grid[(y * w + x) as usize]);
        }
        out.push('\n');
    }
    out
}
