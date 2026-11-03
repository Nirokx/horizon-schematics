//! Rotating and mirroring a placement: positions, and the block states that
//! face a way (`facing`, `axis`, `rotation`, rail shapes, connections...).
//! The mirror applies first, then the rotation, as vanilla's structure
//! templates do.

use crate::schematic::BlockState;

/// Clockwise quarter turns seen from above.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rotation {
    #[default]
    None,
    Cw90,
    Cw180,
    Ccw90,
}

impl Rotation {
    pub const ALL: [Rotation; 4] = [Rotation::None, Rotation::Cw90, Rotation::Cw180, Rotation::Ccw90];
    pub fn turns(self) -> u8 {
        match self {
            Rotation::None => 0,
            Rotation::Cw90 => 1,
            Rotation::Cw180 => 2,
            Rotation::Ccw90 => 3,
        }
    }
    pub fn from_turns(n: i32) -> Self {
        Self::ALL[n.rem_euclid(4) as usize]
    }
    pub fn cw(self) -> Self {
        Self::from_turns(i32::from(self.turns()) + 1)
    }
    pub fn ccw(self) -> Self {
        Self::from_turns(i32::from(self.turns()) + 3)
    }
    pub fn degrees(self) -> u32 {
        u32::from(self.turns()) * 90
    }
    pub fn name(self) -> &'static str {
        ["none", "cw_90", "cw_180", "ccw_90"][self.turns() as usize]
    }
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.name() == text || r.degrees().to_string() == text)
    }
}

/// `LEFT_RIGHT` flips z (north and south swap), `FRONT_BACK` flips x.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mirror {
    #[default]
    None,
    LeftRight,
    FrontBack,
}

impl Mirror {
    pub const ALL: [Mirror; 3] = [Mirror::None, Mirror::LeftRight, Mirror::FrontBack];
    pub fn next(self) -> Self {
        match self {
            Mirror::None => Mirror::FrontBack,
            Mirror::FrontBack => Mirror::LeftRight,
            Mirror::LeftRight => Mirror::None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Mirror::None => "None",
            Mirror::LeftRight => "Z (north-south)",
            Mirror::FrontBack => "X (east-west)",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Mirror::None => "none",
            Mirror::LeftRight => "left_right",
            Mirror::FrontBack => "front_back",
        }
    }
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Mirror::None),
            "left_right" | "z" => Some(Mirror::LeftRight),
            "front_back" | "x" => Some(Mirror::FrontBack),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Transform {
    pub rotation: Rotation,
    pub mirror: Mirror,
}

