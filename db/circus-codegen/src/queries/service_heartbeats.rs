// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct RecordParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub service: T1,
    pub poll_interval_seconds: i32,
    pub version: Option<T2>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ListStatus {
    pub service: String,
    pub last_heartbeat_at: chrono::DateTime<chrono::Utc>,
    pub seconds_since: f64,
    pub poll_interval_seconds: i32,
    pub version: Option<String>,
}
pub struct ListStatusBorrowed<'a> {
    pub service: &'a str,
    pub last_heartbeat_at: chrono::DateTime<chrono::Utc>,
    pub seconds_since: f64,
    pub poll_interval_seconds: i32,
    pub version: Option<&'a str>,
}
impl<'a> From<ListStatusBorrowed<'a>> for ListStatus {
    fn from(
        ListStatusBorrowed {
            service,
            last_heartbeat_at,
            seconds_since,
            poll_interval_seconds,
            version,
        }: ListStatusBorrowed<'a>,
    ) -> Self {
        Self {
            service: service.into(),
            last_heartbeat_at,
            seconds_since,
            poll_interval_seconds,
            version: version.map(|v| v.into()),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct ListStatusQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ListStatusBorrowed, tokio_postgres::Error>,
    mapper: fn(ListStatusBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ListStatusQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(ListStatusBorrowed) -> R,
    ) -> ListStatusQuery<'c, 'a, 's, C, R, N> {
        ListStatusQuery {
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
pub struct RecordStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record() -> RecordStmt {
    RecordStmt(
        "INSERT INTO service_heartbeats ( service, last_heartbeat_at, poll_interval_seconds, version ) VALUES ( $1, NOW(), $2, $3 ) ON CONFLICT (service) DO UPDATE SET last_heartbeat_at = EXCLUDED.last_heartbeat_at, poll_interval_seconds = EXCLUDED.poll_interval_seconds, version = EXCLUDED.version",
        None,
    )
}
impl RecordStmt {
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
        service: &'a T1,
        poll_interval_seconds: &'a i32,
        version: &'a Option<T2>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[service, poll_interval_seconds, version])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        RecordParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for RecordStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a RecordParams<T1, T2>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.service,
            &params.poll_interval_seconds,
            &params.version,
        ))
    }
}
pub struct ListStatusStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_status() -> ListStatusStmt {
    ListStatusStmt(
        "SELECT service, last_heartbeat_at, EXTRACT( EPOCH FROM (NOW() - last_heartbeat_at) )::float8 AS seconds_since, poll_interval_seconds, version FROM service_heartbeats",
        None,
    )
}
impl ListStatusStmt {
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
    ) -> ListStatusQuery<'c, 'a, 's, C, ListStatus, 0> {
        ListStatusQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ListStatusBorrowed, tokio_postgres::Error> {
                    Ok(ListStatusBorrowed {
                        service: row.try_get(0)?,
                        last_heartbeat_at: row.try_get(1)?,
                        seconds_since: row.try_get(2)?,
                        poll_interval_seconds: row.try_get(3)?,
                        version: row.try_get(4)?,
                    })
                },
            mapper: |it| ListStatus::from(it),
        }
    }
}
