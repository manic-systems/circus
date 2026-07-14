// This file was generated with `cornucopia`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct CreateParams {
    pub build_id: uuid::Uuid,
    pub dependency_build_id: uuid::Uuid,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct BuildDependencyRow {
    pub id: uuid::Uuid,
    pub build_id: uuid::Uuid,
    pub dependency_build_id: uuid::Uuid,
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
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct BuildDependencyRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<BuildDependencyRow, tokio_postgres::Error>,
    mapper: fn(BuildDependencyRow) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BuildDependencyRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(BuildDependencyRow) -> R,
    ) -> BuildDependencyRowQuery<'c, 'a, 's, C, R, N> {
        BuildDependencyRowQuery {
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
        "INSERT INTO build_dependencies (build_id, dependency_build_id) VALUES ($1,$2) RETURNING *",
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
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        build_id: &'a uuid::Uuid,
        dependency_build_id: &'a uuid::Uuid,
    ) -> BuildDependencyRowQuery<'c, 'a, 's, C, BuildDependencyRow, 2> {
        BuildDependencyRowQuery {
            client,
            params: [build_id, dependency_build_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildDependencyRow, tokio_postgres::Error> {
                    Ok(BuildDependencyRow {
                        id: row.try_get(0)?,
                        build_id: row.try_get(1)?,
                        dependency_build_id: row.try_get(2)?,
                    })
                },
            mapper: |it| BuildDependencyRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams,
        BuildDependencyRowQuery<'c, 'a, 's, C, BuildDependencyRow, 2>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams,
    ) -> BuildDependencyRowQuery<'c, 'a, 's, C, BuildDependencyRow, 2> {
        self.bind(client, &params.build_id, &params.dependency_build_id)
    }
}
pub struct ListForBuildStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_build() -> ListForBuildStmt {
    ListForBuildStmt("SELECT * FROM build_dependencies WHERE build_id =$1", None)
}
impl ListForBuildStmt {
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
    ) -> BuildDependencyRowQuery<'c, 'a, 's, C, BuildDependencyRow, 1> {
        BuildDependencyRowQuery {
            client,
            params: [build_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildDependencyRow, tokio_postgres::Error> {
                    Ok(BuildDependencyRow {
                        id: row.try_get(0)?,
                        build_id: row.try_get(1)?,
                        dependency_build_id: row.try_get(2)?,
                    })
                },
            mapper: |it| BuildDependencyRow::from(it),
        }
    }
}
pub struct ListDependencyBuildsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_dependency_builds() -> ListDependencyBuildsStmt {
    ListDependencyBuildsStmt(
        "SELECT b.* FROM build_dependencies bd JOIN builds b ON b.id = bd.dependency_build_id WHERE bd.build_id =$1 ORDER BY b.job_name",
        None,
    )
}
impl ListDependencyBuildsStmt {
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
pub struct ListDependentBuildsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_dependent_builds() -> ListDependentBuildsStmt {
    ListDependentBuildsStmt(
        "SELECT b.* FROM build_dependencies bd JOIN builds b ON b.id = bd.build_id WHERE bd.dependency_build_id =$1 ORDER BY b.job_name",
        None,
    )
}
impl ListDependentBuildsStmt {
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
    ) -> BuildRowQuery<'c, 'a, 's, C, BuildRow, 1> {
        BuildRowQuery {
            client,
            params: [dependency_build_id],
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
pub struct CheckDepsForBuildsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn check_deps_for_builds() -> CheckDepsForBuildsStmt {
    CheckDepsForBuildsStmt(
        "SELECT DISTINCT bd.build_id FROM build_dependencies bd JOIN builds b ON bd.dependency_build_id = b.id WHERE bd.build_id = ANY ($1) AND b.status != 'succeeded'",
        None,
    )
}
impl CheckDepsForBuildsStmt {
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
pub struct AllDepsCompletedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn all_deps_completed() -> AllDepsCompletedStmt {
    AllDepsCompletedStmt(
        "SELECT COUNT(*) FROM build_dependencies bd JOIN builds b ON bd.dependency_build_id = b.id WHERE bd.build_id =$1 AND b.status != 'succeeded'",
        None,
    )
}
impl AllDepsCompletedStmt {
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
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [build_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
