use sha2::{Digest, Sha256};

use crate::world::{SettlementKind, World};

pub fn state_hash(world: &World) -> String {
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
