// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct UpsertParams<T1: crate::StringSql> {
    pub build_id: uuid::Uuid,
    pub retry_count: i32,
    pub token_hash: T1,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct ActiveProject {
    pub project_id: uuid::Uuid,
    pub build_id: uuid::Uuid,
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct ActiveProjectQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ActiveProject, tokio_postgres::Error>,
    mapper: fn(ActiveProject) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ActiveProjectQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ActiveProject) -> R) -> ActiveProjectQuery<'c, 'a, 's, C, R, N> {
        ActiveProjectQuery {
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
pub struct UpsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert() -> UpsertStmt {
    UpsertStmt(
        "INSERT INTO effect_task_tokens (build_id, retry_count, token_hash) VALUES ($1, $2, $3) ON CONFLICT (build_id) DO UPDATE SET retry_count = EXCLUDED.retry_count, token_hash = EXCLUDED.token_hash, created_at = NOW()",
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
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        build_id: &'a uuid::Uuid,
        retry_count: &'a i32,
        token_hash: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[build_id, retry_count, token_hash])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpsertParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpsertStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpsertParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.build_id,
            &params.retry_count,
            &params.token_hash,
        ))
    }
}
pub struct ActiveProjectStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn active_project() -> ActiveProjectStmt {
    ActiveProjectStmt(
        "SELECT j.project_id, b.id AS build_id FROM effect_task_tokens t JOIN builds b ON b.id = t.build_id JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE t.token_hash = $1 AND b.retry_count = t.retry_count AND b.kind = 'effect' AND b.status = 'running' AND b.effect_execution_active",
        None,
    )
}
impl ActiveProjectStmt {
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
        token_hash: &'a T1,
    ) -> ActiveProjectQuery<'c, 'a, 's, C, ActiveProject, 1> {
        ActiveProjectQuery {
            client,
            params: [token_hash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &tokio_postgres::Row| -> Result<ActiveProject, tokio_postgres::Error> {
                Ok(ActiveProject {
                    project_id: row.try_get(0)?,
                    build_id: row.try_get(1)?,
                })
            },
            mapper: |it| ActiveProject::from(it),
        }
    }
}
