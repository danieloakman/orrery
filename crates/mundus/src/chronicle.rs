use crate::event::Event;
use crate::world::World;

pub fn render_chronicle_md(world: &World) -> String {
    let mut out = String::new();
    out.push_str("# Mundus Chronicle\n\n");
    out.push_str(&format!(
        "_seed {} · {}×{} · 1 tick = 1 minute · year {}_\n\n",
        world.config.seed,
        world.config.width,
        world.config.height,
        world.clock.year()
    ));
    for ev in &world.events {
        out.push_str(&format_event(ev));
        out.push('\n');
    }
    out
}

fn format_event(ev: &Event) -> String {
    format!(
        "**Year {}** (day {}): {}",
        ev.year,
        ev.day,
        ev.kind.summary()
    )
}
