// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::JsonSql,
    T5: crate::JsonSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
    T8: crate::StringSql,
    T9: crate::StringSql,
    T10: crate::StringSql,
    T11: crate::StringSql,
    T12: crate::ArraySql<Item = T11>,
> {
    pub evaluation_id: uuid::Uuid,
    pub job_name: T1,
    pub drv_path: T2,
    pub system: Option<T3>,
    pub outputs: Option<T4>,
    pub is_aggregate: bool,
    pub constituents: Option<T5>,
    pub is_fod: bool,
    pub fod_hash: Option<T6>,
    pub meta_description: Option<T7>,
    pub meta_license: Option<T8>,
    pub meta_homepage: Option<T9>,
    pub meta_maintainers: Option<T10>,
    pub required_features: T12,
}
#[derive(Debug)]
pub struct ListForJobsetEvaluationsParams<T1: crate::ArraySql<Item = uuid::Uuid>> {
    pub jobset_id: uuid::Uuid,
    pub evaluation_ids: T1,
}
#[derive(Clone, Copy, Debug)]
pub struct ListPendingParams {
    pub schedulable_capacity: i32,
    pub limit: i64,
}
#[derive(Debug)]
pub struct CompleteParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
> {
    pub status: T1,
    pub log_path: Option<T2>,
    pub build_output_path: Option<T3>,
    pub error_message: Option<T4>,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct ListPendingInSchedulerOrderParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub system: Option<T1>,
    pub job_name: Option<T2>,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct BumpPriorityParams {
    pub delta: i32,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct ResetOrphanedParams<T1: crate::ArraySql<Item = uuid::Uuid>> {
    pub older_than_secs: i64,
    pub excluded_ids: T1,
}
#[derive(Debug)]
pub struct ListFilteredParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub evaluation_id: Option<uuid::Uuid>,
    pub status: Option<T1>,
    pub system: Option<T2>,
    pub job_name: Option<T3>,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct CountFilteredParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub evaluation_id: Option<uuid::Uuid>,
    pub status: Option<T1>,
    pub system: Option<T2>,
    pub job_name: Option<T3>,
}
#[derive(Debug)]
pub struct SetEffectiveFeaturesParams<T1: crate::StringSql, T2: crate::ArraySql<Item = T1>> {
    pub features: T2,
    pub id: uuid::Uuid,
}
#[derive(Clone, Copy, Debug)]
pub struct SetKeepParams {
    pub keep: bool,
    pub id: uuid::Uuid,
}
#[derive(Clone, Copy, Debug)]
pub struct SetBuilderParams {
    pub builder_id: uuid::Uuid,
    pub id: uuid::Uuid,
}
#[derive(Clone, Copy, Debug)]
pub struct SetAgentParams {
    pub machine_id: uuid::Uuid,
    pub id: uuid::Uuid,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BuildRow {
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
pub struct BuildRowBorrowed<'a> {
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
impl<'a> From<BuildRowBorrowed<'a>> for BuildRow {
    fn from(
        BuildRowBorrowed {
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
        }: BuildRowBorrowed<'a>,
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
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct GetStats {
    pub total_builds: Option<i64>,
    pub completed_builds: Option<i64>,
    pub failed_builds: Option<i64>,
    pub running_builds: Option<i64>,
    pub pending_builds: Option<i64>,
    pub avg_duration_seconds: Option<f64>,
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct BuildRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<BuildRowBorrowed, tokio_postgres::Error>,
    mapper: fn(BuildRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BuildRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(BuildRowBorrowed) -> R) -> BuildRowQuery<'c, 'a, 's, C, R, N> {
        BuildRowQuery {
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
pub struct UuidUuidQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<uuid::Uuid, tokio_postgres::Error>,
    mapper: fn(uuid::Uuid) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> UuidUuidQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(uuid::Uuid) -> R) -> UuidUuidQuery<'c, 'a, 's, C, R, N> {
        UuidUuidQuery {
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
pub struct StringQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<&str, tokio_postgres::Error>,
    mapper: fn(&str) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> StringQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(&str) -> R) -> StringQuery<'c, 'a, 's, C, R, N> {
        StringQuery {
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
pub struct GetStatsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<GetStats, tokio_postgres::Error>,
    mapper: fn(GetStats) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> GetStatsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(GetStats) -> R) -> GetStatsQuery<'c, 'a, 's, C, R, N> {
        GetStatsQuery {
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
pub struct CreateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create() -> CreateStmt {
    CreateStmt(
        "INSERT INTO builds ( evaluation_id, job_name, drv_path, status, system, outputs, is_aggregate, constituents, is_fod, fod_hash, meta_description, meta_license, meta_homepage, meta_maintainers, required_features ) VALUES ( $1, $2, $3, 'pending', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14 ) RETURNING *",
        None,
    )
}
impl CreateStmt {
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
        T4: crate::JsonSql,
        T5: crate::JsonSql,
        T6: crate::StringSql,
        T7: crate::StringSql,
        T8: crate::StringSql,
        T9: crate::StringSql,
        T10: crate::StringSql,
        T11: crate::StringSql,
        T12: crate::ArraySql<Item = T11>,
    >(
        &'s self,
        client: &'c C,
        evaluation_id: &'a uuid::Uuid,
        job_name: &'a T1,
        drv_path: &'a T2,
        system: &'a Option<T3>,
        outputs: &'a Option<T4>,
        is_aggregate: &'a bool,
        constituents: &'a Option<T5>,
        is_fod: &'a bool,
        fod_hash: &'a Option<T6>,
        meta_description: &'a Option<T7>,
        meta_license: &'a Option<T8>,
        meta_homepage: &'a Option<T9>,
        meta_maintainers: &'a Option<T10>,
        required_features: &'a T12,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 14> {
        BuildRowQuery {
            client,
            params: [
                evaluation_id,
                job_name,
                drv_path,
                system,
                outputs,
                is_aggregate,
                constituents,
                is_fod,
                fod_hash,
                meta_description,
                meta_license,
                meta_homepage,
                meta_maintainers,
                required_features,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
impl<
    'c,
    'a,
    's,
    C: GenericClient,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::JsonSql,
    T5: crate::JsonSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
    T8: crate::StringSql,
    T9: crate::StringSql,
    T10: crate::StringSql,
    T11: crate::StringSql,
    T12: crate::ArraySql<Item = T11>,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12>,
        BuildRowQuery<'c, 'a, 's, C, BuildRow, 14>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12>,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 14> {
        self.bind(
            client,
            &params.evaluation_id,
            &params.job_name,
            &params.drv_path,
            &params.system,
            &params.outputs,
            &params.is_aggregate,
            &params.constituents,
            &params.is_fod,
            &params.fod_hash,
            &params.meta_description,
            &params.meta_license,
            &params.meta_homepage,
            &params.meta_maintainers,
            &params.required_features,
        )
    }
}
pub struct GetCompletedByDrvPathStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_completed_by_drv_path() -> GetCompletedByDrvPathStmt {
    GetCompletedByDrvPathStmt(
        "SELECT * FROM builds WHERE drv_path = $1 AND status = 'succeeded' LIMIT 1",
        None,
    )
}
impl GetCompletedByDrvPathStmt {
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
        drv_path: &'a T1,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [drv_path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM builds WHERE id = $1", None)
}
impl GetStmt {
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
        id: &'a uuid::Uuid,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct ProjectIdForBuildStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn project_id_for_build() -> ProjectIdForBuildStmt {
    ProjectIdForBuildStmt(
        "SELECT j.project_id FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.id = $1",
        None,
    )
}
impl ProjectIdForBuildStmt {
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
        id: &'a uuid::Uuid,
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 1> {
        UuidUuidQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct ListForEvaluationStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_evaluation() -> ListForEvaluationStmt {
    ListForEvaluationStmt(
        "SELECT * FROM builds WHERE evaluation_id = $1 ORDER BY created_at DESC",
        None,
    )
}
impl ListForEvaluationStmt {
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
        evaluation_id: &'a uuid::Uuid,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [evaluation_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct ListForJobsetEvaluationsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_jobset_evaluations() -> ListForJobsetEvaluationsStmt {
    ListForJobsetEvaluationsStmt(
        "SELECT b.* FROM builds b JOIN evaluations e ON b.evaluation_id = e.id WHERE e.jobset_id = $1 AND b.evaluation_id = ANY($2) ORDER BY b.job_name ASC, e.evaluation_time DESC",
        None,
    )
}
impl ListForJobsetEvaluationsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = uuid::Uuid>>(
        &'s self,
        client: &'c C,
        jobset_id: &'a uuid::Uuid,
        evaluation_ids: &'a T1,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 2> {
        BuildRowQuery {
            client,
            params: [jobset_id, evaluation_ids],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = uuid::Uuid>>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListForJobsetEvaluationsParams<T1>,
        BuildRowQuery<'c, 'a, 's, C, BuildRow, 2>,
        C,
    > for ListForJobsetEvaluationsStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListForJobsetEvaluationsParams<T1>,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 2> {
        self.bind(client, &params.jobset_id, &params.evaluation_ids)
    }
}
pub struct ListPendingStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_pending() -> ListPendingStmt {
    ListPendingStmt(
        "WITH eligible_pending AS ( SELECT b.* FROM builds b WHERE b.status = 'pending' AND NOT EXISTS ( SELECT 1 FROM build_dependencies bd JOIN builds dep ON dep.id = bd.dependency_build_id WHERE bd.build_id = b.id AND dep.status != 'succeeded' ) AND NOT EXISTS ( SELECT 1 FROM builds active WHERE active.drv_path = b.drv_path AND active.status = 'running' ) ), running_counts AS ( SELECT e.jobset_id, COUNT(*) AS running FROM builds b JOIN evaluations e ON b.evaluation_id = e.id WHERE b.status = 'running' GROUP BY e.jobset_id ), active_shares AS ( SELECT j.id AS jobset_id, j.scheduling_shares, COALESCE(rc.running, 0) AS running, SUM(j.scheduling_shares) OVER () AS total_shares FROM jobsets j JOIN evaluations e2 ON e2.jobset_id = j.id JOIN eligible_pending b2 ON b2.evaluation_id = e2.id LEFT JOIN running_counts rc ON rc.jobset_id = j.id WHERE j.scheduling_shares > 0 GROUP BY j.id, j.scheduling_shares, rc.running ) SELECT b.* FROM eligible_pending b JOIN evaluations e ON b.evaluation_id = e.id JOIN active_shares ash ON ash.jobset_id = e.jobset_id ORDER BY b.priority DESC, cardinality(COALESCE(b.effective_features, b.required_features)) DESC, (ash.scheduling_shares::float / GREATEST(ash.total_shares, 1) - ash.running::float / GREATEST($1, 1)) DESC, b.created_at ASC, b.id ASC LIMIT $2",
        None,
    )
}
impl ListPendingStmt {
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
        schedulable_capacity: &'a i32,
        limit: &'a i64,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 2> {
        BuildRowQuery {
            client,
            params: [schedulable_capacity, limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListPendingParams,
        BuildRowQuery<'c, 'a, 's, C, BuildRow, 2>,
        C,
    > for ListPendingStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListPendingParams,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 2> {
        self.bind(client, &params.schedulable_capacity, &params.limit)
    }
}
pub struct StartStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn start() -> StartStmt {
    StartStmt(
        "WITH candidate AS ( SELECT b.id FROM builds b WHERE b.id = $1 AND b.status = 'pending' AND pg_try_advisory_xact_lock(hashtextextended(b.drv_path, 0)) AND NOT EXISTS ( SELECT 1 FROM builds active WHERE active.drv_path = b.drv_path AND active.status = 'running' ) FOR UPDATE SKIP LOCKED ) UPDATE builds SET status = 'running', started_at = NOW() FROM candidate WHERE builds.id = candidate.id RETURNING builds.*",
        None,
    )
}
impl StartStmt {
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
        id: &'a uuid::Uuid,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct MarkStartedNotifiedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn mark_started_notified() -> MarkStartedNotifiedStmt {
    MarkStartedNotifiedStmt(
        "UPDATE builds SET started_notified_at = NOW() WHERE id = $1 AND started_notified_at IS NULL RETURNING id",
        None,
    )
}
impl MarkStartedNotifiedStmt {
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
        id: &'a uuid::Uuid,
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 1> {
        UuidUuidQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct RequeueStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn requeue() -> RequeueStmt {
    RequeueStmt(
        "WITH bumped AS ( UPDATE builds SET status = 'pending', started_at = NULL, completed_at = NULL, effective_features = NULL WHERE id = $1 AND status = 'running' RETURNING * ) SELECT * FROM bumped",
        None,
    )
}
impl RequeueStmt {
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
        id: &'a uuid::Uuid,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct RetryStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn retry() -> RetryStmt {
    RetryStmt(
        "UPDATE builds SET status = 'pending', started_at = NULL, retry_count = retry_count + 1, completed_at = NULL, effective_features = NULL WHERE id = $1",
        None,
    )
}
impl RetryStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[id]).await
    }
}
pub struct CompleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn complete() -> CompleteStmt {
    CompleteStmt(
        "UPDATE builds SET status = $1, completed_at = NOW(), log_path = $2, build_output_path = $3, error_message = $4 WHERE id = $5 RETURNING *",
        None,
    )
}
impl CompleteStmt {
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
        T4: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        status: &'a T1,
        log_path: &'a Option<T2>,
        build_output_path: &'a Option<T3>,
        error_message: &'a Option<T4>,
        id: &'a uuid::Uuid,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 5> {
        BuildRowQuery {
            client,
            params: [status, log_path, build_output_path, error_message, id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
impl<
    'c,
    'a,
    's,
    C: GenericClient,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CompleteParams<T1, T2, T3, T4>,
        BuildRowQuery<'c, 'a, 's, C, BuildRow, 5>,
        C,
    > for CompleteStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CompleteParams<T1, T2, T3, T4>,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 5> {
        self.bind(
            client,
            &params.status,
            &params.log_path,
            &params.build_output_path,
            &params.error_message,
            &params.id,
        )
    }
}
pub struct ListPendingInSchedulerOrderStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_pending_in_scheduler_order() -> ListPendingInSchedulerOrderStmt {
    ListPendingInSchedulerOrderStmt(
        "SELECT * FROM builds WHERE status = 'pending' AND ($1::text IS NULL OR system = $1) AND ($2::text IS NULL OR job_name ILIKE '%' || $2 || '%') ORDER BY priority DESC, cardinality(COALESCE(effective_features, required_features)) DESC, created_at ASC, id ASC LIMIT $3 OFFSET $4",
        None,
    )
}
impl ListPendingInSchedulerOrderStmt {
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
        system: &'a Option<T1>,
        job_name: &'a Option<T2>,
        limit: &'a i64,
        offset: &'a i64,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 4> {
        BuildRowQuery {
            client,
            params: [system, job_name, limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListPendingInSchedulerOrderParams<T1, T2>,
        BuildRowQuery<'c, 'a, 's, C, BuildRow, 4>,
        C,
    > for ListPendingInSchedulerOrderStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListPendingInSchedulerOrderParams<T1, T2>,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 4> {
        self.bind(
            client,
            &params.system,
            &params.job_name,
            &params.limit,
            &params.offset,
        )
    }
}
pub struct ListPendingForSystemsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_pending_for_systems() -> ListPendingForSystemsStmt {
    ListPendingForSystemsStmt(
        "SELECT * FROM builds WHERE status = 'pending' AND system = ANY($1) ORDER BY priority DESC, created_at ASC LIMIT 512",
        None,
    )
}
impl ListPendingForSystemsStmt {
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
        T2: crate::ArraySql<Item = T1>,
    >(
        &'s self,
        client: &'c C,
        systems: &'a T2,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [systems],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct PendingFeatureDemandStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn pending_feature_demand() -> PendingFeatureDemandStmt {
    PendingFeatureDemandStmt(
        "SELECT DISTINCT unnest(COALESCE(effective_features, required_features)) FROM builds WHERE status = 'pending' AND system = $1",
        None,
    )
}
impl PendingFeatureDemandStmt {
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
        system: &'a T1,
    ) -> StringQuery<'c, 'a, 's, C, String, 1> {
        StringQuery {
            client,
            params: [system],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
pub struct BumpPriorityStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn bump_priority() -> BumpPriorityStmt {
    BumpPriorityStmt(
        "UPDATE builds SET priority = priority + $1 WHERE id = $2 AND status = 'pending' RETURNING *",
        None,
    )
}
impl BumpPriorityStmt {
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
        delta: &'a i32,
        id: &'a uuid::Uuid,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 2> {
        BuildRowQuery {
            client,
            params: [delta, id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        BumpPriorityParams,
        BuildRowQuery<'c, 'a, 's, C, BuildRow, 2>,
        C,
    > for BumpPriorityStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a BumpPriorityParams,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 2> {
        self.bind(client, &params.delta, &params.id)
    }
}
pub struct ListRecentStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_recent() -> ListRecentStmt {
    ListRecentStmt(
        "SELECT * FROM builds ORDER BY created_at DESC LIMIT $1",
        None,
    )
}
impl ListRecentStmt {
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
        limit: &'a i64,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct ListForProjectStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_project() -> ListForProjectStmt {
    ListForProjectStmt(
        "SELECT b.* FROM builds b JOIN evaluations e ON b.evaluation_id = e.id JOIN jobsets j ON e.jobset_id = j.id WHERE j.project_id = $1 ORDER BY b.created_at DESC",
        None,
    )
}
impl ListForProjectStmt {
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
        project_id: &'a uuid::Uuid,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct GetStatsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_stats() -> GetStatsStmt {
    GetStatsStmt("SELECT * FROM build_stats", None)
}
impl GetStatsStmt {
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
    ) -> GetStatsQuery<'c, 'a, 's, C, GetStats, 0> {
        GetStatsQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &tokio_postgres::Row| -> Result<GetStats, tokio_postgres::Error> {
                Ok(GetStats {
                    total_builds: row.try_get(0)?,
                    completed_builds: row.try_get(1)?,
                    failed_builds: row.try_get(2)?,
                    running_builds: row.try_get(3)?,
                    pending_builds: row.try_get(4)?,
                    avg_duration_seconds: row.try_get(5)?,
                })
            },
            mapper: |it| GetStats::from(it),
        }
    }
}
pub struct ResetOrphanedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn reset_orphaned() -> ResetOrphanedStmt {
    ResetOrphanedStmt(
        "UPDATE builds SET status = 'pending', started_at = NULL, effective_features = NULL WHERE status = 'running' AND started_at < NOW() - make_interval(secs => $1::bigint) AND NOT (id = ANY($2))",
        None,
    )
}
impl ResetOrphanedStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = uuid::Uuid>>(
        &'s self,
        client: &'c C,
        older_than_secs: &'a i64,
        excluded_ids: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[older_than_secs, excluded_ids])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::ArraySql<Item = uuid::Uuid>>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        ResetOrphanedParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for ResetOrphanedStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a ResetOrphanedParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.older_than_secs, &params.excluded_ids))
    }
}
pub struct ListFilteredStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_filtered() -> ListFilteredStmt {
    ListFilteredStmt(
        "SELECT * FROM builds WHERE ($1::uuid IS NULL OR evaluation_id = $1) AND ($2::text IS NULL OR status = $2) AND ($3::text IS NULL OR system = $3) AND ($4::text IS NULL OR job_name ILIKE '%' || $4 || '%') ORDER BY created_at DESC LIMIT $5 OFFSET $6",
        None,
    )
}
impl ListFilteredStmt {
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
        evaluation_id: &'a Option<uuid::Uuid>,
        status: &'a Option<T1>,
        system: &'a Option<T2>,
        job_name: &'a Option<T3>,
        limit: &'a i64,
        offset: &'a i64,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 6> {
        BuildRowQuery {
            client,
            params: [evaluation_id, status, system, job_name, limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListFilteredParams<T1, T2, T3>,
        BuildRowQuery<'c, 'a, 's, C, BuildRow, 6>,
        C,
    > for ListFilteredStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListFilteredParams<T1, T2, T3>,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 6> {
        self.bind(
            client,
            &params.evaluation_id,
            &params.status,
            &params.system,
            &params.job_name,
            &params.limit,
            &params.offset,
        )
    }
}
pub struct CountFilteredStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_filtered() -> CountFilteredStmt {
    CountFilteredStmt(
        "SELECT COUNT(*) FROM builds WHERE ($1::uuid IS NULL OR evaluation_id = $1) AND ($2::text IS NULL OR status = $2) AND ($3::text IS NULL OR system = $3) AND ($4::text IS NULL OR job_name ILIKE '%' || $4 || '%')",
        None,
    )
}
impl CountFilteredStmt {
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
        evaluation_id: &'a Option<uuid::Uuid>,
        status: &'a Option<T1>,
        system: &'a Option<T2>,
        job_name: &'a Option<T3>,
    ) -> I64Query<'c, 'a, 's, C, i64, 4> {
        I64Query {
            client,
            params: [evaluation_id, status, system, job_name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CountFilteredParams<T1, T2, T3>,
        I64Query<'c, 'a, 's, C, i64, 4>,
        C,
    > for CountFilteredStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CountFilteredParams<T1, T2, T3>,
    ) -> I64Query<'c, 'a, 's, C, i64, 4> {
        self.bind(
            client,
            &params.evaluation_id,
            &params.status,
            &params.system,
            &params.job_name,
        )
    }
}
pub struct GetCancelledAmongStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_cancelled_among() -> GetCancelledAmongStmt {
    GetCancelledAmongStmt(
        "SELECT id FROM builds WHERE id = ANY($1) AND status = 'cancelled'",
        None,
    )
}
impl GetCancelledAmongStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = uuid::Uuid>>(
        &'s self,
        client: &'c C,
        build_ids: &'a T1,
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 1> {
        UuidUuidQuery {
            client,
            params: [build_ids],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct CancelStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn cancel() -> CancelStmt {
    CancelStmt(
        "UPDATE builds SET status = 'cancelled', completed_at = NOW() WHERE id = $1 AND status IN ('pending', 'running') RETURNING *",
        None,
    )
}
impl CancelStmt {
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
        id: &'a uuid::Uuid,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct CancelCascadeDependentsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn cancel_cascade_dependents() -> CancelCascadeDependentsStmt {
    CancelCascadeDependentsStmt(
        "SELECT build_id FROM build_dependencies WHERE dependency_build_id = $1",
        None,
    )
}
impl CancelCascadeDependentsStmt {
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
        dependency_build_id: &'a uuid::Uuid,
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 1> {
        UuidUuidQuery {
            client,
            params: [dependency_build_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct RestartStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn restart() -> RestartStmt {
    RestartStmt(
        "UPDATE builds SET status = 'pending', started_at = NULL, completed_at = NULL, log_path = NULL, build_output_path = NULL, error_message = NULL, started_notified_at = NULL, effective_features = NULL, retry_count = retry_count + 1 WHERE id = $1 AND status IN ('failed', 'succeeded', 'cancelled', 'cached_failure') RETURNING *",
        None,
    )
}
impl RestartStmt {
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
        id: &'a uuid::Uuid,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct SetEffectiveFeaturesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_effective_features() -> SetEffectiveFeaturesStmt {
    SetEffectiveFeaturesStmt(
        "UPDATE builds SET effective_features = $1 WHERE id = $2",
        None,
    )
}
impl SetEffectiveFeaturesStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::ArraySql<Item = T1>,
    >(
        &'s self,
        client: &'c C,
        features: &'a T2,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[features, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql, T2: crate::ArraySql<Item = T1>>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetEffectiveFeaturesParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetEffectiveFeaturesStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SetEffectiveFeaturesParams<T1, T2>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.features, &params.id))
    }
}
pub struct MarkSignedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn mark_signed() -> MarkSignedStmt {
    MarkSignedStmt("UPDATE builds SET signed = true WHERE id = $1", None)
}
impl MarkSignedStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[id]).await
    }
}
pub struct GetCompletedByDrvPathsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_completed_by_drv_paths() -> GetCompletedByDrvPathsStmt {
    GetCompletedByDrvPathsStmt(
        "SELECT DISTINCT ON (drv_path) * FROM builds WHERE drv_path = ANY($1) AND status = 'succeeded' ORDER BY drv_path, completed_at DESC",
        None,
    )
}
impl GetCompletedByDrvPathsStmt {
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
        T2: crate::ArraySql<Item = T1>,
    >(
        &'s self,
        client: &'c C,
        drv_paths: &'a T2,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [drv_paths],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct ListPinnedIdsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_pinned_ids() -> ListPinnedIdsStmt {
    ListPinnedIdsStmt("SELECT id FROM builds WHERE keep = true", None)
}
impl ListPinnedIdsStmt {
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
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 0> {
        UuidUuidQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct SetKeepStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_keep() -> SetKeepStmt {
    SetKeepStmt(
        "UPDATE builds SET keep = $1 WHERE id = $2 RETURNING *",
        None,
    )
}
impl SetKeepStmt {
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
        keep: &'a bool,
        id: &'a uuid::Uuid,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 2> {
        BuildRowQuery {
            client,
            params: [keep, id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        SetKeepParams,
        BuildRowQuery<'c, 'a, 's, C, BuildRow, 2>,
        C,
    > for SetKeepStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a SetKeepParams,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 2> {
        self.bind(client, &params.keep, &params.id)
    }
}
pub struct SetBuilderStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_builder() -> SetBuilderStmt {
    SetBuilderStmt("UPDATE builds SET builder_id = $1 WHERE id = $2", None)
}
impl SetBuilderStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        builder_id: &'a uuid::Uuid,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[builder_id, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetBuilderParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetBuilderStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SetBuilderParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.builder_id, &params.id))
    }
}
pub struct SetAgentStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_agent() -> SetAgentStmt {
    SetAgentStmt(
        "UPDATE builds SET agent_machine_id = $1 WHERE id = $2",
        None,
    )
}
impl SetAgentStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        machine_id: &'a uuid::Uuid,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[machine_id, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetAgentParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetAgentStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SetAgentParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.machine_id, &params.id))
    }
}
pub struct ListConstituentsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_constituents() -> ListConstituentsStmt {
    ListConstituentsStmt(
        "SELECT b.* FROM builds b JOIN build_dependencies bd ON b.id = bd.dependency_build_id WHERE bd.build_id = $1 ORDER BY b.created_at",
        None,
    )
}
impl ListConstituentsStmt {
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
        build_id: &'a uuid::Uuid,
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [build_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildRowBorrowed {
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
            mapper: |it| BuildRow::from(it),
        }
    }
}
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("DELETE FROM builds WHERE id = $1", None)
}
impl DeleteStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[id]).await
    }
}
