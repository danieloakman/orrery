use rand::Rng;

const PREFIXES: &[&str] = &[
    "Kael", "Tor", "Ven", "Mir", "Ash", "Lor", "Dun", "Bel", "Rin", "Thal", "Cor", "Fen", "Yar",
    "Sul", "Bren",
];
const MIDDLES: &[&str] = &["a", "e", "i", "o", "u", "ar", "en", "or", "ul", "an"];
const SUFFIXES: &[&str] = &[
    "folk", "heim", "mere", "vale", "march", "hold", "reach", "moor", "dell", "ford",
];

pub fn culture_name(rng: &mut impl Rng) -> String {
    let p = PREFIXES[rng.gen_range(0..PREFIXES.len())];
    let m = MIDDLES[rng.gen_range(0..MIDDLES.len())];
    let s = SUFFIXES[rng.gen_range(0..SUFFIXES.len())];
    // Occasionally short names
    if rng.gen_bool(0.35) {
        format!("{p}{s}")
    } else {
        format!("{p}{m}{s}")
    }
}
