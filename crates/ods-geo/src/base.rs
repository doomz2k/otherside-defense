//! The chapterhouse: a 6x6 grid of facilities, X-COM style. The layout will
//! eventually double as the base-defense battle map, so position matters —
//! don't collapse this to a list of counts.

use crate::geography::Region;

pub const GRID: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Facility {
    /// The way in and out. Every chapterhouse has exactly one.
    Gatehouse,
    /// Houses 10 personnel each.
    Quarters,
    /// Watches for reality thinning in this region.
    AugurArray,
    /// Hosts 5 occultists' research each.
    Library,
    /// Doubles the pace of wound recovery (any number, effect is flat).
    Infirmary,
    /// Hosts 5 artificers' production each.
    Workshop,
    /// Candles, psalms, and quiet: where broken minds are mended.
    Chapel,
    /// Warded meditation cells: garrisoned soldiers slowly steel their nerve.
    Sanctum,
    /// A drill yard: garrisoned soldiers train toward the chosen focus.
    TrainingGround,
    /// Pre-chalked ward lines for the day the gate comes down.
    WardTower,
    /// Blessed hounds — they fight for the halls when the halls are hit.
    Kennel,
    /// Warded storage: salvage survives a Reckoning's looting.
    Vault,
    /// A 2x2 berth for one consecrated airship. Each hangar is one Zeppelin,
    /// and a sortie flies from a base only if it has a free one.
    Hangar,
}

impl Facility {
    pub const BUILDABLE: [Facility; 12] = [
        Facility::Quarters,
        Facility::Hangar,
        Facility::AugurArray,
        Facility::Library,
        Facility::Infirmary,
        Facility::Workshop,
        Facility::Chapel,
        Facility::Sanctum,
        Facility::TrainingGround,
        Facility::WardTower,
        Facility::Kennel,
        Facility::Vault,
    ];

    /// Cells wide × tall on the floor plan. Only the hangar spans 2×2.
    pub fn footprint(self) -> (usize, usize) {
        match self {
            Facility::Hangar => (2, 2),
            _ => (1, 1),
        }
    }

    pub fn cost(self) -> i64 {
        match self {
            Facility::Gatehouse => 0,
            Facility::Quarters => 100,
            Facility::AugurArray => 150,
            Facility::Library => 150,
            Facility::Infirmary => 200,
            Facility::Workshop => 150,
            Facility::Chapel => 180,
            Facility::Sanctum => 220,
            Facility::TrainingGround => 160,
            Facility::WardTower => 140,
            Facility::Kennel => 170,
            Facility::Vault => 190,
            Facility::Hangar => 300,
        }
    }

    pub fn build_days(self) -> u32 {
        match self {
            Facility::Gatehouse => 0,
            Facility::Quarters => 8,
            Facility::AugurArray => 10,
            Facility::Library => 10,
            Facility::Infirmary => 12,
            Facility::Workshop => 10,
            Facility::Chapel => 12,
            Facility::Sanctum => 14,
            Facility::TrainingGround => 10,
            Facility::WardTower => 8,
            Facility::Kennel => 10,
            Facility::Vault => 12,
            Facility::Hangar => 16,
        }
    }

