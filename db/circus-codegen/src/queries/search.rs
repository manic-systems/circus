// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct QuickProjectsParams<T1: crate::StringSql> {
    pub pattern: T1,
    pub limit: i64,
}
#[derive(Debug)]
pub struct QuickBuildsParams<T1: crate::StringSql> {
    pub pattern: T1,
    pub limit: i64,
}
#[derive(Debug)]
pub struct SearchProjectsParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub pattern: T1,
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
    pub has_jobsets: Option<bool>,
    pub sort: Option<T2>,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct CountProjectsParams<T1: crate::StringSql> {
    pub pattern: T1,
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
    pub has_jobsets: Option<bool>,
}
#[derive(Debug)]
pub struct SearchJobsetsParams<T1: crate::StringSql> {
    pub pattern: T1,
    pub project_id: Option<uuid::Uuid>,
    pub enabled: Option<bool>,
    pub flake_mode: Option<bool>,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct CountJobsetsParams<T1: crate::StringSql> {
    pub pattern: T1,
    pub project_id: Option<uuid::Uuid>,
    pub enabled: Option<bool>,
    pub flake_mode: Option<bool>,
}
#[derive(Clone, Copy, Debug)]
pub struct SearchEvaluationsParams {
    pub project_id: Option<uuid::Uuid>,
    pub jobset_id: Option<uuid::Uuid>,
    pub has_builds: Option<bool>,
    pub finished_after: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_before: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct CountEvaluationsParams {
    pub project_id: Option<uuid::Uuid>,
    pub jobset_id: Option<uuid::Uuid>,
    pub has_builds: Option<bool>,
    pub finished_after: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_before: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Debug)]
pub struct SearchBuildsParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub pattern: T1,
    pub status: Option<T2>,
    pub project_id: Option<uuid::Uuid>,
    pub jobset_id: Option<uuid::Uuid>,
    pub evaluation_id: Option<uuid::Uuid>,
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
    pub min_priority: Option<i32>,
    pub max_priority: Option<i32>,
    pub sort: Option<T3>,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct CountBuildsParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub pattern: T1,
    pub status: Option<T2>,
    pub project_id: Option<uuid::Uuid>,
    pub jobset_id: Option<uuid::Uuid>,
    pub evaluation_id: Option<uuid::Uuid>,
    pub created_after: Option<chrono::DateTime<chrono::Utc>>,
    pub created_before: Option<chrono::DateTime<chrono::Utc>>,
    pub min_priority: Option<i32>,
    pub max_priority: Option<i32>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectQuickSearchRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub repository_url: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub cache_enabled: bool,
    pub cache_url: Option<String>,
    pub cache_upstreams: serde_json::Value,
}
pub struct ProjectQuickSearchRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub repository_url: &'a str,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub cache_enabled: bool,
    pub cache_url: Option<&'a str>,
    pub cache_upstreams: postgres_types::Json<&'a serde_json::value::RawValue>,
}
impl<'a> From<ProjectQuickSearchRowBorrowed<'a>> for ProjectQuickSearchRow {
    fn from(
        ProjectQuickSearchRowBorrowed {
            id,
            name,
            description,
            repository_url,
            created_at,
            updated_at,
            cache_enabled,
            cache_url,
            cache_upstreams,
        }: ProjectQuickSearchRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            description: description.map(|v| v.into()),
            repository_url: repository_url.into(),
            created_at,
            updated_at,
            cache_enabled,
            cache_url: cache_url.map(|v| v.into()),
            cache_upstreams: serde_json::from_str(cache_upstreams.0.get()).unwrap(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct BuildQuickSearchRow {
    pub id: uuid::Uuid,
    pub evaluation_id: uuid::Uuid,
    pub job_name: String,
    pub drv_path: String,
    pub status: String,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub log_path: Option<String>,
    pub build_output_path: Option<String>,
    pub error_message: Option<String>,
    pub priority: i32,
    pub retry_count: i32,
    pub max_retries: i32,
    pub notification_pending_since: Option<chrono::DateTime<chrono::Utc>>,
    pub outputs: Option<serde_json::Value>,
    pub is_aggregate: bool,
    pub constituents: Option<serde_json::Value>,
    pub builder_id: Option<uuid::Uuid>,
    pub signed: bool,
    pub system: Option<String>,
    pub keep: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub is_fod: bool,
    pub fod_hash: Option<String>,
    pub meta_description: Option<String>,
    pub meta_license: Option<String>,
    pub meta_homepage: Option<String>,
    pub meta_maintainers: Option<String>,
    pub required_features: Vec<String>,
    pub agent_machine_id: Option<uuid::Uuid>,
    pub started_notified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub effective_features: Option<Vec<String>>,
}
pub struct BuildQuickSearchRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub evaluation_id: uuid::Uuid,
    pub job_name: &'a str,
    pub drv_path: &'a str,
    pub status: &'a str,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub log_path: Option<&'a str>,
    pub build_output_path: Option<&'a str>,
    pub error_message: Option<&'a str>,
    pub priority: i32,
    pub retry_count: i32,
    pub max_retries: i32,
    pub notification_pending_since: Option<chrono::DateTime<chrono::Utc>>,
    pub outputs: Option<postgres_types::Json<&'a serde_json::value::RawValue>>,
    pub is_aggregate: bool,
    pub constituents: Option<postgres_types::Json<&'a serde_json::value::RawValue>>,
    pub builder_id: Option<uuid::Uuid>,
    pub signed: bool,
    pub system: Option<&'a str>,
    pub keep: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub is_fod: bool,
    pub fod_hash: Option<&'a str>,
    pub meta_description: Option<&'a str>,
    pub meta_license: Option<&'a str>,
    pub meta_homepage: Option<&'a str>,
    pub meta_maintainers: Option<&'a str>,
    pub required_features: crate::ArrayIterator<'a, &'a str>,
    pub agent_machine_id: Option<uuid::Uuid>,
    pub started_notified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub effective_features: Option<crate::ArrayIterator<'a, &'a str>>,
}
impl<'a> From<BuildQuickSearchRowBorrowed<'a>> for BuildQuickSearchRow {
    fn from(
        BuildQuickSearchRowBorrowed {
            id,
            evaluation_id,
            job_name,
            drv_path,
            status,
            started_at,
            completed_at,
            log_path,
            build_output_path,
            error_message,
            priority,
            retry_count,
            max_retries,
            notification_pending_since,
            outputs,
            is_aggregate,
            constituents,
            builder_id,
            signed,
            system,
            keep,
            created_at,
            is_fod,
            fod_hash,
            meta_description,
            meta_license,
            meta_homepage,
            meta_maintainers,
            required_features,
            agent_machine_id,
            started_notified_at,
            effective_features,
        }: BuildQuickSearchRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            evaluation_id,
            job_name: job_name.into(),
            drv_path: drv_path.into(),
            status: status.into(),
            started_at,
            completed_at,
            log_path: log_path.map(|v| v.into()),
            build_output_path: build_output_path.map(|v| v.into()),
            error_message: error_message.map(|v| v.into()),
            priority,
            retry_count,
            max_retries,
            notification_pending_since,
            outputs: outputs.map(|v| serde_json::from_str(v.0.get()).unwrap()),
            is_aggregate,
            constituents: constituents.map(|v| serde_json::from_str(v.0.get()).unwrap()),
            builder_id,
            signed,
            system: system.map(|v| v.into()),
            keep,
            created_at,
            is_fod,
            fod_hash: fod_hash.map(|v| v.into()),
            meta_description: meta_description.map(|v| v.into()),
            meta_license: meta_license.map(|v| v.into()),
            meta_homepage: meta_homepage.map(|v| v.into()),
            meta_maintainers: meta_maintainers.map(|v| v.into()),
            required_features: required_features.map(|v| v.into()).collect(),
            agent_machine_id,
            started_notified_at,
            effective_features: effective_features.map(|v| v.map(|v| v.into()).collect()),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct JobsetSearchRow {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub name: String,
    pub nix_expression: String,
    pub enabled: bool,
    pub flake_mode: bool,
    pub check_interval: i32,
    pub branch: Option<String>,
    pub scheduling_shares: i32,
    pub state: String,
    pub last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub keep_nr: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub trigger_mode: String,
    pub branch_pattern: Option<String>,
    pub tag_pattern: Option<String>,
    pub systems: Option<Vec<String>>,
    pub only_build_latest: bool,
}
pub struct JobsetSearchRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub name: &'a str,
    pub nix_expression: &'a str,
    pub enabled: bool,
    pub flake_mode: bool,
    pub check_interval: i32,
    pub branch: Option<&'a str>,
    pub scheduling_shares: i32,
    pub state: &'a str,
    pub last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub keep_nr: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub trigger_mode: &'a str,
    pub branch_pattern: Option<&'a str>,
    pub tag_pattern: Option<&'a str>,
    pub systems: Option<crate::ArrayIterator<'a, &'a str>>,
    pub only_build_latest: bool,
}
impl<'a> From<JobsetSearchRowBorrowed<'a>> for JobsetSearchRow {
    fn from(
        JobsetSearchRowBorrowed {
            id,
            project_id,
            name,
            nix_expression,
            enabled,
            flake_mode,
            check_interval,
            branch,
            scheduling_shares,
            state,
            last_checked_at,
            keep_nr,
            created_at,
            updated_at,
            trigger_mode,
            branch_pattern,
            tag_pattern,
            systems,
            only_build_latest,
        }: JobsetSearchRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            project_id,
            name: name.into(),
            nix_expression: nix_expression.into(),
            enabled,
            flake_mode,
            check_interval,
            branch: branch.map(|v| v.into()),
            scheduling_shares,
            state: state.into(),
            last_checked_at,
            keep_nr,
            created_at,
            updated_at,
            trigger_mode: trigger_mode.into(),
            branch_pattern: branch_pattern.map(|v| v.into()),
            tag_pattern: tag_pattern.map(|v| v.into()),
            systems: systems.map(|v| v.map(|v| v.into()).collect()),
            only_build_latest,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationSearchRow {
    pub id: uuid::Uuid,
    pub jobset_id: uuid::Uuid,
    pub commit_hash: String,
    pub evaluation_time: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub error_message: Option<String>,
    pub inputs_hash: Option<String>,
    pub pr_number: Option<i32>,
    pub pr_head_branch: Option<String>,
    pub pr_base_branch: Option<String>,
    pub pr_action: Option<String>,
    pub trigger_kind: String,
    pub hidden: bool,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub orphaned_count: i32,
    pub source_scope: Option<String>,
    pub superseded_by: Option<uuid::Uuid>,
}
pub struct EvaluationSearchRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub jobset_id: uuid::Uuid,
    pub commit_hash: &'a str,
    pub evaluation_time: chrono::DateTime<chrono::Utc>,
    pub status: &'a str,
    pub error_message: Option<&'a str>,
    pub inputs_hash: Option<&'a str>,
    pub pr_number: Option<i32>,
    pub pr_head_branch: Option<&'a str>,
    pub pr_base_branch: Option<&'a str>,
    pub pr_action: Option<&'a str>,
    pub trigger_kind: &'a str,
    pub hidden: bool,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub orphaned_count: i32,
    pub source_scope: Option<&'a str>,
    pub superseded_by: Option<uuid::Uuid>,
}
impl<'a> From<EvaluationSearchRowBorrowed<'a>> for EvaluationSearchRow {
    fn from(
        EvaluationSearchRowBorrowed {
            id,
            jobset_id,
            commit_hash,
            evaluation_time,
            status,
            error_message,
            inputs_hash,
            pr_number,
            pr_head_branch,
            pr_base_branch,
            pr_action,
            trigger_kind,
            hidden,
            started_at,
            orphaned_count,
            source_scope,
            superseded_by,
        }: EvaluationSearchRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            jobset_id,
            commit_hash: commit_hash.into(),
            evaluation_time,
            status: status.into(),
            error_message: error_message.map(|v| v.into()),
            inputs_hash: inputs_hash.map(|v| v.into()),
            pr_number,
            pr_head_branch: pr_head_branch.map(|v| v.into()),
            pr_base_branch: pr_base_branch.map(|v| v.into()),
            pr_action: pr_action.map(|v| v.into()),
            trigger_kind: trigger_kind.into(),
            hidden,
            started_at,
            orphaned_count,
            source_scope: source_scope.map(|v| v.into()),
            superseded_by,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct ProjectQuickSearchRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<ProjectQuickSearchRowBorrowed, tokio_postgres::Error>,
    mapper: fn(ProjectQuickSearchRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ProjectQuickSearchRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(ProjectQuickSearchRowBorrowed) -> R,
    ) -> ProjectQuickSearchRowQuery<'c, 'a, 's, C, R, N> {
        ProjectQuickSearchRowQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct BuildQuickSearchRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<BuildQuickSearchRowBorrowed, tokio_postgres::Error>,
    mapper: fn(BuildQuickSearchRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BuildQuickSearchRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(BuildQuickSearchRowBorrowed) -> R,
    ) -> BuildQuickSearchRowQuery<'c, 'a, 's, C, R, N> {
        BuildQuickSearchRowQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct I64Query<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<i64, tokio_postgres::Error>,
    mapper: fn(i64) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> I64Query<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(i64) -> R) -> I64Query<'c, 'a, 's, C, R, N> {
        I64Query {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct JobsetSearchRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<JobsetSearchRowBorrowed, tokio_postgres::Error>,
    mapper: fn(JobsetSearchRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> JobsetSearchRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(JobsetSearchRowBorrowed) -> R,
    ) -> JobsetSearchRowQuery<'c, 'a, 's, C, R, N> {
        JobsetSearchRowQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct EvaluationSearchRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<EvaluationSearchRowBorrowed, tokio_postgres::Error>,
    mapper: fn(EvaluationSearchRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> EvaluationSearchRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(EvaluationSearchRowBorrowed) -> R,
    ) -> EvaluationSearchRowQuery<'c, 'a, 's, C, R, N> {
        EvaluationSearchRowQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub async fn one(self) -> Result<T, tokio_postgres::Error> {
        let row =
            crate::client::async_::one(self.client, self.query, &self.params, self.cached).await?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub async fn all(self) -> Result<Vec<T>, tokio_postgres::Error> {
        self.iter().await?.try_collect().await
    }
    pub async fn opt(self) -> Result<Option<T>, tokio_postgres::Error> {
        let opt_row =
            crate::client::async_::opt(self.client, self.query, &self.params, self.cached).await?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub async fn iter(
        self,
    ) -> Result<
        impl futures::Stream<Item = Result<T, tokio_postgres::Error>> + 'c,
        tokio_postgres::Error,
    > {
        let stream = crate::client::async_::raw(
            self.client,
            self.query,
            crate::slice_iter(&self.params),
            self.cached,
        )
        .await?;
        let mapped = stream
            .map(move |res| {
                res.and_then(|row| {
                    let extracted = (self.extractor)(&row)?;
                    Ok((self.mapper)(extracted))
                })
            })
            .into_stream();
        Ok(mapped)
    }
}
pub struct QuickProjectsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn quick_projects() -> QuickProjectsStmt {
    QuickProjectsStmt(
        "SELECT * FROM projects WHERE name ILIKE $1 OR description ILIKE $1 ORDER BY name LIMIT $2",
        None,
    )
}
impl QuickProjectsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        pattern: &'a T1,
        limit: &'a i64,
    ) -> ProjectQuickSearchRowQuery<'c, 'a, 's, C, ProjectQuickSearchRow, 2> {
        ProjectQuickSearchRowQuery {
            client,
            params: [pattern, limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ProjectQuickSearchRowBorrowed, tokio_postgres::Error> {
                Ok(ProjectQuickSearchRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    description: row.try_get(2)?,
                    repository_url: row.try_get(3)?,
                    created_at: row.try_get(4)?,
                    updated_at: row.try_get(5)?,
                    cache_enabled: row.try_get(6)?,
                    cache_url: row.try_get(7)?,
                    cache_upstreams: row.try_get(8)?,
                })
            },
            mapper: |it| ProjectQuickSearchRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        QuickProjectsParams<T1>,
        ProjectQuickSearchRowQuery<'c, 'a, 's, C, ProjectQuickSearchRow, 2>,
        C,
    > for QuickProjectsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a QuickProjectsParams<T1>,
    ) -> ProjectQuickSearchRowQuery<'c, 'a, 's, C, ProjectQuickSearchRow, 2> {
        self.bind(client, &params.pattern, &params.limit)
    }
}
pub struct QuickBuildsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn quick_builds() -> QuickBuildsStmt {
    QuickBuildsStmt(
        "SELECT * FROM builds WHERE job_name ILIKE $1 OR drv_path ILIKE $1 ORDER BY created_at DESC LIMIT $2",
        None,
    )
}
impl QuickBuildsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        pattern: &'a T1,
        limit: &'a i64,
    ) -> BuildQuickSearchRowQuery<'c, 'a, 's, C, BuildQuickSearchRow, 2> {
        BuildQuickSearchRowQuery {
            client,
            params: [pattern, limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuildQuickSearchRowBorrowed, tokio_postgres::Error> {
                Ok(BuildQuickSearchRowBorrowed {
                    id: row.try_get(0)?,
                    evaluation_id: row.try_get(1)?,
                    job_name: row.try_get(2)?,
                    drv_path: row.try_get(3)?,
                    status: row.try_get(4)?,
                    started_at: row.try_get(5)?,
                    completed_at: row.try_get(6)?,
                    log_path: row.try_get(7)?,
                    build_output_path: row.try_get(8)?,
                    error_message: row.try_get(9)?,
                    priority: row.try_get(10)?,
                    retry_count: row.try_get(11)?,
                    max_retries: row.try_get(12)?,
                    notification_pending_since: row.try_get(13)?,
                    outputs: row.try_get(14)?,
                    is_aggregate: row.try_get(15)?,
                    constituents: row.try_get(16)?,
                    builder_id: row.try_get(17)?,
                    signed: row.try_get(18)?,
                    system: row.try_get(19)?,
                    keep: row.try_get(20)?,
                    created_at: row.try_get(21)?,
                    is_fod: row.try_get(22)?,
                    fod_hash: row.try_get(23)?,
                    meta_description: row.try_get(24)?,
                    meta_license: row.try_get(25)?,
                    meta_homepage: row.try_get(26)?,
                    meta_maintainers: row.try_get(27)?,
                    required_features: row.try_get(28)?,
                    agent_machine_id: row.try_get(29)?,
                    started_notified_at: row.try_get(30)?,
                    effective_features: row.try_get(31)?,
                })
            },
            mapper: |it| BuildQuickSearchRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        QuickBuildsParams<T1>,
        BuildQuickSearchRowQuery<'c, 'a, 's, C, BuildQuickSearchRow, 2>,
        C,
    > for QuickBuildsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a QuickBuildsParams<T1>,
    ) -> BuildQuickSearchRowQuery<'c, 'a, 's, C, BuildQuickSearchRow, 2> {
        self.bind(client, &params.pattern, &params.limit)
    }
}
pub struct SearchProjectsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn search_projects() -> SearchProjectsStmt {
    SearchProjectsStmt(
        "SELECT p.* FROM projects p WHERE (p.name ILIKE $1 OR p.description ILIKE $1) AND ($2::timestamptz IS NULL OR p.created_at >= $2) AND ($3::timestamptz IS NULL OR p.created_at <= $3) AND ($4::bool IS NULL OR $4 = EXISTS (SELECT 1 FROM jobsets j WHERE j.project_id = p.id)) ORDER BY CASE WHEN $5 = 'name_asc' THEN p.name END ASC, CASE WHEN $5 = 'name_desc' THEN p.name END DESC, CASE WHEN $5 = 'created_at_asc' THEN p.created_at END ASC, CASE WHEN $5 = 'created_at_desc' THEN p.created_at END DESC, p.name ASC LIMIT $6 OFFSET $7",
        None,
    )
}
impl SearchProjectsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        pattern: &'a T1,
        created_after: &'a Option<chrono::DateTime<chrono::Utc>>,
        created_before: &'a Option<chrono::DateTime<chrono::Utc>>,
        has_jobsets: &'a Option<bool>,
        sort: &'a Option<T2>,
        limit: &'a i64,
        offset: &'a i64,
    ) -> ProjectQuickSearchRowQuery<'c, 'a, 's, C, ProjectQuickSearchRow, 7> {
        ProjectQuickSearchRowQuery {
            client,
            params: [
                pattern,
                created_after,
                created_before,
                has_jobsets,
                sort,
                limit,
                offset,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ProjectQuickSearchRowBorrowed, tokio_postgres::Error> {
                Ok(ProjectQuickSearchRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    description: row.try_get(2)?,
                    repository_url: row.try_get(3)?,
                    created_at: row.try_get(4)?,
                    updated_at: row.try_get(5)?,
                    cache_enabled: row.try_get(6)?,
                    cache_url: row.try_get(7)?,
                    cache_upstreams: row.try_get(8)?,
                })
            },
            mapper: |it| ProjectQuickSearchRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        SearchProjectsParams<T1, T2>,
        ProjectQuickSearchRowQuery<'c, 'a, 's, C, ProjectQuickSearchRow, 7>,
        C,
    > for SearchProjectsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a SearchProjectsParams<T1, T2>,
    ) -> ProjectQuickSearchRowQuery<'c, 'a, 's, C, ProjectQuickSearchRow, 7> {
        self.bind(
            client,
            &params.pattern,
            &params.created_after,
            &params.created_before,
            &params.has_jobsets,
            &params.sort,
            &params.limit,
            &params.offset,
        )
    }
}
pub struct CountProjectsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_projects() -> CountProjectsStmt {
    CountProjectsStmt(
        "SELECT COUNT(*) FROM projects p WHERE (p.name ILIKE $1 OR p.description ILIKE $1) AND ($2::timestamptz IS NULL OR p.created_at >= $2) AND ($3::timestamptz IS NULL OR p.created_at <= $3) AND ($4::bool IS NULL OR $4 = EXISTS (SELECT 1 FROM jobsets j WHERE j.project_id = p.id))",
        None,
    )
}
impl CountProjectsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        pattern: &'a T1,
        created_after: &'a Option<chrono::DateTime<chrono::Utc>>,
        created_before: &'a Option<chrono::DateTime<chrono::Utc>>,
        has_jobsets: &'a Option<bool>,
    ) -> I64Query<'c, 'a, 's, C, i64, 4> {
        I64Query {
            client,
            params: [pattern, created_after, created_before, has_jobsets],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CountProjectsParams<T1>,
        I64Query<'c, 'a, 's, C, i64, 4>,
        C,
    > for CountProjectsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CountProjectsParams<T1>,
    ) -> I64Query<'c, 'a, 's, C, i64, 4> {
        self.bind(
            client,
            &params.pattern,
            &params.created_after,
            &params.created_before,
            &params.has_jobsets,
        )
    }
}
pub struct SearchJobsetsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn search_jobsets() -> SearchJobsetsStmt {
    SearchJobsetsStmt(
        "SELECT * FROM jobsets WHERE name ILIKE $1 AND ($2::uuid IS NULL OR project_id = $2) AND ($3::bool IS NULL OR enabled = $3) AND ($4::bool IS NULL OR flake_mode = $4) ORDER BY name ASC LIMIT $5 OFFSET $6",
        None,
    )
}
impl SearchJobsetsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        pattern: &'a T1,
        project_id: &'a Option<uuid::Uuid>,
        enabled: &'a Option<bool>,
        flake_mode: &'a Option<bool>,
        limit: &'a i64,
        offset: &'a i64,
    ) -> JobsetSearchRowQuery<'c, 'a, 's, C, JobsetSearchRow, 6> {
        JobsetSearchRowQuery {
            client,
            params: [pattern, project_id, enabled, flake_mode, limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<JobsetSearchRowBorrowed, tokio_postgres::Error> {
                Ok(JobsetSearchRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    name: row.try_get(2)?,
                    nix_expression: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    flake_mode: row.try_get(5)?,
                    check_interval: row.try_get(6)?,
                    branch: row.try_get(7)?,
                    scheduling_shares: row.try_get(8)?,
                    state: row.try_get(9)?,
                    last_checked_at: row.try_get(10)?,
                    keep_nr: row.try_get(11)?,
                    created_at: row.try_get(12)?,
                    updated_at: row.try_get(13)?,
                    trigger_mode: row.try_get(14)?,
                    branch_pattern: row.try_get(15)?,
                    tag_pattern: row.try_get(16)?,
                    systems: row.try_get(17)?,
                    only_build_latest: row.try_get(18)?,
                })
            },
            mapper: |it| JobsetSearchRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        SearchJobsetsParams<T1>,
        JobsetSearchRowQuery<'c, 'a, 's, C, JobsetSearchRow, 6>,
        C,
    > for SearchJobsetsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a SearchJobsetsParams<T1>,
    ) -> JobsetSearchRowQuery<'c, 'a, 's, C, JobsetSearchRow, 6> {
        self.bind(
            client,
            &params.pattern,
            &params.project_id,
            &params.enabled,
            &params.flake_mode,
            &params.limit,
            &params.offset,
        )
    }
}
pub struct CountJobsetsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_jobsets() -> CountJobsetsStmt {
    CountJobsetsStmt(
        "SELECT COUNT(*) FROM jobsets WHERE name ILIKE $1 AND ($2::uuid IS NULL OR project_id = $2) AND ($3::bool IS NULL OR enabled = $3) AND ($4::bool IS NULL OR flake_mode = $4)",
        None,
    )
}
impl CountJobsetsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        pattern: &'a T1,
        project_id: &'a Option<uuid::Uuid>,
        enabled: &'a Option<bool>,
        flake_mode: &'a Option<bool>,
    ) -> I64Query<'c, 'a, 's, C, i64, 4> {
        I64Query {
            client,
            params: [pattern, project_id, enabled, flake_mode],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CountJobsetsParams<T1>,
        I64Query<'c, 'a, 's, C, i64, 4>,
        C,
    > for CountJobsetsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CountJobsetsParams<T1>,
    ) -> I64Query<'c, 'a, 's, C, i64, 4> {
        self.bind(
            client,
            &params.pattern,
            &params.project_id,
            &params.enabled,
            &params.flake_mode,
        )
    }
}
pub struct SearchEvaluationsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn search_evaluations() -> SearchEvaluationsStmt {
    SearchEvaluationsStmt(
        "SELECT e.* FROM evaluations e JOIN jobsets j ON j.id = e.jobset_id WHERE ($1::uuid IS NULL OR j.project_id = $1) AND ($2::uuid IS NULL OR e.jobset_id = $2) AND ($3::bool IS NULL OR $3 = EXISTS (SELECT 1 FROM builds b WHERE b.evaluation_id = e.id)) AND ($4::timestamptz IS NULL OR e.evaluation_time >= $4) AND ($5::timestamptz IS NULL OR e.evaluation_time <= $5) ORDER BY e.evaluation_time DESC LIMIT $6 OFFSET $7",
        None,
    )
}
impl SearchEvaluationsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        project_id: &'a Option<uuid::Uuid>,
        jobset_id: &'a Option<uuid::Uuid>,
        has_builds: &'a Option<bool>,
        finished_after: &'a Option<chrono::DateTime<chrono::Utc>>,
        finished_before: &'a Option<chrono::DateTime<chrono::Utc>>,
        limit: &'a i64,
        offset: &'a i64,
    ) -> EvaluationSearchRowQuery<'c, 'a, 's, C, EvaluationSearchRow, 7> {
        EvaluationSearchRowQuery {
            client,
            params: [
                project_id,
                jobset_id,
                has_builds,
                finished_after,
                finished_before,
                limit,
                offset,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<EvaluationSearchRowBorrowed, tokio_postgres::Error> {
                Ok(EvaluationSearchRowBorrowed {
                    id: row.try_get(0)?,
                    jobset_id: row.try_get(1)?,
                    commit_hash: row.try_get(2)?,
                    evaluation_time: row.try_get(3)?,
                    status: row.try_get(4)?,
                    error_message: row.try_get(5)?,
                    inputs_hash: row.try_get(6)?,
                    pr_number: row.try_get(7)?,
                    pr_head_branch: row.try_get(8)?,
                    pr_base_branch: row.try_get(9)?,
                    pr_action: row.try_get(10)?,
                    trigger_kind: row.try_get(11)?,
                    hidden: row.try_get(12)?,
                    started_at: row.try_get(13)?,
                    orphaned_count: row.try_get(14)?,
                    source_scope: row.try_get(15)?,
                    superseded_by: row.try_get(16)?,
                })
            },
            mapper: |it| EvaluationSearchRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        SearchEvaluationsParams,
        EvaluationSearchRowQuery<'c, 'a, 's, C, EvaluationSearchRow, 7>,
        C,
    > for SearchEvaluationsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a SearchEvaluationsParams,
    ) -> EvaluationSearchRowQuery<'c, 'a, 's, C, EvaluationSearchRow, 7> {
        self.bind(
            client,
            &params.project_id,
            &params.jobset_id,
            &params.has_builds,
            &params.finished_after,
            &params.finished_before,
            &params.limit,
            &params.offset,
        )
    }
}
pub struct CountEvaluationsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_evaluations() -> CountEvaluationsStmt {
    CountEvaluationsStmt(
        "SELECT COUNT(*) FROM evaluations e JOIN jobsets j ON j.id = e.jobset_id WHERE ($1::uuid IS NULL OR j.project_id = $1) AND ($2::uuid IS NULL OR e.jobset_id = $2) AND ($3::bool IS NULL OR $3 = EXISTS (SELECT 1 FROM builds b WHERE b.evaluation_id = e.id)) AND ($4::timestamptz IS NULL OR e.evaluation_time >= $4) AND ($5::timestamptz IS NULL OR e.evaluation_time <= $5)",
        None,
    )
}
impl CountEvaluationsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        project_id: &'a Option<uuid::Uuid>,
        jobset_id: &'a Option<uuid::Uuid>,
        has_builds: &'a Option<bool>,
        finished_after: &'a Option<chrono::DateTime<chrono::Utc>>,
        finished_before: &'a Option<chrono::DateTime<chrono::Utc>>,
    ) -> I64Query<'c, 'a, 's, C, i64, 5> {
        I64Query {
            client,
            params: [
                project_id,
                jobset_id,
                has_builds,
                finished_after,
                finished_before,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CountEvaluationsParams,
        I64Query<'c, 'a, 's, C, i64, 5>,
        C,
    > for CountEvaluationsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CountEvaluationsParams,
    ) -> I64Query<'c, 'a, 's, C, i64, 5> {
        self.bind(
            client,
            &params.project_id,
            &params.jobset_id,
            &params.has_builds,
            &params.finished_after,
            &params.finished_before,
        )
    }
}
pub struct SearchBuildsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn search_builds() -> SearchBuildsStmt {
    SearchBuildsStmt(
        "SELECT b.* FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE (b.job_name ILIKE $1 OR b.drv_path ILIKE $1) AND ($2::text IS NULL OR b.status = $2) AND ($3::uuid IS NULL OR j.project_id = $3) AND ($4::uuid IS NULL OR e.jobset_id = $4) AND ($5::uuid IS NULL OR b.evaluation_id = $5) AND ($6::timestamptz IS NULL OR b.created_at >= $6) AND ($7::timestamptz IS NULL OR b.created_at <= $7) AND ($8::int IS NULL OR b.priority >= $8) AND ($9::int IS NULL OR b.priority <= $9) ORDER BY CASE WHEN $10 = 'created_at_asc' THEN b.created_at END ASC, CASE WHEN $10 = 'created_at_desc' THEN b.created_at END DESC, CASE WHEN $10 = 'job_name_asc' THEN b.job_name END ASC, CASE WHEN $10 = 'job_name_desc' THEN b.job_name END DESC, CASE WHEN $10 = 'status_asc' THEN b.status END ASC, CASE WHEN $10 = 'status_desc' THEN b.status END DESC, CASE WHEN $10 = 'priority_asc' THEN b.priority END ASC, CASE WHEN $10 = 'priority_desc' THEN b.priority END DESC, b.created_at DESC LIMIT $11 OFFSET $12",
        None,
    )
}
impl SearchBuildsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        pattern: &'a T1,
        status: &'a Option<T2>,
        project_id: &'a Option<uuid::Uuid>,
        jobset_id: &'a Option<uuid::Uuid>,
        evaluation_id: &'a Option<uuid::Uuid>,
        created_after: &'a Option<chrono::DateTime<chrono::Utc>>,
        created_before: &'a Option<chrono::DateTime<chrono::Utc>>,
        min_priority: &'a Option<i32>,
        max_priority: &'a Option<i32>,
        sort: &'a Option<T3>,
        limit: &'a i64,
        offset: &'a i64,
    ) -> BuildQuickSearchRowQuery<'c, 'a, 's, C, BuildQuickSearchRow, 12> {
        BuildQuickSearchRowQuery {
            client,
            params: [
                pattern,
                status,
                project_id,
                jobset_id,
                evaluation_id,
                created_after,
                created_before,
                min_priority,
                max_priority,
                sort,
                limit,
                offset,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuildQuickSearchRowBorrowed, tokio_postgres::Error> {
                Ok(BuildQuickSearchRowBorrowed {
                    id: row.try_get(0)?,
                    evaluation_id: row.try_get(1)?,
                    job_name: row.try_get(2)?,
                    drv_path: row.try_get(3)?,
                    status: row.try_get(4)?,
                    started_at: row.try_get(5)?,
                    completed_at: row.try_get(6)?,
                    log_path: row.try_get(7)?,
                    build_output_path: row.try_get(8)?,
                    error_message: row.try_get(9)?,
                    priority: row.try_get(10)?,
                    retry_count: row.try_get(11)?,
                    max_retries: row.try_get(12)?,
                    notification_pending_since: row.try_get(13)?,
                    outputs: row.try_get(14)?,
                    is_aggregate: row.try_get(15)?,
                    constituents: row.try_get(16)?,
                    builder_id: row.try_get(17)?,
                    signed: row.try_get(18)?,
                    system: row.try_get(19)?,
                    keep: row.try_get(20)?,
                    created_at: row.try_get(21)?,
                    is_fod: row.try_get(22)?,
                    fod_hash: row.try_get(23)?,
                    meta_description: row.try_get(24)?,
                    meta_license: row.try_get(25)?,
                    meta_homepage: row.try_get(26)?,
                    meta_maintainers: row.try_get(27)?,
                    required_features: row.try_get(28)?,
                    agent_machine_id: row.try_get(29)?,
                    started_notified_at: row.try_get(30)?,
                    effective_features: row.try_get(31)?,
                })
            },
            mapper: |it| BuildQuickSearchRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        SearchBuildsParams<T1, T2, T3>,
        BuildQuickSearchRowQuery<'c, 'a, 's, C, BuildQuickSearchRow, 12>,
        C,
    > for SearchBuildsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a SearchBuildsParams<T1, T2, T3>,
    ) -> BuildQuickSearchRowQuery<'c, 'a, 's, C, BuildQuickSearchRow, 12> {
        self.bind(
            client,
            &params.pattern,
            &params.status,
            &params.project_id,
            &params.jobset_id,
            &params.evaluation_id,
            &params.created_after,
            &params.created_before,
            &params.min_priority,
            &params.max_priority,
            &params.sort,
            &params.limit,
            &params.offset,
        )
    }
}
pub struct CountBuildsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_builds() -> CountBuildsStmt {
    CountBuildsStmt(
        "SELECT COUNT(*) FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE (b.job_name ILIKE $1 OR b.drv_path ILIKE $1) AND ($2::text IS NULL OR b.status = $2) AND ($3::uuid IS NULL OR j.project_id = $3) AND ($4::uuid IS NULL OR e.jobset_id = $4) AND ($5::uuid IS NULL OR b.evaluation_id = $5) AND ($6::timestamptz IS NULL OR b.created_at >= $6) AND ($7::timestamptz IS NULL OR b.created_at <= $7) AND ($8::int IS NULL OR b.priority >= $8) AND ($9::int IS NULL OR b.priority <= $9)",
        None,
    )
}
impl CountBuildsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        pattern: &'a T1,
        status: &'a Option<T2>,
        project_id: &'a Option<uuid::Uuid>,
        jobset_id: &'a Option<uuid::Uuid>,
        evaluation_id: &'a Option<uuid::Uuid>,
        created_after: &'a Option<chrono::DateTime<chrono::Utc>>,
        created_before: &'a Option<chrono::DateTime<chrono::Utc>>,
        min_priority: &'a Option<i32>,
        max_priority: &'a Option<i32>,
    ) -> I64Query<'c, 'a, 's, C, i64, 9> {
        I64Query {
            client,
            params: [
                pattern,
                status,
                project_id,
                jobset_id,
                evaluation_id,
                created_after,
                created_before,
                min_priority,
                max_priority,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CountBuildsParams<T1, T2>,
        I64Query<'c, 'a, 's, C, i64, 9>,
        C,
    > for CountBuildsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CountBuildsParams<T1, T2>,
    ) -> I64Query<'c, 'a, 's, C, i64, 9> {
        self.bind(
            client,
            &params.pattern,
            &params.status,
            &params.project_id,
            &params.jobset_id,
            &params.evaluation_id,
            &params.created_after,
            &params.created_before,
            &params.min_priority,
            &params.max_priority,
        )
    }
}
