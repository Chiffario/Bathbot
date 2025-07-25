mod attributes;
mod claim_name;
mod country_snipe_stats;
mod fix_score;
mod medal_stats;
mod osustats_counts;
mod player_snipe_stats;
mod pp_missing;
mod profile_compare;
mod ratio;
mod sniped;
mod whatif;

#[cfg(feature = "matchlive")]
mod match_live;

use std::fmt::{Display, Formatter, Result as FmtResult};

use rosu_v2::prelude::{GameModIntermode, GameMode, GameMods, ScoreStatistics};

#[cfg(feature = "matchlive")]
pub use self::match_live::*;
pub use self::{
    attributes::*, claim_name::*, country_snipe_stats::*, fix_score::*, medal_stats::*,
    osustats_counts::*, player_snipe_stats::*, pp_missing::*, profile_compare::*, ratio::*,
    sniped::*, whatif::*,
};

pub struct ComboFormatter {
    score: u32,
    max: Option<u32>,
}

impl ComboFormatter {
    pub fn new(score: u32, max: Option<u32>) -> Self {
        Self { score, max }
    }
}

impl Display for ComboFormatter {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "**{}x**/", self.score)?;

        match self.max {
            Some(combo) => write!(f, "{combo}x"),
            None => f.write_str("-"),
        }
    }
}

pub struct PpFormatter {
    actual: Option<f32>,
    max: Option<f32>,
}

impl PpFormatter {
    pub fn new(actual: Option<f32>, max: Option<f32>) -> Self {
        Self { actual, max }
    }
}

impl Display for PpFormatter {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match (self.actual, self.max) {
            (Some(actual), Some(max)) => {
                write!(f, "**{actual:.2}**/{max:.2}", max = max.max(actual))?
            }
            (Some(actual), None) => write!(f, "**{actual:.2}**/-")?,
            (None, Some(max)) => write!(f, "-/{max:.2}")?,
            (None, None) => f.write_str("-/-")?,
        }

        f.write_str("PP")
    }
}

pub struct KeyFormatter<'m> {
    mods: &'m GameMods,
    cs: u32,
}

impl<'m> KeyFormatter<'m> {
    pub fn new(mods: &'m GameMods, cs: f32) -> Self {
        Self {
            mods,
            cs: cs as u32,
        }
    }
}

impl Display for KeyFormatter<'_> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let key_mod = [
            GameModIntermode::OneKey,
            GameModIntermode::TwoKeys,
            GameModIntermode::ThreeKeys,
            GameModIntermode::FourKeys,
            GameModIntermode::FiveKeys,
            GameModIntermode::SixKeys,
            GameModIntermode::SevenKeys,
            GameModIntermode::EightKeys,
            GameModIntermode::NineKeys,
            GameModIntermode::TenKeys,
        ]
        .into_iter()
        .find(|gamemod| self.mods.contains_intermode(gamemod));

        match key_mod {
            Some(key_mod) => write!(f, "[{key_mod}]"),
            None => write!(f, "[{}K]", self.cs),
        }
    }
}

#[derive(Clone)]
pub struct HitResultFormatter<'a> {
    mode: GameMode,
    stats: &'a ScoreStatistics,
}

impl<'a> HitResultFormatter<'a> {
    pub fn new(mode: GameMode, stats: &'a ScoreStatistics) -> Self {
        Self { mode, stats }
    }
}

impl Display for HitResultFormatter<'_> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str("{")?;

        if self.mode == GameMode::Mania {
            write!(f, "{}/", self.stats.perfect)?;
        }

        write!(f, "{}/", self.stats.great)?;

        if self.mode == GameMode::Mania {
            write!(f, "{}/", self.stats.good)?;
        }

        let n100 = match self.mode {
            GameMode::Osu | GameMode::Taiko | GameMode::Mania => self.stats.ok,
            GameMode::Catch => self.stats.ok.max(self.stats.large_tick_hit),
        };

        write!(f, "{n100}/")?;

        if self.mode != GameMode::Taiko {
            let n50 = match self.mode {
                GameMode::Osu | GameMode::Mania => self.stats.meh,
                GameMode::Catch => self.stats.meh.max(self.stats.small_tick_hit),
                GameMode::Taiko => unreachable!(),
            };

            write!(f, "{n50}/")?;
        }

        write!(f, "{}}}", self.stats.miss)
    }
}