    pub fn maintenance(self) -> i64 {
        match self {
            Facility::Gatehouse => 20,
            Facility::Quarters => 5,
            Facility::AugurArray => 10,
            Facility::Library => 10,
            Facility::Infirmary => 15,
            Facility::Workshop => 10,
            Facility::Chapel => 8,
            Facility::Sanctum => 10,
            Facility::TrainingGround => 8,
            Facility::WardTower => 6,
            Facility::Kennel => 10,
            Facility::Vault => 8,
            Facility::Hangar => 18,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Facility::Gatehouse => "Gatehouse",
            Facility::Quarters => "Quarters",
            Facility::AugurArray => "Augur Array",
            Facility::Library => "Library",
            Facility::Infirmary => "Infirmary",
            Facility::Workshop => "Workshop",
            Facility::Chapel => "Chapel",
            Facility::Sanctum => "Sanctum",
            Facility::TrainingGround => "Training Ground",
            Facility::WardTower => "Ward Tower",
            Facility::Kennel => "Kennel",
            Facility::Vault => "Vault",
            Facility::Hangar => "Hangar",
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Slot {
    facility: Facility,
    days_left: u32,
    /// For multi-cell facilities: `None` marks the top-left origin cell that
    /// carries the real build state; `Some((ax, ay))` marks a satellite cell
    /// pointing back at its origin. Single-cell facilities are all origins.
    #[serde(default)]
    anchor: Option<(u8, u8)>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Chapterhouse {
    pub region: Region,
    grid: [[Option<Slot>; GRID]; GRID],
    /// What this house's drill yard drills (each house sets its own).
    #[serde(default)]
    pub focus: crate::campaign::Focus,
    /// Scholars posted to THIS house: they research only at its lecterns.
    #[serde(default)]
    pub occultists: u32,
    /// Smiths posted to THIS house: they forge only at its benches.
    #[serde(default)]
    pub artificers: u32,
}

impl Chapterhouse {
    /// A founding chapterhouse: gatehouse, quarters, one augur, one library.
    pub fn founding(region: Region) -> Self {
        let mut ch = Self {
            region,
            grid: Default::default(),
            focus: crate::campaign::Focus::default(),
            occultists: 0,
            artificers: 0,
        };
        for (x, y, f) in [
            (2, 2, Facility::Gatehouse),
            (2, 3, Facility::Quarters),
            (3, 2, Facility::AugurArray),
            (3, 3, Facility::Library),
        ] {
            ch.grid[y][x] = Some(Slot { facility: f, days_left: 0, anchor: None });
        }
        // A founding hangar so the house can answer a rift on day one: a 2×2
        // berth to the north, its origin at (2,0), abutting the gatehouse.
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let anchor = if (dx, dy) == (0, 0) { None } else { Some((2u8, 0u8)) };
            ch.grid[dy][2 + dx] = Some(Slot {
                facility: Facility::Hangar,
                days_left: 0,
                anchor,
            });
        }
        ch
    }

    /// Resolve a cell to the origin (top-left) of whatever facility owns it.
    fn origin_of(&self, x: usize, y: usize) -> Option<(usize, usize)> {
        let slot = self.grid[y][x].as_ref()?;
        Some(slot.anchor.map_or((x, y), |(ax, ay)| (ax as usize, ay as usize)))
    }

    pub fn facility_at(&self, x: usize, y: usize) -> Option<(Facility, bool)> {
        let slot = self.grid[y][x].as_ref()?;
        let (ox, oy) = self.origin_of(x, y)?;
        let built = self.grid[oy][ox].as_ref().is_none_or(|o| o.days_left == 0);
        Some((slot.facility, built))
    }

    pub fn is_free(&self, x: usize, y: usize) -> bool {
        x < GRID && y < GRID && self.grid[y][x].is_none()
    }

    /// True when the cell shares an edge with something already standing
    /// (built or building). New walls must grow from old walls.
    pub fn touches(&self, x: usize, y: usize) -> bool {
        let (x, y) = (x as i32, y as i32);
        [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
            let (nx, ny) = (x + dx, y + dy);
            nx >= 0
                && ny >= 0
                && (nx as usize) < GRID
                && (ny as usize) < GRID
                && self.grid[ny as usize][nx as usize].is_some()
        })
    }

    /// Whether a facility's whole footprint anchored at (x, y) fits: in
    /// bounds, every cell free, and at least one cell abutting the halls.
    pub fn fits(&self, facility: Facility, x: usize, y: usize) -> bool {
        let (fw, fh) = facility.footprint();
        if x + fw > GRID || y + fh > GRID {
            return false;
        }
        let all_free = (0..fh).all(|dy| (0..fw).all(|dx| self.is_free(x + dx, y + dy)));
        let touches = (0..fh).any(|dy| (0..fw).any(|dx| self.touches(x + dx, y + dy)));
        all_free && touches
    }

    /// Begin construction. The campaign layer checks and deducts funds.
    /// New facilities must abut the existing halls — no free-standing
    /// islands out on the grounds — and their whole footprint must fit.
    pub fn start_build(&mut self, facility: Facility, x: usize, y: usize) -> bool {
        if !self.fits(facility, x, y) {
            return false;
        }
        let (fw, fh) = facility.footprint();
        let days = facility.build_days();
        for dy in 0..fh {
            for dx in 0..fw {
                let anchor = if (dx, dy) == (0, 0) {
                    None
                } else {
                    Some((x as u8, y as u8))
                };
                self.grid[y + dy][x + dx] = Some(Slot { facility, days_left: days, anchor });
            }
        }
        true
    }

    /// Days of construction left at a cell (None when empty or finished),
    /// read from whichever cell owns the footprint.
    pub fn build_days_left(&self, x: usize, y: usize) -> Option<u32> {
        let (ox, oy) = self.origin_of(x, y)?;
        self.grid[oy][ox]
            .as_ref()
            .filter(|s| s.days_left > 0)
            .map(|s| s.days_left)
    }

    /// Advance construction one day; returns facilities that completed today.
    /// Only origin cells carry build state, so a 2×2 finishes once.
    pub fn advance_day(&mut self) -> Vec<Facility> {
        let mut done = Vec::new();
        for row in &mut self.grid {
            for slot in row.iter_mut().flatten() {
                if slot.anchor.is_none() && slot.days_left > 0 {
                    slot.days_left -= 1;
                    if slot.days_left == 0 {
                        done.push(slot.facility);
                    }
                }
            }
        }
        done
    }

    pub fn count_active(&self, facility: Facility) -> usize {
        self.grid
            .iter()
            .flatten()
            .flatten()
            .filter(|s| s.facility == facility && s.days_left == 0 && s.anchor.is_none())
            .count()
    }

    /// Built airship berths — one Zeppelin apiece.
    pub fn hangars(&self) -> usize {
        self.count_active(Facility::Hangar)
    }

    /// Drop a finished hangar into the first 2×2 that fits (for save
    /// migration). Silently does nothing if the grid has no room left.
    pub fn ensure_hangar(&mut self) {
        for y in 0..GRID {
            for x in 0..GRID {
                if self.fits(Facility::Hangar, x, y) {
                    self.start_build(Facility::Hangar, x, y);
                    // Finish it at once — a migrated berth is already standing.
                    for dy in 0..2 {
                        for dx in 0..2 {
                            if let Some(slot) = self.grid[y + dy][x + dx].as_mut() {
                                slot.days_left = 0;
                            }
                        }
                    }
                    return;
                }
            }
        }
    }

    /// Beds for soldiers + occultists.
    pub fn quarters_capacity(&self) -> usize {
        4 + 10 * self.count_active(Facility::Quarters)
    }

    pub fn library_capacity(&self) -> usize {
        5 * self.count_active(Facility::Library)
    }

    pub fn workshop_capacity(&self) -> usize {
        5 * self.count_active(Facility::Workshop)
    }

    /// Grid coordinates of every built (or building) facility cell — the
    /// floor plan a Reckoning is fought in.
    pub fn occupied_cells(&self) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();
        for (y, row) in self.grid.iter().enumerate() {
            for (x, slot) in row.iter().enumerate() {
                if slot.is_some() {
                    cells.push((x, y));
                }
            }
        }
        cells
    }

    /// The cells a Reckoning is actually fought through: everything the
    /// gatehouse can reach walking cell to cell. A room cut off from the
    /// gate (by demolition or wreckage) stands outside the battle.
    pub fn linked_cells(&self) -> Vec<(usize, usize)> {
        let gate = self.gate();
        let mut order = vec![gate];
        let mut seen = std::collections::HashSet::from([gate]);
        let mut head = 0;
        while head < order.len() {
            let (x, y) = order[head];
            head += 1;
            for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if nx < 0 || ny < 0 || nx as usize >= GRID || ny as usize >= GRID {
                    continue;
                }
                let cell = (nx as usize, ny as usize);
                if self.grid[cell.1][cell.0].is_some() && seen.insert(cell) {
                    order.push(cell);
                }
            }
        }
        order
    }

