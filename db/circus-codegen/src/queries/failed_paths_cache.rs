// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct InsertParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub drv_path: T1,
    pub source_build_id: Option<uuid::Uuid>,
    pub failure_status: Option<T2>,
}
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
pub struct IsCachedFailureStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn is_cached_failure() -> IsCachedFailureStmt {
    IsCachedFailureStmt(
        "SELECT true AS exists FROM failed_paths_cache WHERE drv_path =$1",
        None,
    )
}
impl IsCachedFailureStmt {
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
    ) -> BoolQuery<'c, 'a, 's, C, bool, 1> {
        BoolQuery {
            client,
            params: [drv_path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct InsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn insert() -> InsertStmt {
    InsertStmt(
        "INSERT INTO failed_paths_cache ( drv_path, source_build_id, failure_status, failed_at ) VALUES ($1,$2,$3, NOW()) ON CONFLICT (drv_path) DO UPDATE SET source_build_id =$2, failure_status =$3, failed_at = NOW()",
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
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        drv_path: &'a T1,
        source_build_id: &'a Option<uuid::Uuid>,
        failure_status: &'a Option<T2>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[drv_path, source_build_id, failure_status])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        InsertParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for InsertStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a InsertParams<T1, T2>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.drv_path,
            &params.source_build_id,
            &params.failure_status,
        ))
    }
}
pub struct InvalidateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn invalidate() -> InvalidateStmt {
    InvalidateStmt("DELETE FROM failed_paths_cache WHERE drv_path =$1", None)
}
impl InvalidateStmt {
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
        drv_path: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[drv_path]).await
    }
}
pub struct CleanupExpiredStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn cleanup_expired() -> CleanupExpiredStmt {
    CleanupExpiredStmt(
        "DELETE FROM failed_paths_cache WHERE failed_at < NOW() - make_interval(secs =>$1)",
        None,
    )
}
impl CleanupExpiredStmt {
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
        ttl_seconds: &'a f64,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[ttl_seconds]).await
    }
}
