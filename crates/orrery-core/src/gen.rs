use rand::Rng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Biome {
    Ocean,
    Coast,
    Plains,
    Forest,
    Hills,
    Mountains,
    Desert,
    Swamp,
    River,
}

impl Biome {
    pub fn hospitable(self) -> bool {
        matches!(
            self,
            Biome::Plains | Biome::Forest | Biome::Hills | Biome::Coast | Biome::River | Biome::Swamp
        )
    }

    pub fn food_mult(self) -> f32 {
        match self {
            Biome::Plains | Biome::River => 1.3,
            Biome::Forest | Biome::Coast => 1.1,
            Biome::Hills => 0.9,
            Biome::Swamp => 0.7,
            Biome::Desert => 0.4,
            Biome::Mountains => 0.3,
            Biome::Ocean => 0.0,
        }
    }

    pub fn glyph(self) -> char {
        match self {
            Biome::Ocean => '~',
            Biome::Coast => '.',
            Biome::Plains => ',',
            Biome::Forest => 'T',
            Biome::Hills => 'n',
            Biome::Mountains => '^',
            Biome::Desert => ':',
            Biome::Swamp => ';',
            Biome::River => '=',
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Map {
    pub width: u32,
    pub height: u32,
    pub heightmap: Vec<f32>,
    pub biomes: Vec<Biome>,
}

impl Map {
    pub fn idx(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height
    }

    pub fn biome(&self, x: u32, y: u32) -> Biome {
        self.biomes[self.idx(x, y)]
    }

    pub fn set_biome(&mut self, x: u32, y: u32, b: Biome) {
        let i = self.idx(x, y);
        self.biomes[i] = b;
    }
}

/// Value noise-ish height from seeded hash. Deterministic, no external noise crate.
fn hash_noise(seed: u64, x: i32, y: i32) -> f32 {
    let mut n = seed
        .wrapping_add(x as u64 * 374761393)
        .wrapping_add(y as u64 * 668265263);
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n ^= n >> 16;
    (n & 0xFFFF) as f32 / 65535.0
}

fn smooth_noise(seed: u64, x: f32, y: f32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let n00 = hash_noise(seed, x0, y0);
    let n10 = hash_noise(seed, x0 + 1, y0);
    let n01 = hash_noise(seed, x0, y0 + 1);
    let n11 = hash_noise(seed, x0 + 1, y0 + 1);
    let nx0 = n00 * (1.0 - sx) + n10 * sx;
    let nx1 = n01 * (1.0 - sx) + n11 * sx;
    nx0 * (1.0 - sy) + nx1 * sy
}

fn fbm(seed: u64, x: f32, y: f32) -> f32 {
    let mut v = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    for octave in 0..5 {
        v += amp * smooth_noise(seed.wrapping_add(octave * 17), x * freq, y * freq);
        amp *= 0.5;
        freq *= 2.0;
    }
    v
}

pub fn generate_map(rng: &mut ChaCha8Rng, width: u32, height: u32) -> Map {
    let seed = rng.gen::<u64>();
    let mut heightmap = vec![0.0f32; (width * height) as usize];
    let cx = (width as f32 - 1.0) * 0.5;
    let cy = (height as f32 - 1.0) * 0.5;
    let max_r = (cx.hypot(cy)).max(1.0);

    for y in 0..height {
        for x in 0..width {
            let nx = x as f32 / width as f32 * 4.0;
            let ny = y as f32 / height as f32 * 4.0;
            let mut h = fbm(seed, nx, ny);
            // Island-ish falloff
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let fall = (dx.hypot(dy) / max_r).clamp(0.0, 1.0);
            h *= 1.0 - fall.powf(1.8) * 0.85;
            heightmap[(y * width + x) as usize] = h;
        }
    }

    let mut biomes = vec![Biome::Ocean; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) as usize;
            let h = heightmap[i];
            let lat = (y as f32 / height as f32 - 0.5).abs();
            let b = if h < 0.28 {
                Biome::Ocean
            } else if h < 0.32 {
                Biome::Coast
            } else if h > 0.72 {
                Biome::Mountains
            } else if h > 0.58 {
                Biome::Hills
            } else if lat > 0.35 && h < 0.42 {
                Biome::Desert
            } else if h < 0.38 && lat < 0.2 {
                Biome::Swamp
            } else if hash_noise(seed ^ 0xABC, x as i32, y as i32) > 0.55 {
                Biome::Forest
            } else {
                Biome::Plains
            };
            biomes[i] = b;
        }
    }

    let mut map = Map {
        width,
        height,
        heightmap,
        biomes,
    };
    carve_rivers(rng, &mut map, 6);
    map
}

fn carve_rivers(rng: &mut ChaCha8Rng, map: &mut Map, count: usize) {
    let w = map.width as i32;
    let h = map.height as i32;
    for _ in 0..count {
        // Start near a mountain/hill
        let mut x = rng.gen_range(1..w - 1);
        let mut y = rng.gen_range(1..h - 1);
        let mut tries = 0;
        while map.biome(x as u32, y as u32) != Biome::Mountains
            && map.biome(x as u32, y as u32) != Biome::Hills
            && tries < 40
        {
            x = rng.gen_range(1..w - 1);
            y = rng.gen_range(1..h - 1);
            tries += 1;
        }

        for _ in 0..map.width.max(map.height) {
            let biome = map.biome(x as u32, y as u32);
            if biome == Biome::Ocean {
                break;
            }
            if biome != Biome::Mountains {
                map.set_biome(x as u32, y as u32, Biome::River);
            }
            // Step downhill
            let mut best = (x, y);
            let mut best_h = map.heightmap[map.idx(x as u32, y as u32)];
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let nx = x + dx;
                let ny = y + dy;
                if !map.in_bounds(nx, ny) {
                    continue;
                }
                let nh = map.heightmap[map.idx(nx as u32, ny as u32)];
                if nh < best_h {
                    best_h = nh;
                    best = (nx, ny);
                }
            }
            if best == (x, y) {
                break;
            }
            x = best.0;
            y = best.1;
        }
    }
}

pub fn land_tiles(map: &Map) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for y in 0..map.height {
        for x in 0..map.width {
            if map.biome(x, y).hospitable() {
                out.push((x, y));
            }
        }
    }
    out
}
