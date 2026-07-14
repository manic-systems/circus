// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct FlushParams<
    T1: crate::StringSql,
    T2: crate::ArraySql<Item = T1>,
    T3: crate::ArraySql<Item = i64>,
    T4: crate::ArraySql<Item = i64>,
> {
    pub cache_names: T2,
    pub requests: T3,
    pub bytes_served: T4,
}
#[derive(Debug)]
pub struct TrafficTimeseriesParams<T1: crate::StringSql> {
    pub bucket_seconds: i64,
    pub cache_name: T1,
    pub window_seconds: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct StorageTimeseriesParams {
    pub project_id: Option<uuid::Uuid>,
    pub bucket_seconds: i64,
    pub window_seconds: i64,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct TrafficTimeseries {
    pub bucket_time: chrono::DateTime<chrono::Utc>,
    pub requests: i64,
    pub bytes: i64,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct StorageTimeseries {
    pub bucket_time: chrono::DateTime<chrono::Utc>,
    pub packages_added: i64,
    pub bytes_added: i64,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct TrafficLastHour {
    pub requests: i64,
    pub bytes_served: i64,
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct TrafficTimeseriesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<TrafficTimeseries, tokio_postgres::Error>,
    mapper: fn(TrafficTimeseries) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> TrafficTimeseriesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(TrafficTimeseries) -> R,
    ) -> TrafficTimeseriesQuery<'c, 'a, 's, C, R, N> {
        TrafficTimeseriesQuery {
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
pub struct StorageTimeseriesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<StorageTimeseries, tokio_postgres::Error>,
    mapper: fn(StorageTimeseries) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> StorageTimeseriesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(StorageTimeseries) -> R,
    ) -> StorageTimeseriesQuery<'c, 'a, 's, C, R, N> {
        StorageTimeseriesQuery {
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
pub struct TrafficLastHourQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<TrafficLastHour, tokio_postgres::Error>,
    mapper: fn(TrafficLastHour) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> TrafficLastHourQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(TrafficLastHour) -> R,
    ) -> TrafficLastHourQuery<'c, 'a, 's, C, R, N> {
        TrafficLastHourQuery {
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
pub struct FlushStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn flush() -> FlushStmt {
    FlushStmt(
        "INSERT INTO cache_traffic (cache_name, requests, bytes_served) SELECT * FROM UNNEST($1::text[], $2::bigint[], $3::bigint[])",
        None,
    )
}
impl FlushStmt {
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
        T3: crate::ArraySql<Item = i64>,
        T4: crate::ArraySql<Item = i64>,
    >(
        &'s self,
        client: &'c C,
        cache_names: &'a T2,
        requests: &'a T3,
        bytes_served: &'a T4,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[cache_names, requests, bytes_served])
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::ArraySql<Item = T1>,
    T3: crate::ArraySql<Item = i64>,
    T4: crate::ArraySql<Item = i64>,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        FlushParams<T1, T2, T3, T4>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for FlushStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a FlushParams<T1, T2, T3, T4>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.cache_names,
            &params.requests,
            &params.bytes_served,
        ))
    }
}
pub struct TrafficTimeseriesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn traffic_timeseries() -> TrafficTimeseriesStmt {
    TrafficTimeseriesStmt(
        "SELECT to_timestamp(floor(extract(epoch FROM recorded_at) / $1::bigint) * $1::bigint) AS bucket_time, COALESCE(SUM(requests), 0)::bigint AS requests, COALESCE(SUM(bytes_served), 0)::bigint AS bytes FROM cache_traffic WHERE cache_name = $2 AND recorded_at > NOW() - ($3::bigint * INTERVAL '1 second') GROUP BY bucket_time ORDER BY bucket_time ASC",
        None,
    )
}
impl TrafficTimeseriesStmt {
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
        bucket_seconds: &'a i64,
        cache_name: &'a T1,
        window_seconds: &'a i64,
    ) -> TrafficTimeseriesQuery<'c, 'a, 's, C, TrafficTimeseries, 3> {
        TrafficTimeseriesQuery {
            client,
            params: [bucket_seconds, cache_name, window_seconds],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<TrafficTimeseries, tokio_postgres::Error> {
                    Ok(TrafficTimeseries {
                        bucket_time: row.try_get(0)?,
                        requests: row.try_get(1)?,
                        bytes: row.try_get(2)?,
                    })
                },
            mapper: |it| TrafficTimeseries::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        TrafficTimeseriesParams<T1>,
        TrafficTimeseriesQuery<'c, 'a, 's, C, TrafficTimeseries, 3>,
        C,
    > for TrafficTimeseriesStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a TrafficTimeseriesParams<T1>,
    ) -> TrafficTimeseriesQuery<'c, 'a, 's, C, TrafficTimeseries, 3> {
        self.bind(
            client,
            &params.bucket_seconds,
            &params.cache_name,
            &params.window_seconds,
        )
    }
}
pub struct StorageTimeseriesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn storage_timeseries() -> StorageTimeseriesStmt {
    StorageTimeseriesStmt(
        "WITH uploaded AS ( SELECT store_path, created_at, file_size FROM narinfo_cache n WHERE ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $1)) ), local AS ( SELECT DISTINCT ON (path) path AS store_path, created_at, COALESCE(file_size, 0) AS file_size FROM ( SELECT bp.path, bp.created_at, bp.file_size FROM build_products bp JOIN builds b ON b.id = bp.build_id JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND ($1::uuid IS NULL OR j.project_id = $1) UNION ALL SELECT b.build_output_path AS path, COALESCE(b.completed_at, b.created_at) AS created_at, NULL::bigint AS file_size FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND b.build_output_path IS NOT NULL AND ($1::uuid IS NULL OR j.project_id = $1) ) candidates WHERE NOT EXISTS ( SELECT 1 FROM narinfo_cache n WHERE n.store_path = candidates.path AND ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $1)) ) ORDER BY path, created_at DESC ), inventory AS ( SELECT * FROM uploaded UNION ALL SELECT * FROM local ) SELECT to_timestamp(floor(extract(epoch FROM created_at) / $2::bigint) * $2::bigint) AS bucket_time, COUNT(*) AS packages_added, COALESCE(SUM(file_size), 0)::bigint AS bytes_added FROM inventory WHERE created_at > NOW() - ($3::bigint * INTERVAL '1 second') GROUP BY bucket_time ORDER BY bucket_time ASC",
        None,
    )
}
impl StorageTimeseriesStmt {
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
        bucket_seconds: &'a i64,
        window_seconds: &'a i64,
    ) -> StorageTimeseriesQuery<'c, 'a, 's, C, StorageTimeseries, 3> {
        StorageTimeseriesQuery {
            client,
            params: [project_id, bucket_seconds, window_seconds],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<StorageTimeseries, tokio_postgres::Error> {
                    Ok(StorageTimeseries {
                        bucket_time: row.try_get(0)?,
                        packages_added: row.try_get(1)?,
                        bytes_added: row.try_get(2)?,
                    })
                },
            mapper: |it| StorageTimeseries::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        StorageTimeseriesParams,
        StorageTimeseriesQuery<'c, 'a, 's, C, StorageTimeseries, 3>,
        C,
    > for StorageTimeseriesStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a StorageTimeseriesParams,
    ) -> StorageTimeseriesQuery<'c, 'a, 's, C, StorageTimeseries, 3> {
        self.bind(
            client,
            &params.project_id,
            &params.bucket_seconds,
            &params.window_seconds,
        )
    }
}
pub struct TrafficLastHourStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn traffic_last_hour() -> TrafficLastHourStmt {
    TrafficLastHourStmt(
        "SELECT COALESCE(SUM(requests), 0)::bigint AS requests, COALESCE(SUM(bytes_served), 0)::bigint AS bytes_served FROM cache_traffic WHERE cache_name = $1 AND recorded_at > NOW() - INTERVAL '1 hour'",
        None,
    )
}
impl TrafficLastHourStmt {
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
        cache_name: &'a T1,
    ) -> TrafficLastHourQuery<'c, 'a, 's, C, TrafficLastHour, 1> {
        TrafficLastHourQuery {
            client,
            params: [cache_name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<TrafficLastHour, tokio_postgres::Error> {
                    Ok(TrafficLastHour {
                        requests: row.try_get(0)?,
                        bytes_served: row.try_get(1)?,
                    })
                },
            mapper: |it| TrafficLastHour::from(it),
        }
    }
}
