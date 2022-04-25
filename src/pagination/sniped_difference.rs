use std::sync::Arc;

use command_macros::BasePagination;
use hashbrown::HashMap;
use rosu_pp::Beatmap;
use rosu_v2::model::user::User;
use twilight_model::channel::Message;

use crate::{
    commands::osu::Difference, core::Context, custom_client::SnipeRecent, embeds::SnipedDiffEmbed,
    BotResult,
};

use super::{Pages, Pagination};

#[derive(BasePagination)]
#[pagination(single_step = 5)]
pub struct SnipedDiffPagination {
    ctx: Arc<Context>,
    msg: Message,
    pages: Pages,
    user: User,
    diff: Difference,
    scores: Vec<SnipeRecent>,
    maps: HashMap<u32, Beatmap>,
}

impl SnipedDiffPagination {
    pub fn new(
        msg: Message,
        user: User,
        diff: Difference,
        scores: Vec<SnipeRecent>,
        maps: HashMap<u32, Beatmap>,
        ctx: Arc<Context>,
    ) -> Self {
        Self {
            pages: Pages::new(5, scores.len()),
            msg,
            user,
            diff,
            scores,
            maps,
            ctx,
        }
    }
}

#[async_trait]
impl Pagination for SnipedDiffPagination {
    type PageData = SnipedDiffEmbed;

    async fn build_page(&mut self) -> BotResult<Self::PageData> {
        SnipedDiffEmbed::new(
            &self.user,
            self.diff,
            &self.scores,
            self.pages.index,
            (self.page(), self.pages.total_pages),
            &mut self.maps,
            &self.ctx,
        )
        .await
    }
}
