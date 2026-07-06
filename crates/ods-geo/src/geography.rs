//! The world as the funding council sees it: eight regions, each with a
//! monthly contribution that rises and falls with your performance there.

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum Region {
    NorthAmerica,
    SouthAmerica,
    Europe,
    Africa,
    MiddleEast,
    Asia,
    Oceania,
    Arctic,
}

impl Region {
    pub const ALL: [Region; 8] = [
        Region::NorthAmerica,
        Region::SouthAmerica,
        Region::Europe,
        Region::Africa,
        Region::MiddleEast,
        Region::Asia,
        Region::Oceania,
        Region::Arctic,
    ];

    /// Rough bounding box (lat_min, lat_max, lon_min, lon_max) for placing
    /// rifts inside the region on the globe.
    pub fn bounds(self) -> (f32, f32, f32, f32) {
        match self {
            Region::NorthAmerica => (18.0, 60.0, -125.0, -70.0),
            Region::SouthAmerica => (-45.0, 5.0, -75.0, -40.0),
            Region::Europe => (38.0, 62.0, -8.0, 35.0),
            Region::Africa => (-30.0, 28.0, -12.0, 45.0),
            Region::MiddleEast => (14.0, 40.0, 28.0, 60.0),
            Region::Asia => (10.0, 60.0, 45.0, 140.0),
            Region::Oceania => (-42.0, -12.0, 115.0, 175.0),
            Region::Arctic => (68.0, 80.0, -50.0, 50.0),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Region::NorthAmerica => "North America",
            Region::SouthAmerica => "South America",
            Region::Europe => "Europe",
            Region::Africa => "Africa",
            Region::MiddleEast => "Middle East",
            Region::Asia => "Asia",
            Region::Oceania => "Oceania",
            Region::Arctic => "the Arctic",
        }
    }

    /// The ground the squads actually fight on there: drives battlemap
    /// generation for rift assaults in this region.
    pub fn biome(self) -> ods_sim::scenario::Biome {
        use ods_sim::scenario::Biome;
        match self {
            Region::NorthAmerica | Region::Europe | Region::Asia => Biome::Temperate,
            Region::Africa | Region::MiddleEast => Biome::Desert,
            Region::SouthAmerica | Region::Oceania => Biome::Jungle,
            Region::Arctic => Biome::Tundra,
        }
    }

    /// Marker anchor / squad-departure point per region (lat, lon).
    pub fn centroid(self) -> (f32, f32) {
        match self {
            Region::NorthAmerica => (45.0, -100.0),
            Region::SouthAmerica => (-15.0, -60.0),
            Region::Europe => (50.0, 15.0),
            Region::Africa => (5.0, 20.0),
            Region::MiddleEast => (28.0, 45.0),
            Region::Asia => (45.0, 90.0),
            Region::Oceania => (-25.0, 135.0),
            Region::Arctic => (75.0, -40.0),
        }
    }

    /// Great-circle angular distance in degrees between two (lat, lon)
    /// points — the yardstick for squad travel time.
    pub fn arc_degrees(a: (f32, f32), b: (f32, f32)) -> f32 {
        let (la, lo) = (a.0.to_radians(), a.1.to_radians());
        let (lb, lp) = (b.0.to_radians(), b.1.to_radians());
        let cos = la.sin() * lb.sin() + la.cos() * lb.cos() * (lo - lp).cos();
        cos.clamp(-1.0, 1.0).acos().to_degrees()
    }
}
