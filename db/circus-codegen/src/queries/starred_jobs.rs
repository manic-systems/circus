// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql> {
    pub user_id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub jobset_id: Option<uuid::Uuid>,
    pub job_name: T1,
}
#[derive(Clone, Copy, Debug)]
pub struct ListForUserParams {
    pub user_id: uuid::Uuid,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct IsStarredParams<T1: crate::StringSql> {
    pub user_id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub jobset_id: Option<uuid::Uuid>,
    pub job_name: T1,
}
#[derive(Clone, Copy, Debug)]
pub struct DeleteForUserParams {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
}
#[derive(Debug)]
pub struct DeleteByJobParams<T1: crate::StringSql> {
    pub user_id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub jobset_id: Option<uuid::Uuid>,
    pub job_name: T1,
}
#[derive(Debug, Clone, PartialEq)]
pub struct StarredJobRow {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub jobset_id: Option<uuid::Uuid>,
    pub job_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
pub struct StarredJobRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub jobset_id: Option<uuid::Uuid>,
    pub job_name: &'a str,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
impl<'a> From<StarredJobRowBorrowed<'a>> for StarredJobRow {
    fn from(
        StarredJobRowBorrowed {
            id,
            user_id,
            project_id,
            jobset_id,
            job_name,
            created_at,
        }: StarredJobRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            user_id,
            project_id,
            jobset_id,
            job_name: job_name.into(),
            created_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct StarredJobRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<StarredJobRowBorrowed, tokio_postgres::Error>,
    mapper: fn(StarredJobRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> StarredJobRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(StarredJobRowBorrowed) -> R,
    ) -> StarredJobRowQuery<'c, 'a, 's, C, R, N> {
        StarredJobRowQuery {
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
        "INSERT INTO starred_jobs (user_id, project_id, jobset_id, job_name) VALUES ($1, $2, $3, $4) RETURNING *",
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
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
        project_id: &'a uuid::Uuid,
        jobset_id: &'a Option<uuid::Uuid>,
        job_name: &'a T1,
    ) -> StarredJobRowQuery<'c, 'a, 's, C, StarredJobRow, 4> {
        StarredJobRowQuery {
            client,
            params: [user_id, project_id, jobset_id, job_name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<StarredJobRowBorrowed, tokio_postgres::Error> {
                    Ok(StarredJobRowBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        project_id: row.try_get(2)?,
                        jobset_id: row.try_get(3)?,
                        job_name: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                    })
                },
            mapper: |it| StarredJobRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1>,
        StarredJobRowQuery<'c, 'a, 's, C, StarredJobRow, 4>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1>,
    ) -> StarredJobRowQuery<'c, 'a, 's, C, StarredJobRow, 4> {
        self.bind(
            client,
            &params.user_id,
            &params.project_id,
            &params.jobset_id,
            &params.job_name,
        )
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM starred_jobs WHERE id = $1", None)
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
    ) -> StarredJobRowQuery<'c, 'a, 's, C, StarredJobRow, 1> {
        StarredJobRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<StarredJobRowBorrowed, tokio_postgres::Error> {
                    Ok(StarredJobRowBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        project_id: row.try_get(2)?,
                        jobset_id: row.try_get(3)?,
                        job_name: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                    })
                },
            mapper: |it| StarredJobRow::from(it),
        }
    }
}
pub struct ListForUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_user() -> ListForUserStmt {
    ListForUserStmt(
        "SELECT * FROM starred_jobs WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        None,
    )
}
impl ListForUserStmt {
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
        user_id: &'a uuid::Uuid,
        limit: &'a i64,
        offset: &'a i64,
    ) -> StarredJobRowQuery<'c, 'a, 's, C, StarredJobRow, 3> {
        StarredJobRowQuery {
            client,
            params: [user_id, limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<StarredJobRowBorrowed, tokio_postgres::Error> {
                    Ok(StarredJobRowBorrowed {
                        id: row.try_get(0)?,
                        user_id: row.try_get(1)?,
                        project_id: row.try_get(2)?,
                        jobset_id: row.try_get(3)?,
                        job_name: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                    })
                },
            mapper: |it| StarredJobRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListForUserParams,
        StarredJobRowQuery<'c, 'a, 's, C, StarredJobRow, 3>,
        C,
    > for ListForUserStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListForUserParams,
    ) -> StarredJobRowQuery<'c, 'a, 's, C, StarredJobRow, 3> {
        self.bind(client, &params.user_id, &params.limit, &params.offset)
    }
}
pub struct CountForUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_for_user() -> CountForUserStmt {
    CountForUserStmt("SELECT COUNT(*) FROM starred_jobs WHERE user_id = $1", None)
}
impl CountForUserStmt {
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
        user_id: &'a uuid::Uuid,
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct IsStarredStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn is_starred() -> IsStarredStmt {
    IsStarredStmt(
        "SELECT COUNT(*) FROM starred_jobs WHERE user_id = $1 AND project_id = $2 AND jobset_id IS NOT DISTINCT FROM $3 AND job_name = $4",
        None,
    )
}
impl IsStarredStmt {
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
        user_id: &'a uuid::Uuid,
        project_id: &'a uuid::Uuid,
        jobset_id: &'a Option<uuid::Uuid>,
        job_name: &'a T1,
    ) -> I64Query<'c, 'a, 's, C, i64, 4> {
        I64Query {
            client,
            params: [user_id, project_id, jobset_id, job_name],
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
        IsStarredParams<T1>,
        I64Query<'c, 'a, 's, C, i64, 4>,
        C,
    > for IsStarredStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a IsStarredParams<T1>,
    ) -> I64Query<'c, 'a, 's, C, i64, 4> {
        self.bind(
            client,
            &params.user_id,
            &params.project_id,
            &params.jobset_id,
            &params.job_name,
        )
    }
}
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("DELETE FROM starred_jobs WHERE id = $1", None)
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
pub struct DeleteForUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_for_user() -> DeleteForUserStmt {
    DeleteForUserStmt(
        "DELETE FROM starred_jobs WHERE id = $1 AND user_id = $2",
        None,
    )
}
impl DeleteForUserStmt {
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
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[id, user_id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        DeleteForUserParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for DeleteForUserStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a DeleteForUserParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.id, &params.user_id))
    }
}
pub struct DeleteByJobStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_by_job() -> DeleteByJobStmt {
    DeleteByJobStmt(
        "DELETE FROM starred_jobs WHERE user_id = $1 AND project_id = $2 AND jobset_id IS NOT DISTINCT FROM $3 AND job_name = $4",
        None,
    )
}
impl DeleteByJobStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        user_id: &'a uuid::Uuid,
        project_id: &'a uuid::Uuid,
        jobset_id: &'a Option<uuid::Uuid>,
        job_name: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[user_id, project_id, jobset_id, job_name])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        DeleteByJobParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for DeleteByJobStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a DeleteByJobParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.user_id,
            &params.project_id,
            &params.jobset_id,
            &params.job_name,
        ))
    }
}
pub struct DeleteAllForUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_all_for_user() -> DeleteAllForUserStmt {
    DeleteAllForUserStmt("DELETE FROM starred_jobs WHERE user_id = $1", None)
}
impl DeleteAllForUserStmt {
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
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[user_id]).await
    }
}
