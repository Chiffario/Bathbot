use bathbot_util::numbers::MinMaxAvg;
use eyre::Result;
use rosu_v2::prelude::Score;

use super::ProfileMenu;
use crate::core::Context;

pub(super) struct Top100Stats {
    pub acc: MinMaxAvg<f32>,
    pub combo: MinMaxAvg<u32>,
    pub misses: MinMaxAvg<u32>,
    pub pp: MinMaxAvg<f32>,
    pub stars: MinMaxAvg<f64>,
    pub ar: MinMaxAvg<f64>,
    pub cs: MinMaxAvg<f64>,
    pub hp: MinMaxAvg<f64>,
    pub od: MinMaxAvg<f64>,
    pub bpm: MinMaxAvg<f32>,
    pub len: MinMaxAvg<f32>,
}

impl Top100Stats {
    pub(super) async fn prepare(menu: &mut ProfileMenu) -> Option<&Self> {
        if let Some(ref stats) = menu.top100stats {
            return Some(stats);
        }

        let user_id = menu.user.user_id.to_native();
        let mode = menu.user.mode;
        let scores = menu.scores.get(user_id, mode, menu.legacy_scores).await?;

        match Self::new(scores).await {
            Ok(stats) => Some(menu.top100stats.insert(stats)),
            Err(err) => {
                warn!(?err, "Failed to calculate top100 stats");

                None
            }
        }
    }

    async fn new(scores: &[Score]) -> Result<Self> {
        let maps_id_checksum = scores
            .iter()
            .map(|score| {
                let checksum = score.map.as_ref().and_then(|map| map.checksum.as_deref());

                (score.map_id as i32, checksum)
            })
            .collect();

        let maps = Context::osu_map().maps(&maps_id_checksum).await?;

        let mut this = Self {
            acc: MinMaxAvg::new(),
            combo: MinMaxAvg::new(),
            misses: MinMaxAvg::new(),
            pp: MinMaxAvg::new(),
            stars: MinMaxAvg::new(),
            ar: MinMaxAvg::new(),
            cs: MinMaxAvg::new(),
            hp: MinMaxAvg::new(),
            od: MinMaxAvg::new(),
            bpm: MinMaxAvg::new(),
            len: MinMaxAvg::new(),
        };

        for score in scores {
            let map = score
                .map
                .as_ref()
                .and_then(|map| maps.get(&map.map_id))
                .expect("missing map");

            let mut calc = Context::pp(map).mode(score.mode).mods(score.mods.clone());

            let Some(difficulty) = calc.difficulty().await else {
                continue;
            };

            this.acc.add(score.accuracy);
            this.combo.add(score.max_combo);
            this.misses.add(score.statistics.miss);

            let stars = difficulty.stars();
            this.stars.add(stars);

            let pp = match score.pp {
                Some(pp) => pp,
                None => match calc.score(score).performance().await {
                    Some(attrs) => attrs.pp() as f32,
                    // technically unreachable because we already got
                    // difficulty attributes earlier
                    None => 0.0,
                },
            };

            this.pp.add(pp);

            let map_attrs = map.attributes().mods(score.mods.clone()).build();

            this.ar.add(map_attrs.ar);
            this.cs.add(map_attrs.cs);
            this.hp.add(map_attrs.hp);
            this.od.add(map_attrs.od);
            this.bpm.add(map.bpm() * map_attrs.clock_rate as f32);
            this.len
                .add(map.seconds_drain() as f32 / map_attrs.clock_rate as f32);
        }

        Ok(this)
    }
}
