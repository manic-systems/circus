// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
> {
    pub project_id: uuid::Uuid,
    pub name: T1,
    pub nix_expression: T2,
    pub enabled: bool,
    pub flake_mode: bool,
    pub check_interval: i32,
    pub trigger_mode: T3,
    pub branch: Option<T4>,
    pub branch_pattern: Option<T5>,
    pub tag_pattern: Option<T6>,
    pub scheduling_shares: i32,
    pub state: T7,
    pub keep_nr: i32,
}
#[derive(Clone, Copy, Debug)]
pub struct ListForProjectParams {
    pub project_id: uuid::Uuid,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct UpdateParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
> {
    pub name: T1,
    pub nix_expression: T2,
    pub enabled: bool,
    pub flake_mode: bool,
    pub check_interval: i32,
    pub trigger_mode: T3,
    pub branch: Option<T4>,
    pub branch_pattern: Option<T5>,
    pub tag_pattern: Option<T6>,
    pub scheduling_shares: i32,
    pub state: T7,
    pub keep_nr: i32,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpsertParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
> {
    pub project_id: uuid::Uuid,
    pub name: T1,
    pub nix_expression: T2,
    pub enabled: bool,
    pub flake_mode: bool,
    pub check_interval: i32,
    pub trigger_mode: T3,
    pub branch: Option<T4>,
    pub branch_pattern: Option<T5>,
    pub tag_pattern: Option<T6>,
    pub scheduling_shares: i32,
    pub state: T7,
    pub keep_nr: i32,
}
#[derive(Debug, Clone, PartialEq)]
pub struct JobsetRow {
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
}
pub struct JobsetRowBorrowed<'a> {
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
}
impl<'a> From<JobsetRowBorrowed<'a>> for JobsetRow {
    fn from(
        JobsetRowBorrowed {
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
        }: JobsetRowBorrowed<'a>,
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
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveJobsetRow {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub name: String,
    pub nix_expression: String,
    pub enabled: bool,
    pub flake_mode: bool,
    pub check_interval: i32,
    pub branch: Option<String>,
    pub branch_pattern: Option<String>,
    pub tag_pattern: Option<String>,
    pub scheduling_shares: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub state: String,
    pub last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub keep_nr: i32,
    pub project_name: String,
    pub repository_url: String,
    pub trigger_mode: String,
}
pub struct ActiveJobsetRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub name: &'a str,
    pub nix_expression: &'a str,
    pub enabled: bool,
    pub flake_mode: bool,
    pub check_interval: i32,
    pub branch: Option<&'a str>,
    pub branch_pattern: Option<&'a str>,
    pub tag_pattern: Option<&'a str>,
    pub scheduling_shares: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub state: &'a str,
    pub last_checked_at: Option<chrono::DateTime<chrono::Utc>>,
    pub keep_nr: i32,
    pub project_name: &'a str,
    pub repository_url: &'a str,
    pub trigger_mode: &'a str,
}
impl<'a> From<ActiveJobsetRowBorrowed<'a>> for ActiveJobsetRow {
    fn from(
        ActiveJobsetRowBorrowed {
            id,
            project_id,
            name,
            nix_expression,
            enabled,
            flake_mode,
            check_interval,
            branch,
            branch_pattern,
            tag_pattern,
            scheduling_shares,
            created_at,
            updated_at,
            state,
            last_checked_at,
            keep_nr,
            project_name,
            repository_url,
            trigger_mode,
        }: ActiveJobsetRowBorrowed<'a>,
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
            branch_pattern: branch_pattern.map(|v| v.into()),
            tag_pattern: tag_pattern.map(|v| v.into()),
            scheduling_shares,
            created_at,
            updated_at,
            state: state.into(),
            last_checked_at,
            keep_nr,
            project_name: project_name.into(),
            repository_url: repository_url.into(),
            trigger_mode: trigger_mode.into(),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct JobsetRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<JobsetRowBorrowed, tokio_postgres::Error>,
    mapper: fn(JobsetRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> JobsetRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(JobsetRowBorrowed) -> R) -> JobsetRowQuery<'c, 'a, 's, C, R, N> {
        JobsetRowQuery {
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
pub struct ActiveJobsetRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ActiveJobsetRowBorrowed, tokio_postgres::Error>,
    mapper: fn(ActiveJobsetRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ActiveJobsetRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(ActiveJobsetRowBorrowed) -> R,
    ) -> ActiveJobsetRowQuery<'c, 'a, 's, C, R, N> {
        ActiveJobsetRowQuery {
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
        "INSERT INTO jobsets (project_id, name, nix_expression, enabled, flake_mode, check_interval, trigger_mode, branch, branch_pattern, tag_pattern, scheduling_shares, state, keep_nr) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING *",
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
        T4: crate::StringSql,
        T5: crate::StringSql,
        T6: crate::StringSql,
        T7: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        project_id: &'a uuid::Uuid,
        name: &'a T1,
        nix_expression: &'a T2,
        enabled: &'a bool,
        flake_mode: &'a bool,
        check_interval: &'a i32,
        trigger_mode: &'a T3,
        branch: &'a Option<T4>,
        branch_pattern: &'a Option<T5>,
        tag_pattern: &'a Option<T6>,
        scheduling_shares: &'a i32,
        state: &'a T7,
        keep_nr: &'a i32,
    ) -> JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 13> {
        JobsetRowQuery {
            client,
            params: [
                project_id,
                name,
                nix_expression,
                enabled,
                flake_mode,
                check_interval,
                trigger_mode,
                branch,
                branch_pattern,
                tag_pattern,
                scheduling_shares,
                state,
                keep_nr,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<JobsetRowBorrowed, tokio_postgres::Error> {
                    Ok(JobsetRowBorrowed {
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
                    })
                },
            mapper: |it| JobsetRow::from(it),
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
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1, T2, T3, T4, T5, T6, T7>,
        JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 13>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1, T2, T3, T4, T5, T6, T7>,
    ) -> JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 13> {
        self.bind(
            client,
            &params.project_id,
            &params.name,
            &params.nix_expression,
            &params.enabled,
            &params.flake_mode,
            &params.check_interval,
            &params.trigger_mode,
            &params.branch,
            &params.branch_pattern,
            &params.tag_pattern,
            &params.scheduling_shares,
            &params.state,
            &params.keep_nr,
        )
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM jobsets WHERE id = $1", None)
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
    ) -> JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 1> {
        JobsetRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<JobsetRowBorrowed, tokio_postgres::Error> {
                    Ok(JobsetRowBorrowed {
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
                    })
                },
            mapper: |it| JobsetRow::from(it),
        }
    }
}
pub struct ListForProjectStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_project() -> ListForProjectStmt {
    ListForProjectStmt(
        "SELECT * FROM jobsets WHERE project_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
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
        limit: &'a i64,
        offset: &'a i64,
    ) -> JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 3> {
        JobsetRowQuery {
            client,
            params: [project_id, limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<JobsetRowBorrowed, tokio_postgres::Error> {
                    Ok(JobsetRowBorrowed {
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
                    })
                },
            mapper: |it| JobsetRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListForProjectParams,
        JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 3>,
        C,
    > for ListForProjectStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListForProjectParams,
    ) -> JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 3> {
        self.bind(client, &params.project_id, &params.limit, &params.offset)
    }
}
pub struct ListAllForProjectStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_all_for_project() -> ListAllForProjectStmt {
    ListAllForProjectStmt(
        "SELECT * FROM jobsets WHERE project_id = $1 ORDER BY created_at DESC",
        None,
    )
}
impl ListAllForProjectStmt {
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
    ) -> JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 1> {
        JobsetRowQuery {
            client,
            params: [project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<JobsetRowBorrowed, tokio_postgres::Error> {
                    Ok(JobsetRowBorrowed {
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
                    })
                },
            mapper: |it| JobsetRow::from(it),
        }
    }
}
pub struct CountForProjectStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_for_project() -> CountForProjectStmt {
    CountForProjectStmt("SELECT COUNT(*) FROM jobsets WHERE project_id = $1", None)
}
impl CountForProjectStmt {
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
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct CountStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count() -> CountStmt {
    CountStmt("SELECT COUNT(*) FROM jobsets", None)
}
impl CountStmt {
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
    ) -> I64Query<'c, 'a, 's, C, i64, 0> {
        I64Query {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct UpdateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update() -> UpdateStmt {
    UpdateStmt(
        "UPDATE jobsets SET name = $1, nix_expression = $2, enabled = $3, flake_mode = $4, check_interval = $5, trigger_mode = $6, branch = $7, branch_pattern = $8, tag_pattern = $9, scheduling_shares = $10, state = $11, keep_nr = $12 WHERE id = $13 RETURNING *",
        None,
    )
}
impl UpdateStmt {
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
        T5: crate::StringSql,
        T6: crate::StringSql,
        T7: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        name: &'a T1,
        nix_expression: &'a T2,
        enabled: &'a bool,
        flake_mode: &'a bool,
        check_interval: &'a i32,
        trigger_mode: &'a T3,
        branch: &'a Option<T4>,
        branch_pattern: &'a Option<T5>,
        tag_pattern: &'a Option<T6>,
        scheduling_shares: &'a i32,
        state: &'a T7,
        keep_nr: &'a i32,
        id: &'a uuid::Uuid,
    ) -> JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 13> {
        JobsetRowQuery {
            client,
            params: [
                name,
                nix_expression,
                enabled,
                flake_mode,
                check_interval,
                trigger_mode,
                branch,
                branch_pattern,
                tag_pattern,
                scheduling_shares,
                state,
                keep_nr,
                id,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<JobsetRowBorrowed, tokio_postgres::Error> {
                    Ok(JobsetRowBorrowed {
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
                    })
                },
            mapper: |it| JobsetRow::from(it),
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
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpdateParams<T1, T2, T3, T4, T5, T6, T7>,
        JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 13>,
        C,
    > for UpdateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpdateParams<T1, T2, T3, T4, T5, T6, T7>,
    ) -> JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 13> {
        self.bind(
            client,
            &params.name,
            &params.nix_expression,
            &params.enabled,
            &params.flake_mode,
            &params.check_interval,
            &params.trigger_mode,
            &params.branch,
            &params.branch_pattern,
            &params.tag_pattern,
            &params.scheduling_shares,
            &params.state,
            &params.keep_nr,
            &params.id,
        )
    }
}
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("DELETE FROM jobsets WHERE id = $1", None)
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
pub struct UpsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert() -> UpsertStmt {
    UpsertStmt(
        "INSERT INTO jobsets (project_id, name, nix_expression, enabled, flake_mode, check_interval, trigger_mode, branch, branch_pattern, tag_pattern, scheduling_shares, state, keep_nr) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) ON CONFLICT (project_id, name) DO UPDATE SET nix_expression = EXCLUDED.nix_expression, enabled = EXCLUDED.enabled, flake_mode = EXCLUDED.flake_mode, check_interval = EXCLUDED.check_interval, trigger_mode = EXCLUDED.trigger_mode, branch = EXCLUDED.branch, branch_pattern = EXCLUDED.branch_pattern, tag_pattern = EXCLUDED.tag_pattern, scheduling_shares = EXCLUDED.scheduling_shares, state = EXCLUDED.state, keep_nr = EXCLUDED.keep_nr RETURNING *",
        None,
    )
}
impl UpsertStmt {
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
        T5: crate::StringSql,
        T6: crate::StringSql,
        T7: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        project_id: &'a uuid::Uuid,
        name: &'a T1,
        nix_expression: &'a T2,
        enabled: &'a bool,
        flake_mode: &'a bool,
        check_interval: &'a i32,
        trigger_mode: &'a T3,
        branch: &'a Option<T4>,
        branch_pattern: &'a Option<T5>,
        tag_pattern: &'a Option<T6>,
        scheduling_shares: &'a i32,
        state: &'a T7,
        keep_nr: &'a i32,
    ) -> JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 13> {
        JobsetRowQuery {
            client,
            params: [
                project_id,
                name,
                nix_expression,
                enabled,
                flake_mode,
                check_interval,
                trigger_mode,
                branch,
                branch_pattern,
                tag_pattern,
                scheduling_shares,
                state,
                keep_nr,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<JobsetRowBorrowed, tokio_postgres::Error> {
                    Ok(JobsetRowBorrowed {
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
                    })
                },
            mapper: |it| JobsetRow::from(it),
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
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpsertParams<T1, T2, T3, T4, T5, T6, T7>,
        JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 13>,
        C,
    > for UpsertStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpsertParams<T1, T2, T3, T4, T5, T6, T7>,
    ) -> JobsetRowQuery<'c, 'a, 's, C, JobsetRow, 13> {
        self.bind(
            client,
            &params.project_id,
            &params.name,
            &params.nix_expression,
            &params.enabled,
            &params.flake_mode,
            &params.check_interval,
            &params.trigger_mode,
            &params.branch,
            &params.branch_pattern,
            &params.tag_pattern,
            &params.scheduling_shares,
            &params.state,
            &params.keep_nr,
        )
    }
}
pub struct ListActiveStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_active() -> ListActiveStmt {
    ListActiveStmt("SELECT * FROM active_jobsets", None)
}
impl ListActiveStmt {
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
    ) -> ActiveJobsetRowQuery<'c, 'a, 's, C, ActiveJobsetRow, 0> {
        ActiveJobsetRowQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ActiveJobsetRowBorrowed, tokio_postgres::Error> {
                Ok(ActiveJobsetRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    name: row.try_get(2)?,
                    nix_expression: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    flake_mode: row.try_get(5)?,
                    check_interval: row.try_get(6)?,
                    branch: row.try_get(7)?,
                    branch_pattern: row.try_get(8)?,
                    tag_pattern: row.try_get(9)?,
                    scheduling_shares: row.try_get(10)?,
                    created_at: row.try_get(11)?,
                    updated_at: row.try_get(12)?,
                    state: row.try_get(13)?,
                    last_checked_at: row.try_get(14)?,
                    keep_nr: row.try_get(15)?,
                    project_name: row.try_get(16)?,
                    repository_url: row.try_get(17)?,
                    trigger_mode: row.try_get(18)?,
                })
            },
            mapper: |it| ActiveJobsetRow::from(it),
        }
    }
}
pub struct MarkOneShotCompleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn mark_one_shot_complete() -> MarkOneShotCompleteStmt {
    MarkOneShotCompleteStmt(
        "UPDATE jobsets SET enabled = false WHERE id = $1 AND state = 'one_shot'",
        None,
    )
}
impl MarkOneShotCompleteStmt {
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
pub struct UpdateLastCheckedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_last_checked() -> UpdateLastCheckedStmt {
    UpdateLastCheckedStmt(
        "UPDATE jobsets SET last_checked_at = NOW() WHERE id = $1",
        None,
    )
}
impl UpdateLastCheckedStmt {
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
pub struct HasRunningBuildsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn has_running_builds() -> HasRunningBuildsStmt {
    HasRunningBuildsStmt(
        "SELECT COUNT(*) FROM builds b JOIN evaluations e ON b.evaluation_id = e.id WHERE e.jobset_id = $1 AND b.status = 'running'",
        None,
    )
}
impl HasRunningBuildsStmt {
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
        jobset_id: &'a uuid::Uuid,
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [jobset_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct HasUnfinishedWorkStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn has_unfinished_work() -> HasUnfinishedWorkStmt {
    HasUnfinishedWorkStmt(
        "SELECT COUNT(*) FROM evaluations e LEFT JOIN builds b ON b.evaluation_id = e.id WHERE e.jobset_id = $1 AND (e.status IN ('pending', 'running') OR b.status IN ('pending', 'running'))",
        None,
    )
}
impl HasUnfinishedWorkStmt {
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
        jobset_id: &'a uuid::Uuid,
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [jobset_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct ListDueForEvalStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_due_for_eval() -> ListDueForEvalStmt {
    ListDueForEvalStmt(
        "SELECT * FROM active_jobsets WHERE last_checked_at IS NULL OR last_checked_at < NOW() - (check_interval || ' seconds')::interval ORDER BY last_checked_at NULLS FIRST LIMIT $1",
        None,
    )
}
impl ListDueForEvalStmt {
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
    ) -> ActiveJobsetRowQuery<'c, 'a, 's, C, ActiveJobsetRow, 1> {
        ActiveJobsetRowQuery {
            client,
            params: [limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ActiveJobsetRowBorrowed, tokio_postgres::Error> {
                Ok(ActiveJobsetRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    name: row.try_get(2)?,
                    nix_expression: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    flake_mode: row.try_get(5)?,
                    check_interval: row.try_get(6)?,
                    branch: row.try_get(7)?,
                    branch_pattern: row.try_get(8)?,
                    tag_pattern: row.try_get(9)?,
                    scheduling_shares: row.try_get(10)?,
                    created_at: row.try_get(11)?,
                    updated_at: row.try_get(12)?,
                    state: row.try_get(13)?,
                    last_checked_at: row.try_get(14)?,
                    keep_nr: row.try_get(15)?,
                    project_name: row.try_get(16)?,
                    repository_url: row.try_get(17)?,
                    trigger_mode: row.try_get(18)?,
                })
            },
            mapper: |it| ActiveJobsetRow::from(it),
        }
    }
}
