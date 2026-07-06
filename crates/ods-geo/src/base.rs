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
}

impl Facility {
    pub fn cost(self) -> i64 {
        match self {
            Facility::Gatehouse => 0,
            Facility::Quarters => 100,
            Facility::AugurArray => 150,
            Facility::Library => 150,
            Facility::Infirmary => 200,
        }
    }

    pub fn build_days(self) -> u32 {
        match self {
            Facility::Gatehouse => 0,
            Facility::Quarters => 8,
            Facility::AugurArray => 10,
            Facility::Library => 10,
            Facility::Infirmary => 12,
        }
    }

    pub fn maintenance(self) -> i64 {
        match self {
            Facility::Gatehouse => 20,
            Facility::Quarters => 5,
            Facility::AugurArray => 10,
            Facility::Library => 10,
            Facility::Infirmary => 15,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Facility::Gatehouse => "Gatehouse",
            Facility::Quarters => "Quarters",
            Facility::AugurArray => "Augur Array",
            Facility::Library => "Library",
            Facility::Infirmary => "Infirmary",
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
}

impl Chapterhouse {
    /// A founding chapterhouse: gatehouse, quarters, one augur, one library.
    pub fn founding(region: Region) -> Self {
        let mut ch = Self {
            region,
            grid: Default::default(),
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

    /// Begin construction. The campaign layer checks and deducts funds.
    pub fn start_build(&mut self, facility: Facility, x: usize, y: usize) -> bool {
        if !self.is_free(x, y) {
            return false;
        }
        self.grid[y][x] = Some(Slot {
            facility,
            days_left: facility.build_days(),
        });
        true
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

    pub fn maintenance(&self) -> i64 {
        self.grid
            .iter()
            .flatten()
            .flatten()
            .map(|s| s.facility.maintenance())
            .sum()
    }
}
