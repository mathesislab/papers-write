//! The colours of Papers Desktop's window, by name: that of a project (its
//! tile in the project bar, its dot among the recent projects), and that of
//! the bar at the top of the app's windows (the settings). How they are
//! drawn is the page's.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Colour {
    Blue,
    Teal,
    Green,
    Yellow,
    Orange,
    Red,
    Pink,
    Purple,
    Graphite,
    /// no bar
    None,
}

impl Colour {
    /// The colours a new project takes, in turn.
    pub const ALL: [Colour; 9] = [
        Colour::Blue,
        Colour::Teal,
        Colour::Green,
        Colour::Yellow,
        Colour::Orange,
        Colour::Red,
        Colour::Pink,
        Colour::Purple,
        Colour::Graphite,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Colour::Blue => "blue",
            Colour::Teal => "teal",
            Colour::Green => "green",
            Colour::Yellow => "yellow",
            Colour::Orange => "orange",
            Colour::Red => "red",
            Colour::Pink => "pink",
            Colour::Purple => "purple",
            Colour::Graphite => "graphite",
            Colour::None => "none",
        }
    }

    /// The colour of a new project: the first not in use, else the least used.
    pub fn next(in_use: &[Colour]) -> Colour {
        let uses = |c: &Colour| in_use.iter().filter(|u| *u == c).count();
        Colour::ALL
            .into_iter()
            .min_by_key(uses)
            .unwrap_or(Colour::Blue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_is_the_first_not_in_use_then_the_least_used() {
        assert_eq!(Colour::next(&[]), Colour::Blue);
        assert_eq!(Colour::next(&[Colour::Blue, Colour::Green]), Colour::Teal);
        let all: Vec<Colour> = Colour::ALL.to_vec();
        assert_eq!(Colour::next(&all), Colour::Blue);
        let mut twice = all.clone();
        twice.push(Colour::Blue);
        assert_eq!(Colour::next(&twice), Colour::Teal);
        // no bar is not a colour in use
        assert_eq!(Colour::next(&[Colour::None]), Colour::Blue);
    }
}
