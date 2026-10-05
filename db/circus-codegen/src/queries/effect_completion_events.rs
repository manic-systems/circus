// This file was generated with `cornucopia`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct AckParams {
    pub build_id: uuid::Uuid,
    pub retry_count: i32,
    pub revision: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct EffectCompletionEventRow {
    pub build_id: uuid::Uuid,
    pub retry_count: i32,
    pub revision: i64,
    pub build_snapshot: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub acknowledged_at: Option<chrono::DateTime<chrono::Utc>>,
}
pub struct EffectCompletionEventRowBorrowed<'a> {
    pub build_id: uuid::Uuid,
    pub retry_count: i32,
    pub revision: i64,
    pub build_snapshot: postgres_types::Json<&'a serde_json::value::RawValue>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub acknowledged_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl<'a> From<EffectCompletionEventRowBorrowed<'a>> for EffectCompletionEventRow {
    fn from(
        EffectCompletionEventRowBorrowed {
            build_id,
            retry_count,
            revision,
            build_snapshot,
            created_at,
            updated_at,
            acknowledged_at,
        }: EffectCompletionEventRowBorrowed<'a>,
    ) -> Self {
        Self {
            build_id,
            retry_count,
            revision,
            build_snapshot: serde_json::from_str(build_snapshot.0.get()).unwrap(),
            created_at,
            updated_at,
            acknowledged_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct EffectCompletionEventRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<EffectCompletionEventRowBorrowed, tokio_postgres::Error>,
    mapper: fn(EffectCompletionEventRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> EffectCompletionEventRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(EffectCompletionEventRowBorrowed) -> R,
    ) -> EffectCompletionEventRowQuery<'c, 'a, 's, C, R, N> {
        EffectCompletionEventRowQuery {
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
pub struct ListPendingStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_pending() -> ListPendingStmt {
    ListPendingStmt(
        "SELECT build_id, retry_count, revision, build_snapshot, created_at, updated_at, acknowledged_at FROM effect_completion_events WHERE acknowledged_at IS NULL ORDER BY created_at, build_id, retry_count LIMIT $1",
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
        limit: &'a i64,
    ) -> EffectCompletionEventRowQuery<'c, 'a, 's, C, EffectCompletionEventRow, 1> {
        EffectCompletionEventRowQuery {
            client,
            params: [limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<EffectCompletionEventRowBorrowed, tokio_postgres::Error> {
                Ok(EffectCompletionEventRowBorrowed {
                    build_id: row.try_get(0)?,
                    retry_count: row.try_get(1)?,
                    revision: row.try_get(2)?,
                    build_snapshot: row.try_get(3)?,
                    created_at: row.try_get(4)?,
                    updated_at: row.try_get(5)?,
                    acknowledged_at: row.try_get(6)?,
                })
            },
            mapper: |it| EffectCompletionEventRow::from(it),
        }
    }
}
pub struct AckStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn ack() -> AckStmt {
    AckStmt(
        "UPDATE effect_completion_events SET acknowledged_at = NOW() WHERE build_id = $1 AND retry_count = $2 AND revision = $3 AND acknowledged_at IS NULL RETURNING build_id",
        None,
    )
}
impl AckStmt {
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
        retry_count: &'a i32,
        revision: &'a i64,
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 3> {
        UuidUuidQuery {
            client,
            params: [build_id, retry_count, revision],
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
        AckParams,
        UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 3>,
        C,
    > for AckStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a AckParams,
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 3> {
        self.bind(
            client,
            &params.build_id,
            &params.retry_count,
            &params.revision,
        )
    }
}
