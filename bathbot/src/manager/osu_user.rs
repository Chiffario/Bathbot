use std::{borrow::Cow, collections::HashMap};

use bathbot_model::{RankingEntries, UserModeStatsColumn, UserStatsColumn};
use bathbot_psql::Database;
use bathbot_util::{CowUtils, IntHasher};
use eyre::{Result, WrapErr};
use rosu_v2::prelude::{GameMode, UserExtended, Username};

use crate::core::Context;

#[derive(Copy, Clone)]
pub struct OsuUserManager {
    psql: &'static Database,
}

impl OsuUserManager {
    pub fn new() -> Self {
        Self {
            psql: Context::psql(),
        }
    }

    pub async fn user_id(self, username: &str, alt_username: Option<&str>) -> Result<Option<u32>> {
        let username = username.cow_replace('_', r"\_");

        self.psql
            .select_osu_id_by_osu_name(username.as_ref(), alt_username)
            .await
            .wrap_err("Failed to get osu id")
    }

    pub async fn name(self, user_id: u32) -> Result<Option<Username>> {
        self.psql
            .select_osu_name_by_osu_id(user_id)
            .await
            .wrap_err("Failed to get username")
    }

    pub async fn names(self, user_ids: &[i32]) -> Result<HashMap<u32, Username, IntHasher>> {
        self.psql
            .select_osu_usernames(user_ids)
            .await
            .wrap_err("Failed to get usernames")
    }

    pub async fn ids(&self, names: &[String]) -> Result<HashMap<Username, u32>> {
        let escaped_names = if names.iter().any(|name| name.contains('_')) {
            let names: Vec<_> = names.iter().map(|name| name.replace('_', r"\_")).collect();

            Cow::Owned(names)
        } else {
            Cow::Borrowed(names)
        };

        self.psql
            .select_osu_user_ids(escaped_names.as_ref())
            .await
            .wrap_err("Failed to get user ids")
    }

    pub async fn stats(
        self,
        discord_ids: &[i64],
        column: UserStatsColumn,
        country_code: Option<&str>,
    ) -> Result<RankingEntries> {
        self.psql
            .select_osu_user_stats(discord_ids, column, country_code)
            .await
            .map(RankingEntries::from)
            .wrap_err("Failed to get user stats")
    }

    pub async fn stats_mode(
        self,
        discord_ids: &[i64],
        mode: GameMode,
        column: UserModeStatsColumn,
        country_code: Option<&str>,
    ) -> Result<RankingEntries> {
        self.psql
            .select_osu_user_mode_stats(discord_ids, mode, column, country_code)
            .await
            .map(RankingEntries::from)
            .wrap_err("Failed to get user mode stats")
    }

    pub async fn store(self, user: &UserExtended, mode: GameMode) {
        if let Err(err) = self.psql.upsert_osu_user(user, mode).await {
            warn!(?err, "Failed to upsert osu user");
        }
    }

    pub async fn remove_stats_and_scores(self, user_id: u32) -> Result<()> {
        self.psql
            .delete_osu_user_stats(user_id)
            .await
            .wrap_err("Failed to delete osu user data")
    }
}
