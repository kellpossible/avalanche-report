use enum_iterator::Sequence;
use forecast_spreadsheet::{Distribution, Sensitivity};
use serde::{Deserialize, Serialize};

/// An imprecise probability of triggering a given avalanche type, a function of sensitivity to
/// triggers and spatial distribution. from [ADAM
/// paper](https://arc.lib.montana.edu/snow-science/objects/ISSW16_O20.03.pdf).
#[derive(Serialize, Deserialize, Debug, PartialEq, PartialOrd, Eq, Ord, Copy, Clone, Sequence)]
#[serde(rename_all = "kebab-case")]
pub enum Probability {
    Unlikely = 0,
    Possible = 1,
    Likely = 2,
    VeryLikely = 3,
}

use Probability::*;

#[rustfmt::skip]
const MATRIX: &[&[Probability]] = &[
    // Isolated, Specific, Widespread
    &[Unlikely, Unlikely, Unlikely  ], // Unreactive
    &[Unlikely, Possible, Possible  ], // Stubborn
    &[Possible, Possible, Likely    ], // Reactive
    &[Possible, Likely  , VeryLikely], // Touchy
];

impl Probability {
    pub fn id(&self) -> &'static str {
        match self {
            VeryLikely => "very-likely",
            Likely => "likely",
            Possible => "possible",
            Unlikely => "unlikely",
        }
    }

    /// Calculate a new [`Probability`].
    pub fn calculate(sensitivity: Sensitivity, distribution: Distribution) -> Self {
        MATRIX[sensitivity as usize][distribution as usize]
    }
}
