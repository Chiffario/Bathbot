use std::{
    iter::{self, Copied, Map},
    slice::Iter,
};

use rosu_v2::prelude::{
    GameMod, GameModIntermode, GameMode, GameMods, GameModsIntermode, Grade, LegacyScoreStatistics,
    Score, ScoreStatistics,
};

use crate::{constants::OSU_BASE, numbers::round};

#[derive(Clone, Debug, PartialEq)]
pub enum ModSelection {
    Include(GameModsIntermode),
    Exclude(GameModsIntermode),
    Exact(GameModsIntermode),
}

impl ModSelection {
    pub fn as_mods(&self) -> &GameModsIntermode {
        match self {
            Self::Include(m) | Self::Exclude(m) | Self::Exact(m) => m,
        }
    }

    pub fn into_mods(self) -> GameModsIntermode {
        match self {
            Self::Include(m) | Self::Exclude(m) | Self::Exact(m) => m,
        }
    }

    /// Returns `true` if the score's mods coincide with this [`ModSelection`]
    pub fn filter_score(&self, score: &Score) -> bool {
        const DT: GameModIntermode = GameModIntermode::DoubleTime;
        const NC: GameModIntermode = GameModIntermode::Nightcore;
        const SD: GameModIntermode = GameModIntermode::SuddenDeath;
        const PF: GameModIntermode = GameModIntermode::Perfect;

        match self {
            ModSelection::Include(mods) | ModSelection::Exact(mods) if mods.is_empty() => {
                score.mods.is_empty()
            }
            ModSelection::Include(mods) => mods.iter().all(|gamemod| match gamemod {
                DT => score.mods.contains_intermode(DT) || score.mods.contains_intermode(NC),
                SD => score.mods.contains_intermode(SD) || score.mods.contains_intermode(PF),
                _ => score.mods.contains_intermode(gamemod),
            }),
            ModSelection::Exclude(mods) if mods.is_empty() => !score.mods.is_empty(),
            ModSelection::Exclude(mods) => !mods.iter().any(|gamemod| match gamemod {
                DT => score.mods.contains_intermode(DT) || score.mods.contains_intermode(NC),
                SD => score.mods.contains_intermode(SD) || score.mods.contains_intermode(PF),
                _ => score.mods.contains_intermode(gamemod),
            }),
            ModSelection::Exact(mods) => score.mods.iter().map(GameMod::intermode).eq(mods.iter()),
        }
    }

    /// Remove all scores whos mods do not coincide with this [`ModSelection`]
    pub fn filter_scores(&self, scores: &mut Vec<Score>) {
        match self {
            ModSelection::Include(mods) | ModSelection::Exact(mods) if mods.is_empty() => {
                scores.retain(|score| score.mods.is_empty())
            }
            ModSelection::Include(mods) => scores.retain(|score| {
                mods.iter()
                    .all(|gamemod| score.mods.contains_intermode(gamemod))
            }),
            ModSelection::Exclude(mods) if mods.is_empty() => {
                scores.retain(|score| !score.mods.is_empty())
            }
            ModSelection::Exclude(mods) => scores.retain(|score| {
                !mods
                    .iter()
                    .any(|gamemod| score.mods.contains_intermode(gamemod))
            }),
            ModSelection::Exact(mods) => {
                scores.retain(|score| score.mods.iter().map(GameMod::intermode).eq(mods.iter()))
            }
        }
    }

    /// Make sure included or exact mods don't exclude each other e.g. EZHR
    pub fn validate(self, mode: GameMode) -> Result<(), &'static str> {
        let mods = match self {
            Self::Include(mods) => mods,
            Self::Exclude(_) => return Ok(()),
            Self::Exact(mods) => mods,
        };

        let mods = mods
            .try_with_mode(mode)
            .ok_or("Looks like inappropriate mods for the mode")?;

        if mods.is_valid() {
            Ok(())
        } else {
            Err("Looks like an invalid mod combination")
        }
    }
}

pub fn flag_url(country_code: &str) -> String {
    // format!("{OSU_BASE}/images/flags/{country_code}.png") // from osu itself but
    // outdated
    flag_url_size(country_code, 256)
}

pub fn flag_url_size(country_code: &str, size: u32) -> String {
    format!("https://osuflags.omkserver.nl/{country_code}-{size}.png") // kelderman
}