impl Transform {
    pub fn new(rotation: Rotation, mirror: Mirror) -> Self {
        Self { rotation, mirror }
    }
    pub fn is_identity(&self) -> bool {
        self.rotation == Rotation::None && self.mirror == Mirror::None
    }
    /// A position relative to the placement's origin, moved.
    pub fn apply(&self, p: [i32; 3]) -> [i32; 3] {
        let [mut x, y, mut z] = p;
        match self.mirror {
            Mirror::LeftRight => z = -z,
            Mirror::FrontBack => x = -x,
            Mirror::None => {}
        }
        match self.rotation {
            Rotation::None => [x, y, z],
            Rotation::Cw90 => [-z, y, x],
            Rotation::Cw180 => [-x, y, -z],
            Rotation::Ccw90 => [z, y, -x],
        }
    }
    /// What [`apply`](Self::apply) undoes.
    pub fn invert(&self, p: [i32; 3]) -> [i32; 3] {
        let [x, y, z] = p;
        let [mut x, y, mut z] = match self.rotation {
            Rotation::None => [x, y, z],
            Rotation::Cw90 => [z, y, -x],
            Rotation::Cw180 => [-x, y, -z],
            Rotation::Ccw90 => [-z, y, x],
        };
        match self.mirror {
            Mirror::LeftRight => z = -z,
            Mirror::FrontBack => x = -x,
            Mirror::None => {}
        }
        [x, y, z]
    }
    /// `apply` for a point inside the blocks (an entity's position), where
    /// block `p` covers `p..p+1`.
    pub fn apply_f(&self, p: [f64; 3]) -> [f64; 3] {
        let [mut x, y, mut z] = p;
        match self.mirror {
            Mirror::LeftRight => z = 1.0 - z,
            Mirror::FrontBack => x = 1.0 - x,
            Mirror::None => {}
        }
        match self.rotation {
            Rotation::None => [x, y, z],
            Rotation::Cw90 => [1.0 - z, y, x],
            Rotation::Cw180 => [1.0 - x, y, 1.0 - z],
            Rotation::Ccw90 => [z, y, 1.0 - x],
        }
    }
    /// An entity's yaw, turned with it.
    pub fn yaw(&self, yaw: f32) -> f32 {
        let mut y = yaw;
        match self.mirror {
            Mirror::LeftRight => y = 180.0 - y,
            Mirror::FrontBack => y = -y,
            Mirror::None => {}
        }
        (y + 90.0 * f32::from(self.rotation.turns())).rem_euclid(360.0)
    }
    pub fn direction(&self, d: &str) -> String {
        let mut d = d;
        match self.mirror {
            Mirror::LeftRight => {
                d = match d {
                    "north" => "south",
                    "south" => "north",
                    other => other,
                }
            }
            Mirror::FrontBack => {
                d = match d {
                    "east" => "west",
                    "west" => "east",
                    other => other,
                }
            }
            Mirror::None => {}
        }
        rotate_dir(d, self.rotation.turns()).to_owned()
    }
    /// `state` as it stands in the moved placement.
    pub fn state(&self, state: &BlockState) -> BlockState {
        if self.is_identity() || state.props.is_empty() {
            return state.clone();
        }
        let mut out = state.clone();
        let flips = self.mirror != Mirror::None;
        let odd = self.rotation.turns() % 2 == 1;
        let name = state.short();
        for (key, value) in &state.props {
            match key.as_str() {
                "facing" | "horizontal_facing" => out.set(key, &self.direction(value)),
                "axis" if odd => out.set(key, match value.as_str() {
                    "x" => "z",
                    "z" => "x",
                    other => other,
                }),
                "rotation" => {
                    if let Ok(r) = value.parse::<i32>() {
                        let r = match self.mirror {
                            Mirror::LeftRight => 8 - r,
                            Mirror::FrontBack => 16 - r,
                            Mirror::None => r,
                        };
                        out.set(key, &(r + 4 * i32::from(self.rotation.turns())).rem_euclid(16).to_string());
                    }
                }
                "shape" if name.ends_with("rail") => out.set(key, &self.rail(value)),
                "shape" if name.ends_with("_stairs") && flips => out.set(key, &swap_sides(value)),
                "hinge" if flips => out.set(key, if value == "left" { "right" } else { "left" }),
                "type" if flips && (name.ends_with("chest")) => out.set(key, &swap_sides(value)),
                "orientation" => {
                    if let Some((front, top)) = value.split_once('_') {
                        out.set(key, &format!("{}_{}", self.direction(front), self.direction(top)));
                    }
                }
                _ => {}
            }
        }
        // Sides keyed by direction: fences, walls, panes, wire, vines, lichen.
        let sides = ["north", "east", "south", "west"];
        if sides.iter().any(|s| state.get(s).is_some()) && !name.ends_with("_slab") {
            for side in sides {
                if let Some(value) = state.get(side) {
                    out.set(&self.direction(side), value);
                }
            }
        }
        out
    }
    fn rail(&self, shape: &str) -> String {
        if let Some(dir) = shape.strip_prefix("ascending_") {
            return format!("ascending_{}", self.direction(dir));
        }
        let Some((a, b)) = shape.split_once('_') else {
            return shape.to_owned();
        };
        let (a, b) = (self.direction(a), self.direction(b));
        // Rails name their ends north/south first, then east/west.
        let order = |d: &str| match d {
            "north" | "south" => 0,
            _ => 1,
        };
        if order(&a) == order(&b) {
            // A straight rail: north_south or east_west.
            return if order(&a) == 0 { "north_south".into() } else { "east_west".into() };
        }
        if order(&a) == 0 { format!("{a}_{b}") } else { format!("{b}_{a}") }
    }
}

