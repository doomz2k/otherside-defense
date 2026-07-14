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
}

impl Facility {
    pub const BUILDABLE: [Facility; 11] = [
        Facility::Quarters,
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
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Slot {
    facility: Facility,
    days_left: u32,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Chapterhouse {
    pub region: Region,
    grid: [[Option<Slot>; GRID]; GRID],
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
            occultists: 0,
            artificers: 0,
        };
        for (x, y, f) in [
            (2, 2, Facility::Gatehouse),
            (2, 3, Facility::Quarters),
            (3, 2, Facility::AugurArray),
            (3, 3, Facility::Library),
        ] {
            ch.grid[y][x] = Some(Slot { facility: f, days_left: 0 });
        }
        ch
    }

    pub fn facility_at(&self, x: usize, y: usize) -> Option<(Facility, bool)> {
        self.grid[y][x]
            .as_ref()
            .map(|s| (s.facility, s.days_left == 0))
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

    /// Begin construction. The campaign layer checks and deducts funds.
    /// New facilities must abut the existing halls — no free-standing
    /// islands out on the grounds.
    pub fn start_build(&mut self, facility: Facility, x: usize, y: usize) -> bool {
        if !self.is_free(x, y) || !self.touches(x, y) {
            return false;
        }
        self.grid[y][x] = Some(Slot {
            facility,
            days_left: facility.build_days(),
        });
        true
    }

    /// Days of construction left at a cell (None when empty or finished).
    pub fn build_days_left(&self, x: usize, y: usize) -> Option<u32> {
        self.grid[y][x]
            .as_ref()
            .filter(|s| s.days_left > 0)
            .map(|s| s.days_left)
    }

    /// Advance construction one day; returns facilities that completed today.
    pub fn advance_day(&mut self) -> Vec<Facility> {
        let mut done = Vec::new();
        for row in &mut self.grid {
            for slot in row.iter_mut().flatten() {
                if slot.days_left > 0 {
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
            .filter(|s| s.facility == facility && s.days_left == 0)
            .count()
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

    /// Tear a facility out of the grid (Reckoning damage).
    pub fn demolish(&mut self, x: usize, y: usize) {
        if x < GRID && y < GRID {
            self.grid[y][x] = None;
        }
    }

    pub fn maintenance(&self) -> i64 {
        self.grid
            .iter()
            .flatten()
            .flatten()
            .map(|s| s.facility.maintenance())
            .sum()
    }
}