pub fn flag_url_svg(country_code: &str) -> String {
    const OFFSET: u32 = 0x1F1A5;

    let [byte0, byte1] = country_code.as_bytes() else {
        panic!("country code `{country_code}` is invalid");
    };

    let url = format!(
        "{OSU_BASE}assets/images/flags/{:x}-{:x}.svg",
        byte0.to_ascii_uppercase() as u32 + OFFSET,
        byte1.to_ascii_uppercase() as u32 + OFFSET
    );

    url
}

pub trait ExtractablePp {
    fn extract_pp(&self) -> Vec<f32>;
}

impl ExtractablePp for [Score] {
    fn extract_pp(&self) -> Vec<f32> {
        self.iter().map(|s| s.pp.unwrap_or(0.0)).collect()
    }
}

// Credits to flowabot
/// Extend the list of pps by taking the average difference
/// between 2 values towards the end and create more values
/// based on that difference
pub fn approx_more_pp(pps: &mut Vec<f32>, more: usize) {
    if pps.len() != 100 {
        return;
    }

    let diff = (pps[89] - pps[99]) / 10.0;

    let extension = iter::successors(pps.last().copied(), |pp| {
        let pp = pp - diff;

        (pp > 0.0).then_some(pp)
    });

    pps.extend(extension.take(more));
}

pub trait PpListUtil {
    /// Accumulate the weighted pp values i.e. sum up `0.95^i * pp`
    fn accum_weighted(&self) -> f32;
}

impl PpListUtil for [f32] {
    fn accum_weighted(&self) -> f32 {
        self.iter()
            .copied()
            .zip(0..)
            .fold(0.0, |sum, (pp, i)| sum + pp * 0.95_f32.powi(i))
    }
}

pub trait IntoPpIter {
    type Inner: Iterator<Item = f32> + DoubleEndedIterator + ExactSizeIterator;

    fn into_pps(self) -> PpIter<Self::Inner>;
}

impl<'s> IntoPpIter for &'s [Score] {
    type Inner = Map<Iter<'s, Score>, fn(&Score) -> f32>;

    #[inline]
    fn into_pps(self) -> PpIter<Self::Inner> {
        PpIter {
            inner: self.iter().map(|score| score.pp.unwrap_or(0.0)),
        }
    }
}

impl<'f> IntoPpIter for &'f [f32] {
    type Inner = Copied<Iter<'f, f32>>;

    #[inline]
    fn into_pps(self) -> PpIter<Self::Inner> {
        PpIter {
            inner: self.iter().copied(),
        }
    }
}

pub struct PpIter<I> {
    inner: I,
}

impl<I: Iterator<Item = f32>> Iterator for PpIter<I> {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<I: Iterator<Item = f32> + DoubleEndedIterator> DoubleEndedIterator for PpIter<I> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner.next_back()
    }
}

impl<I: Iterator<Item = f32> + ExactSizeIterator> ExactSizeIterator for PpIter<I> {
    #[inline]
    fn len(&self) -> usize {
        self.inner.len()
    }
}

/// First element: Weighted missing pp to reach goal from start
///
/// Second element: Index of hypothetical pp in pps
pub fn pp_missing(start: f32, goal: f32, pps: impl IntoPpIter) -> (f32, usize) {
    let mut top = start;
    let mut bot = 0.0;

    //     top + x * 0.95^i + bot = goal
    // <=> x = (goal - top - bot) / 0.95^i
    fn calculate_remaining(idx: usize, goal: f32, top: f32, bot: f32) -> (f32, usize) {
        let factor = 0.95_f32.powi(idx as i32);
        let required = (goal - top - bot) / factor;

        (required, idx)
    }

    for (i, last_pp) in pps.into_pps().enumerate().rev() {
        let factor = 0.95_f32.powi(i as i32);
        let term = factor * last_pp;
        let bot_term = term * 0.95;

        if top + bot + bot_term >= goal {
            return calculate_remaining(i + 1, goal, top, bot);
        }

        bot += bot_term;
        top -= term;
    }

    calculate_remaining(0, goal, top, bot)
}

#[derive(Copy, Clone, Debug)]
pub enum MapIdType {
    Map(u32),
    Set(u32),
}