fn rotate_dir(d: &str, turns: u8) -> &str {
    const ORDER: [&str; 4] = ["north", "east", "south", "west"];
    match ORDER.iter().position(|&o| o == d) {
        Some(i) => ORDER[(i + turns as usize) % 4],
        None => d,
    }
}

fn swap_sides(value: &str) -> String {
    if let Some(rest) = value.strip_suffix("_left") {
        format!("{rest}_right")
    } else if let Some(rest) = value.strip_suffix("_right") {
        format!("{rest}_left")
    } else {
        match value {
            "left" => "right".into(),
            "right" => "left".into(),
            other => other.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<Transform> {
        Mirror::ALL
            .into_iter()
            .flat_map(|m| Rotation::ALL.into_iter().map(move |r| Transform::new(r, m)))
            .collect()
    }

    #[test]
    fn invert_undoes_apply() {
        for t in all() {
            for p in [[1, 2, 3], [-4, 0, 7], [0, 0, 0]] {
                assert_eq!(t.invert(t.apply(p)), p, "{t:?}");
            }
        }
    }

    #[test]
    fn clockwise_turns_north_to_east() {
        let t = Transform::new(Rotation::Cw90, Mirror::None);
        assert_eq!(t.apply([0, 0, -1]), [1, 0, 0]);
        assert_eq!(t.direction("north"), "east");
        let s = BlockState::new("oak_log").with("axis", "x");
        assert_eq!(t.state(&s).get("axis"), Some("z"));
        let sign = BlockState::new("oak_sign").with("rotation", "0");
        assert_eq!(t.state(&sign).get("rotation"), Some("4"));
    }

    #[test]
    fn states_follow_their_positions() {
        // A block facing a neighbour still faces it after any transform.
        let dirs = [("north", [0, 0, -1]), ("east", [1, 0, 0]), ("south", [0, 0, 1]), ("west", [-1, 0, 0])];
        for t in all() {
            for (d, v) in dirs {
                let moved = t.apply(v);
                let expect = dirs.iter().find(|(_, w)| *w == moved).unwrap().0;
                assert_eq!(t.direction(d), expect, "{t:?} {d}");
            }
        }
    }

    #[test]
    fn rails_fences_and_mirrors() {
        let t = Transform::new(Rotation::Cw90, Mirror::None);
        let rail = BlockState::new("rail").with("shape", "north_east");
        assert_eq!(t.state(&rail).get("shape"), Some("south_east"));
        assert_eq!(t.state(&BlockState::new("rail").with("shape", "north_south")).get("shape"), Some("east_west"));
        let fence = BlockState::new("oak_fence").with("north", "true").with("east", "false").with("south", "false").with("west", "false");
        let moved = t.state(&fence);
        assert_eq!(moved.get("east"), Some("true"));
        assert_eq!(moved.get("north"), Some("false"));
        let m = Transform::new(Rotation::None, Mirror::FrontBack);
        let stairs = BlockState::new("oak_stairs").with("facing", "east").with("shape", "outer_left");
        let moved = m.state(&stairs);
        assert_eq!(moved.get("facing"), Some("west"));
        assert_eq!(moved.get("shape"), Some("outer_right"));
        let sign = BlockState::new("oak_sign").with("rotation", "4");
        assert_eq!(m.state(&sign).get("rotation"), Some("12"));
        assert_eq!(Transform::new(Rotation::None, Mirror::LeftRight).state(&sign).get("rotation"), Some("4"));
    }

    #[test]
    fn entity_points_stay_inside_their_blocks() {
        for t in all() {
            let p = t.apply_f([0.25, 1.0, 0.75]);
            let b = t.apply([0, 1, 0]);
            assert!((p[0] - f64::from(b[0])).abs() <= 1.0 && p[0] >= f64::from(b[0].min(0)) - 1.0, "{t:?} {p:?}");
        }
    }
}
