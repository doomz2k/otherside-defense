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
}