    /// Where demons breach: the gatehouse cell.
    pub fn gate(&self) -> (usize, usize) {
        for (y, row) in self.grid.iter().enumerate() {
            for (x, slot) in row.iter().enumerate() {
                if slot.as_ref().is_some_and(|s| s.facility == Facility::Gatehouse) {
                    return (x, y);
                }
            }
        }
        (2, 2) // unreachable in practice: every chapterhouse is founded with one
    }

    /// Tear a facility out of the grid (demolition or Reckoning damage) —
    /// the whole footprint goes, whichever of its cells was struck.
    pub fn demolish(&mut self, x: usize, y: usize) {
        if x >= GRID || y >= GRID {
            return;
        }
        let Some((ox, oy)) = self.origin_of(x, y) else {
            return;
        };
        let (anchor_x, anchor_y) = (ox as u8, oy as u8);
        for row in self.grid.iter_mut() {
            for cell in row.iter_mut() {
                if cell.as_ref().is_some_and(|s| s.anchor == Some((anchor_x, anchor_y))) {
                    *cell = None;
                }
            }
        }
        self.grid[oy][ox] = None;
    }

    /// Upkeep, counted once per facility (a 2×2 hangar bills once).
    pub fn maintenance(&self) -> i64 {
        self.grid
            .iter()
            .flatten()
            .flatten()
            .filter(|s| s.anchor.is_none())
            .map(|s| s.facility.maintenance())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn founding_house_has_one_built_hangar() {
        let ch = Chapterhouse::founding(Region::Europe);
        assert_eq!(ch.hangars(), 1, "a founding house berths one airship");
        // The whole 2×2 footprint reads as the same built facility.
        for (x, y) in [(2, 0), (3, 0), (2, 1), (3, 1)] {
            assert_eq!(ch.facility_at(x, y), Some((Facility::Hangar, true)), "{x},{y}");
        }
    }

    #[test]
    fn a_hangar_takes_a_two_by_two_and_counts_once() {
        let mut ch = Chapterhouse::founding(Region::Europe);
        // A second hangar abutting the augur array, to the east.
        assert!(ch.start_build(Facility::Hangar, 4, 2));
        // All four cells are occupied; the origin carries the build days.
        for (x, y) in [(4, 2), (5, 2), (4, 3), (5, 3)] {
            assert!(!ch.is_free(x, y), "cell {x},{y} should be taken");
        }
        // Under construction: not yet counted as an active berth.
        assert_eq!(ch.hangars(), 1, "still just the founding berth while building");
        for _ in 0..Facility::Hangar.build_days() {
            ch.advance_day();
        }
        assert_eq!(ch.hangars(), 2, "finished: a second berth — counted once");
        // Upkeep bills each facility once, not per cell (two hangars = 2×).
        let expected = Facility::Gatehouse.maintenance()
            + Facility::Quarters.maintenance()
            + Facility::AugurArray.maintenance()
            + Facility::Library.maintenance()
            + 2 * Facility::Hangar.maintenance();
        assert_eq!(ch.maintenance(), expected);
    }

    #[test]
    fn demolishing_any_hangar_cell_removes_the_whole_berth() {
        let mut ch = Chapterhouse::founding(Region::Europe);
        assert_eq!(ch.hangars(), 1);
        // Strike a satellite cell, not the origin.
        ch.demolish(3, 1);
        assert_eq!(ch.hangars(), 0, "the whole berth is gone");
        for (x, y) in [(2, 0), (3, 0), (2, 1), (3, 1)] {
            assert!(ch.is_free(x, y), "cell {x},{y} should be clear");
        }
    }

    #[test]
    fn a_hangar_will_not_overhang_the_grid_or_collide() {
        let ch = Chapterhouse::founding(Region::Europe);
        // Off the east edge: the 2×2 would run out of bounds.
        assert!(!ch.fits(Facility::Hangar, GRID - 1, 2));
        // Onto the founding cluster: cells already taken.
        assert!(!ch.fits(Facility::Hangar, 2, 2));
    }
}