// Credits to https://github.com/RoanH/osu-BonusPP/blob/master/BonusPP/src/me/roan/bonuspp/BonusPP.java#L202
pub struct BonusPP {
    pp: f32,
    ys: [f32; 100],
    len: usize,

    sum_x: f32,
    avg_x: f32,
    avg_y: f32,
}

impl Default for BonusPP {
    #[inline]
    fn default() -> Self {
        Self {
            pp: 0.0,
            ys: [0.0; 100],
            len: 0,

            sum_x: 0.0,
            avg_x: 0.0,
            avg_y: 0.0,
        }
    }
}

impl BonusPP {
    // The actual scores count is being limited to 1,000 in the SQL query resulting
    // in a maximum slightly below 416.67 <https://github.com/ppy/osu-queue-score-statistics/blob/219183cdf1870fd8ce81f2a80b1fe6a4dad48688/osu.Server.Queues.ScoreStatisticsProcessor/Processors/UserTotalPerformanceProcessor.cs#L73>
    const LIMIT: i32 = 1_000;
    const MAX: f32 = 413.89;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, weighted_pp: f32, idx: usize) {
        self.pp += weighted_pp;
        self.ys[idx] = weighted_pp.log10() / 2.0;
        self.len += 1;

        let n = idx as f32 + 1.0;
        let weight = (n + 1.0).ln_1p();

        self.sum_x += weight;
        self.avg_x += n * weight;
        self.avg_y += self.ys[idx] * weight;
    }

    pub fn calculate(self, stats: &impl UserStats) -> f32 {
        fn inner(bonus_pp: BonusPP, stats_pp: f32, grade_counts_sum: i32, playcount: u32) -> f32 {
            let BonusPP {
                mut pp,
                len,
                ys,
                sum_x,
                mut avg_x,
                mut avg_y,
            } = bonus_pp;

            if grade_counts_sum >= BonusPP::LIMIT {
                return BonusPP::MAX;
            } else if stats_pp.abs() < f32::EPSILON {
                return round((417.0 - 1.0 / 3.0) * (1.0 - 0.995_f32.powi(grade_counts_sum)));
            } else if bonus_pp.len < 100 {
                return round(stats_pp - pp);
            }

            avg_x /= sum_x;
            avg_y /= sum_x;

            let mut sum_xy = 0.0;
            let mut sum_x2 = 0.0;

            for n in 1..=len {
                let diff_x = n as f32 - avg_x;
                let ln_n = (n as f32 + 1.0).ln_1p();

                sum_xy += diff_x * (ys[n - 1] - avg_y) * ln_n;
                sum_x2 += diff_x * diff_x * ln_n;
            }

            let xy = sum_xy / sum_x;
            let x2 = sum_x2 / sum_x;

            let m = xy / x2;
            let b = avg_y - (xy / x2) * avg_x;

            for n in 100..=playcount {
                let val = 100.0_f32.powf(m * n as f32 + b);

                if val <= 0.0 {
                    break;
                }

                pp += val;
            }

            round(stats_pp - pp).clamp(0.0, BonusPP::MAX)
        }

        let pp = stats.pp();
        let grade_counts_sum = stats.grade_counts_sum();
        let playcount = stats.playcount();

        inner(self, pp, grade_counts_sum, playcount)
    }
}

pub trait UserStats {
    fn pp(&self) -> f32;
    fn grade_counts_sum(&self) -> i32;
    fn playcount(&self) -> u32;
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum AttributeKind {
    Ar,
    Cs,
    Hp,
    Od,
}

pub trait GradeGameMods {
    fn hd(&self) -> bool;
    fn fl(&self) -> bool;
    fn fi(&self) -> bool;
}

impl GradeGameMods for GameMods {
    fn hd(&self) -> bool {
        self.contains_intermode(GameModIntermode::Hidden)
    }

    fn fl(&self) -> bool {
        self.contains_intermode(GameModIntermode::Flashlight)
    }

    fn fi(&self) -> bool {
        self.contains_intermode(GameModIntermode::FadeIn)
    }
}

impl GradeGameMods for GameModsIntermode {
    fn hd(&self) -> bool {
        self.contains(GameModIntermode::Hidden)
    }

    fn fl(&self) -> bool {
        self.contains(GameModIntermode::Flashlight)
    }

