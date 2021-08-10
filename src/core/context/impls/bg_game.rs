use crate::{
    bg_game::GameWrapper, database::MapsetTagWrapper, util::error::BgGameError, BotResult, Context,
};

use std::sync::Arc;
use twilight_model::id::ChannelId;

impl Context {
    pub fn add_game_and_start(
        this: Arc<Context>,
        channel: ChannelId,
        mapsets: Vec<MapsetTagWrapper>,
    ) {
        if this.data.bg_games.get(&channel).is_some() {
            this.data.bg_games.remove(&channel);
        }

        this.data
            .bg_games
            .entry(channel)
            .or_insert_with(GameWrapper::new)
            .start(Arc::clone(&this), channel, mapsets);
    }

    pub fn has_running_game(&self, channel: ChannelId) -> bool {
        self.data
            .bg_games
            .iter()
            .any(|guard| *guard.key() == channel)
    }

    pub fn game_channels(&self) -> Vec<ChannelId> {
        self.data
            .bg_games
            .iter()
            .map(|guard| *guard.key())
            .collect()
    }

    pub async fn restart_game(&self, channel: ChannelId) -> BotResult<bool> {
        match self.data.bg_games.get(&channel) {
            Some(game) => Ok(game.restart().await.map(|_| true)?),
            None => Ok(false),
        }
    }

    pub async fn stop_game(&self, channel: ChannelId) -> BotResult<bool> {
        if self.data.bg_games.contains_key(&channel) {
            if let Some(game) = self.data.bg_games.get(&channel) {
                game.stop().await?;
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn remove_game(&self, channel: ChannelId) {
        self.data.bg_games.remove(&channel);
    }

    pub async fn game_hint(&self, channel: ChannelId) -> Result<String, BgGameError> {
        match self.data.bg_games.get(&channel) {
            Some(game) => match game.hint().await? {
                Some(hint) => Ok(hint),
                None => Err(BgGameError::NotStarted),
            },
            None => Err(BgGameError::NoGame),
        }
    }

    pub async fn game_bigger(&self, channel: ChannelId) -> Result<Vec<u8>, BgGameError> {
        match self.data.bg_games.get(&channel) {
            Some(game) => match game.sub_image().await? {
                Some(img) => Ok(img),
                None => Err(BgGameError::NotStarted),
            },
            None => Err(BgGameError::NoGame),
        }
    }
}
