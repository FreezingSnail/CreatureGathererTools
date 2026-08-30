use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::rle;

fn default_version() -> u32 {
    1
}

fn default_slot_size() -> u32 {
    2
}

fn default_weight() -> f32 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WfcProject {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_slot_size")]
    pub slot_w: u32,
    #[serde(default = "default_slot_size")]
    pub slot_h: u32,
    #[serde(default)]
    pub pieces: Vec<WfcPiece>,
    #[serde(default)]
    pub socket_labels: Vec<SocketLabel>,
    #[serde(default)]
    pub recipes: Vec<WfcRecipe>,
}

impl Default for WfcProject {
    fn default() -> Self {
        Self {
            version: default_version(),
            slot_w: default_slot_size(),
            slot_h: default_slot_size(),
            pieces: Vec::new(),
            socket_labels: Vec::new(),
            recipes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WfcPiece {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tw: u32,
    #[serde(default)]
    pub th: u32,
    #[serde(default)]
    pub tiles: String,
    #[serde(default = "default_weight")]
    pub weight: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(default)]
    pub edge_overrides: Vec<EdgeSocket>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    N,
    E,
    S,
    W,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EdgeSocket {
    pub side: Side,
    #[serde(default)]
    pub index: u32,
    #[serde(default)]
    pub label: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocketLabel {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: SocketKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SocketKind {
    #[default]
    Symmetric,
    Directed {
        partner: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WfcRecipe {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub piece_ids: Vec<u32>,
    #[serde(default)]
    pub size_w: u32,
    #[serde(default)]
    pub size_h: u32,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub anchors: Vec<Anchor>,
    #[serde(default)]
    pub socket_compat: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    #[serde(default)]
    pub piece_id: u32,
    #[serde(default)]
    pub sx: u32,
    #[serde(default)]
    pub sy: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stamp {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub w: u32,
    #[serde(default)]
    pub h: u32,
    #[serde(default)]
    pub tiles: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<StampOrigin>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StampOrigin {
    #[serde(default)]
    pub recipe_id: u32,
    #[serde(default)]
    pub seed: u64,
    #[serde(default)]
    pub slot_w: u32,
    #[serde(default)]
    pub slot_h: u32,
}

impl WfcProject {
    /// Checks all persisted relationships so callers can surface every problem together.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let piece_ids = collect_ids(
            "piece",
            self.pieces.iter().map(|piece| piece.id),
            &mut errors,
        );
        let label_ids = collect_ids(
            "socket label",
            self.socket_labels.iter().map(|label| label.id),
            &mut errors,
        );
        let _recipe_ids = collect_ids(
            "recipe",
            self.recipes.iter().map(|recipe| recipe.id),
            &mut errors,
        );

        if self.slot_w == 0 {
            errors.push("slot_w must be greater than zero".to_string());
        }
        if self.slot_h == 0 {
            errors.push("slot_h must be greater than zero".to_string());
        }

        for piece in &self.pieces {
            if piece.weight.partial_cmp(&0.0) != Some(Ordering::Greater) {
                errors.push(format!("piece {} has non-positive weight", piece.id));
            }
            if piece.tw == 0 || piece.th == 0 {
                errors.push(format!("piece {} has zero tile dimensions", piece.id));
            }
            if self.slot_w != 0 && piece.tw % self.slot_w != 0 {
                errors.push(format!(
                    "piece {} width is not a multiple of slot_w",
                    piece.id
                ));
            }
            if self.slot_h != 0 && piece.th % self.slot_h != 0 {
                errors.push(format!(
                    "piece {} height is not a multiple of slot_h",
                    piece.id
                ));
            }
            match (piece.tw as usize).checked_mul(piece.th as usize) {
                Some(expected) => {
                    if let Err(error) = rle::decode(&piece.tiles, expected) {
                        errors.push(format!("piece {} has invalid tiles: {error}", piece.id));
                    }
                }
                None => errors.push(format!("piece {} tile dimensions overflow", piece.id)),
            }

            let horizontal_slots = piece.tw.checked_div(self.slot_w).unwrap_or(0);
            let vertical_slots = piece.th.checked_div(self.slot_h).unwrap_or(0);
            for edge in &piece.edge_overrides {
                if !label_ids.contains(&edge.label) {
                    errors.push(format!(
                        "piece {} edge override references unknown socket label {}",
                        piece.id, edge.label
                    ));
                }
                let edge_len = match edge.side {
                    Side::N | Side::S => horizontal_slots,
                    Side::E | Side::W => vertical_slots,
                };
                if edge.index >= edge_len {
                    errors.push(format!(
                        "piece {} edge override index is out of range",
                        piece.id
                    ));
                }
            }
        }

        for label in &self.socket_labels {
            if let SocketKind::Directed { partner } = label.kind
                && !label_ids.contains(&partner)
            {
                errors.push(format!(
                    "directed socket label {} references unknown partner {}",
                    label.id, partner
                ));
            }
        }

        let pieces_by_id: HashMap<u32, &WfcPiece> =
            self.pieces.iter().map(|piece| (piece.id, piece)).collect();
        for recipe in &self.recipes {
            for (left, right) in &recipe.socket_compat {
                for label in [left, right] {
                    if !label_ids.contains(label) {
                        errors.push(format!(
                            "recipe {} socket compatibility references unknown socket label {}",
                            recipe.id, label
                        ));
                    }
                }
            }
            for piece_id in &recipe.piece_ids {
                if !piece_ids.contains(piece_id) {
                    errors.push(format!(
                        "recipe {} references unknown piece {}",
                        recipe.id, piece_id
                    ));
                }
            }

            let max_w = 256u32.checked_div(self.slot_w).unwrap_or(0);
            let max_h = 256u32.checked_div(self.slot_h).unwrap_or(0);
            if recipe.size_w == 0 || recipe.size_w > max_w {
                errors.push(format!("recipe {} has invalid slot width", recipe.id));
            }
            if recipe.size_h == 0 || recipe.size_h > max_h {
                errors.push(format!("recipe {} has invalid slot height", recipe.id));
            }

            let mut occupied = HashSet::new();
            for anchor in &recipe.anchors {
                let Some(piece) = pieces_by_id.get(&anchor.piece_id) else {
                    errors.push(format!(
                        "recipe {} anchor references unknown piece {}",
                        recipe.id, anchor.piece_id
                    ));
                    continue;
                };
                if self.slot_w == 0 || self.slot_h == 0 {
                    continue;
                }
                let width = piece.tw / self.slot_w;
                let height = piece.th / self.slot_h;
                let fits = anchor
                    .sx
                    .checked_add(width)
                    .is_some_and(|end| end <= recipe.size_w)
                    && anchor
                        .sy
                        .checked_add(height)
                        .is_some_and(|end| end <= recipe.size_h);
                if !fits {
                    errors.push(format!("recipe {} anchor does not fit", recipe.id));
                    continue;
                }
                for y in anchor.sy..anchor.sy + height {
                    for x in anchor.sx..anchor.sx + width {
                        if !occupied.insert((x, y)) {
                            errors.push(format!(
                                "recipe {} anchors overlap slot ({x}, {y})",
                                recipe.id
                            ));
                        }
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

fn collect_ids(
    kind: &str,
    ids: impl Iterator<Item = u32>,
    errors: &mut Vec<String>,
) -> HashSet<u32> {
    let mut known = HashSet::new();
    for id in ids {
        if !known.insert(id) {
            errors.push(format!("duplicate {kind} id {id}"));
        }
    }
    known
}
