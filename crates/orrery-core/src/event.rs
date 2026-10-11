use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub tick: u64,
    pub day: u64,
    pub year: u64,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    WorldBorn { civs: u8 },
    Founded { civ: String, x: u32, y: u32 },
    Expanded { civ: String, x: u32, y: u32 },
    War {
        attacker: String,
        defender: String,
        victor: String,
    },
    Fell { civ: String, cause: String },
    Ruin { civ: String, x: u32, y: u32 },
    Myth { civ: String, tale: String },
}

impl EventKind {
    pub fn summary(&self) -> String {
        match self {
            EventKind::WorldBorn { civs } => format!("The age began with {civs} peoples."),
            EventKind::Founded { civ, x, y } => {
                format!("The {civ} founded a settlement at ({x},{y}).")
            }
            EventKind::Expanded { civ, x, y } => {
                format!("The {civ} expanded to ({x},{y}).")
            }
            EventKind::War {
                attacker,
                defender,
                victor,
            } => format!("War between the {attacker} and the {defender}; the {victor} prevailed."),
            EventKind::Fell { civ, cause } => format!("The {civ} fell ({cause})."),
            EventKind::Ruin { civ, x, y } => {
                format!("A ruin of the {civ} remains at ({x},{y}).")
            }
            EventKind::Myth { civ, tale } => format!("Among the {civ}: {tale}"),
        }
    }
}
