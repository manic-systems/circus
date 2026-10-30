// This file was generated with `cornucopia`. Do not modify.

use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct BoolQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<bool, tokio_postgres::Error>,
    mapper: fn(bool) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BoolQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(bool) -> R) -> BoolQuery<'c, 'a, 's, C, R, N> {
        BoolQuery {
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
pub struct InsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn insert() -> InsertStmt {
    InsertStmt(
        "INSERT INTO effect_git_token_requests (build_id) VALUES ($1) ON CONFLICT DO NOTHING",
        None,
    )
}
impl InsertStmt {
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
        build_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[build_id]).await
    }
}
pub struct RequestedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn requested() -> RequestedStmt {
    RequestedStmt(
        "SELECT EXISTS ( SELECT 1 FROM effect_git_token_requests WHERE build_id = $1 ) AS requested",
        None,
    )
}
impl RequestedStmt {
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
    ) -> BoolQuery<'c, 'a, 's, C, bool, 1> {
        BoolQuery {
            client,
            params: [build_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
