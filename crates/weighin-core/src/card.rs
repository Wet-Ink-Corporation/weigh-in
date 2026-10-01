//! The grounded decision card. Its field names are the board's (`board.html` renders them as-is),
//! so a card written by `weighin ground` is exactly what the board shows.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Evidence {
    pub src: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Excerpt {
    pub path: String,
    pub start: usize,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CardOption {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub holds_if: String,
    #[serde(default)]
    pub if_wrong: String,
    /// The MUST this option fails. An option that fails a MUST is shown struck through, never scored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fails_must: Option<String>,
    /// The evasion pattern this option matches, if any ("revisit later" with no trigger, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evasion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Criterion {
    pub name: String,
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Stability {
    pub agree: u32,
    pub of: u32,
    #[serde(default)]
    pub samples: String,
    #[serde(default)]
    pub note: String,
}

impl Stability {
    /// Stable means at least three of four picks agreed (design: Opus plus three Sonnet samples).
    #[must_use]
    pub fn is_stable(&self) -> bool {
        self.of > 0 && self.agree * 4 >= self.of * 3
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Recommendation {
    pub option: String,
    /// "A unless X": the fact that would change the pick.
    #[serde(default)]
    pub flip: String,
    #[serde(default)]
    pub tradeoff: String,
    #[serde(default)]
    pub why_wrong: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stability: Option<Stability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EvasionCheck {
    pub passed: bool,
    #[serde(default)]
    pub flags: Vec<String>,
}

/// Everything grounding adds to a raised question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Card {
    #[serde(default)]
    pub why: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub door_note: Option<String>,
    #[serde(default)]
    pub timing: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<Excerpt>,
    pub options: Vec<CardOption>,
    #[serde(default)]
    pub criteria: Vec<Criterion>,
    pub recommendation: Recommendation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evasion: Option<EvasionCheck>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CardError {
    #[error("a card needs between 2 and 4 options, this one has {0}")]
    OptionCount(usize),
    #[error("option ids must be unique; `{0}` repeats")]
    DuplicateOption(String),
    #[error("the recommendation names `{0}`, which is not an option")]
    UnknownRecommendation(String),
    #[error("the recommended option `{0}` fails a MUST")]
    RecommendsIneligible(String),
    #[error("the recommended option `{0}` is flagged as evasive: {1}")]
    RecommendsEvasion(String, String),
}

impl Card {
    /// Structural checks every card must pass before it is recorded.
    ///
    /// # Errors
    /// Returns the first rule the card breaks.
    pub fn validate(&self) -> Result<(), CardError> {
        let n = self.options.len();
        if !(2..=4).contains(&n) {
            return Err(CardError::OptionCount(n));
        }
        let mut seen = std::collections::BTreeSet::new();
        for o in &self.options {
            if !seen.insert(o.id.as_str()) {
                return Err(CardError::DuplicateOption(o.id.clone()));
            }
        }
        let rec = self
            .option(&self.recommendation.option)
            .ok_or_else(|| CardError::UnknownRecommendation(self.recommendation.option.clone()))?;
        if rec.fails_must.is_some() {
            return Err(CardError::RecommendsIneligible(rec.id.clone()));
        }
        if let Some(e) = &rec.evasion {
            return Err(CardError::RecommendsEvasion(rec.id.clone(), e.clone()));
        }
        Ok(())
    }

    #[must_use]
    pub fn option(&self, id: &str) -> Option<&CardOption> {
        self.options.iter().find(|o| o.id == id)
    }

    /// Whether the recommendation may be pre-selected: it must be stable and pass the evasion check.
    /// One-way items are presented options-first regardless (decided by the caller from the door).
    #[must_use]
    pub fn recommendation_earned(&self) -> bool {
        let stable = self
            .recommendation
            .stability
            .as_ref()
            .is_none_or(Stability::is_stable);
        let evasion_ok = self.evasion.as_ref().is_none_or(|e| e.passed);
        stable && evasion_ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(id: &str) -> CardOption {
        CardOption {
            id: id.into(),
            title: format!("Option {id}"),
            ..Default::default()
        }
    }

    fn card() -> Card {
        Card {
            options: vec![opt("A"), opt("B")],
            recommendation: Recommendation {
                option: "A".into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn valid_card_passes() {
        assert_eq!(card().validate(), Ok(()));
    }

    #[test]
    fn rejects_bad_shapes() {
        let mut c = card();
        c.options.truncate(1);
        assert_eq!(c.validate(), Err(CardError::OptionCount(1)));
        let mut c = card();
        c.recommendation.option = "Z".into();
        assert!(matches!(
            c.validate(),
            Err(CardError::UnknownRecommendation(_))
        ));
        let mut c = card();
        c.options[0].fails_must = Some("one-way doors get a human".into());
        assert!(matches!(
            c.validate(),
            Err(CardError::RecommendsIneligible(_))
        ));
        let mut c = card();
        c.options[0].evasion = Some("revisit later with no trigger".into());
        assert!(matches!(
            c.validate(),
            Err(CardError::RecommendsEvasion(..))
        ));
    }

    #[test]
    fn earned_needs_three_of_four() {
        let mut c = card();
        c.recommendation.stability = Some(Stability {
            agree: 2,
            of: 4,
            ..Default::default()
        });
        assert!(!c.recommendation_earned());
        c.recommendation.stability = Some(Stability {
            agree: 3,
            of: 4,
            ..Default::default()
        });
        assert!(c.recommendation_earned());
    }
}
