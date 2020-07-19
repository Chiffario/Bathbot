use super::{Pages, Pagination};

use crate::{embeds::CommonEmbed, BotResult, Context};

use async_trait::async_trait;
use rosu::models::{Beatmap, Score, User};
use std::collections::HashMap;
use twilight::model::{channel::Message, id::UserId};

pub struct CommonPagination {
    msg: Message,
    pages: Pages,
    users: HashMap<u32, User>,
    scores: HashMap<u32, Vec<Score>>,
    maps: HashMap<u32, Beatmap>,
    id_pps: Vec<(u32, f32)>,
    thumbnail: String,
}

impl CommonPagination {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        ctx: &Context,
        msg: Message,
        users: HashMap<u32, User>,
        scores: HashMap<u32, Vec<Score>>,
        maps: HashMap<u32, Beatmap>,
        id_pps: Vec<(u32, f32)>,
        thumbnail: String,
    ) -> Self {
        Self {
            pages: Pages::new(10, scores.len()),
            msg,
            users,
            scores,
            maps,
            id_pps,
            thumbnail,
        }
    }
}

#[async_trait]
impl Pagination for CommonPagination {
    type PageData = CommonEmbed;
    fn msg(&self) -> &Message {
        &self.msg
    }
    fn pages(&self) -> Pages {
        self.pages
    }
    fn pages_mut(&mut self) -> &mut Pages {
        &mut self.pages
    }
    fn thumbnail(&self) -> Option<String> {
        Some(self.thumbnail.clone())
    }
    async fn build_page(&mut self) -> BotResult<Self::PageData> {
        Ok(CommonEmbed::new(
            &self.users,
            &self.scores,
            &self.maps,
            &self.id_pps[self.pages.index..(self.pages.index + 10).min(self.id_pps.len())],
            self.pages.index,
        ))
    }
}