    fn fi(&self) -> bool {
        self.contains(GameModIntermode::FadeIn)
    }
}

struct GradeGameModsData {
    hd: bool,
    fl: bool,
    fi: bool,
}

impl GradeGameModsData {
    fn new(mods: &impl GradeGameMods) -> Self {
        Self {
            hd: mods.hd(),
            fl: mods.fl(),
            fi: mods.fi(),
        }
    }
}

pub fn calculate_grade(
    mode: GameMode,
    mods: &impl GradeGameMods,
    stats: &ScoreStatistics,
    max_stats: Option<&ScoreStatistics>,
) -> Grade {
    let Some(max_stats) = max_stats else {
        return calculate_legacy_grade(mode, mods, stats);
    };

    let mods = GradeGameModsData::new(mods);

    match mode {
        GameMode::Osu => osu_grade(mods, stats, max_stats),
        GameMode::Taiko => taiko_grade(mods, stats, max_stats),
        GameMode::Catch => catch_grade_legacy(mods, stats),
        GameMode::Mania => mania_grade_legacy(mods, stats, stats.total_hits(GameMode::Mania)),
    }
}

pub fn calculate_legacy_grade(
    mode: GameMode,
    mods: &impl GradeGameMods,
    stats: &impl LegacyStatistics,
) -> Grade {
    let mods = GradeGameModsData::new(mods);

    match mode {
        GameMode::Osu => osu_grade_legacy(mods, stats, stats.passed_objects(GameMode::Osu)),
        GameMode::Taiko => taiko_grade_legacy(mods, stats, stats.passed_objects(GameMode::Taiko)),
        GameMode::Catch => catch_grade_legacy(mods, stats),
        GameMode::Mania => mania_grade_legacy(mods, stats, stats.passed_objects(GameMode::Mania)),
    }
}

fn osu_grade(
    mods: GradeGameModsData,
    stats: &ScoreStatistics,
    max_stats: &ScoreStatistics,
) -> Grade {
    if stats.great == max_stats.great
        && stats.large_tick_hit == max_stats.large_tick_hit
        && stats.slider_tail_hit == max_stats.slider_tail_hit
    {
        return if mods.hd || mods.fl {
            Grade::XH
        } else {
            Grade::X
        };
    }

    let accuracy = stats.accuracy(GameMode::Osu, max_stats);

    if accuracy >= 95.0 && stats.miss == 0 && stats.large_tick_hit == max_stats.large_tick_hit {
        if mods.hd || mods.fl || mods.fi {
            Grade::SH
        } else {
            Grade::S
        }
    } else if accuracy >= 90.0 {
        Grade::A
    } else if accuracy >= 80.0 {
        Grade::B
    } else if accuracy >= 70.0 {
        Grade::C
    } else {
        Grade::D
    }
}

fn taiko_grade(
    mods: GradeGameModsData,
    stats: &ScoreStatistics,
    max_stats: &ScoreStatistics,
) -> Grade {
    if stats.great == max_stats.great {
        return if mods.hd || mods.fl {
            Grade::XH
        } else {
            Grade::X
        };
    }

    let accuracy = stats.accuracy(GameMode::Taiko, max_stats);

    if accuracy >= 95.0 && stats.miss == 0 {
        if mods.hd || mods.fl || mods.fi {
            Grade::SH
        } else {
            Grade::S
        }
    } else if accuracy >= 90.0 {
        Grade::A
    } else if accuracy >= 80.0 {
        Grade::B
    } else if accuracy >= 70.0 {
        Grade::C
    } else {
        Grade::D
    }
}

pub trait LegacyStatistics {
    fn n_geki(&self) -> u32;
    fn n_katu(&self) -> u32;
    fn n300(&self) -> u32;
    fn n100(&self) -> u32;
    fn n50(&self) -> u32;
    fn n_miss(&self) -> u32;
    fn passed_objects(&self, mode: GameMode) -> u32;
    fn accuracy(&self, mode: GameMode) -> f32;
}

#[rustfmt::skip]
impl LegacyStatistics for ScoreStatistics {
    fn n_geki(&self) -> u32 { self.perfect }
    fn n_katu(&self) -> u32 { self.good }
    fn n300(&self) -> u32 { self.great }
    fn n100(&self) -> u32 { self.ok }
    fn n50(&self) -> u32 { self.meh }
    fn n_miss(&self) -> u32 { self.miss }
    fn passed_objects(&self, mode: GameMode) -> u32 { self.total_hits(mode) }
    fn accuracy(&self, mode: GameMode) -> f32 { self.legacy_accuracy(mode) }
}

#[rustfmt::skip]
impl LegacyStatistics for LegacyScoreStatistics {
    fn n_geki(&self) -> u32 { self.count_geki }
    fn n_katu(&self) -> u32 { self.count_katu }
    fn n300(&self) -> u32 { self.count_300 }
    fn n100(&self) -> u32 { self.count_100 }
    fn n50(&self) -> u32 { self.count_50 }
    fn n_miss(&self) -> u32 { self.count_miss }
    fn passed_objects(&self, mode: GameMode) -> u32 { self.total_hits(mode) }
    fn accuracy(&self, mode: GameMode) -> f32 { self.accuracy(mode) }
}

fn osu_grade_legacy(
    mods: GradeGameModsData,
    stats: &impl LegacyStatistics,
    passed_objects: u32,
) -> Grade {
    if stats.n300() == passed_objects {
        return if mods.hd || mods.fl {
            Grade::XH
        } else {
            Grade::X
        };
    }

    let ratio300 = stats.n300() as f32 / passed_objects as f32;
    let ratio50 = stats.n50() as f32 / passed_objects as f32;

    if ratio300 > 0.9 && ratio50 < 0.01 && stats.n_miss() == 0 {
        if mods.hd || mods.fl {
            Grade::SH
        } else {
            Grade::S
        }
    } else if ratio300 > 0.9 || (ratio300 > 0.8 && stats.n_miss() == 0) {
        Grade::A
    } else if ratio300 > 0.8 || (ratio300 > 0.7 && stats.n_miss() == 0) {
        Grade::B
    } else if ratio300 > 0.6 {
        Grade::C
    } else {
        Grade::D
    }
}

fn taiko_grade_legacy(
    mods: GradeGameModsData,
    stats: &impl LegacyStatistics,
    passed_objects: u32,
) -> Grade {
    if stats.n300() == passed_objects {
        return if mods.hd || mods.fl {
            Grade::XH
        } else {
            Grade::X
        };
    }

    let ratio300 = stats.n300() as f32 / passed_objects as f32;

    if ratio300 > 0.9 && stats.n_miss() == 0 {
        if mods.hd || mods.fl {
            Grade::SH
        } else {
            Grade::S
        }
    } else if ratio300 > 0.9 || (ratio300 > 0.8 && stats.n_miss() == 0) {
        Grade::A
    } else if ratio300 > 0.8 || (ratio300 > 0.7 && stats.n_miss() == 0) {
        Grade::B
    } else if ratio300 > 0.6 {
        Grade::C
    } else {
        Grade::D
    }
}

fn catch_grade_legacy(mods: GradeGameModsData, stats: &impl LegacyStatistics) -> Grade {
    let acc = stats.accuracy(GameMode::Catch);

    if (100.0 - acc).abs() <= f32::EPSILON {
        if mods.hd || mods.fl {
            Grade::XH
        } else {
            Grade::X
        }
    } else if acc > 98.0 {
        if mods.hd || mods.fl {
            Grade::SH
        } else {
            Grade::S
        }
    } else if acc > 94.0 {
        Grade::A
    } else if acc > 90.0 {
        Grade::B
    } else if acc > 85.0 {
        Grade::C
    } else {
        Grade::D
    }
}

fn mania_grade_legacy(
    mods: GradeGameModsData,
    stats: &impl LegacyStatistics,
    passed_objects: u32,
) -> Grade {
    if stats.n_geki() == passed_objects {
        return if mods.hd || mods.fl || mods.fi {
            Grade::XH
        } else {
            Grade::X
        };
    }

    let acc = stats.accuracy(GameMode::Mania);

    if acc > 95.0 {
        if mods.hd || mods.fl || mods.fi {
            Grade::SH
        } else {
            Grade::S
        }
    } else if acc > 90.0 {
        Grade::A
    } else if acc > 80.0 {
        Grade::B
    } else if acc > 70.0 {
        Grade::C
    } else {
        Grade::D
    }
}
