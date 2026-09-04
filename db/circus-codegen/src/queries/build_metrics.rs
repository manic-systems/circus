// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct UpsertParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub build_id: uuid::Uuid,
    pub metric_name: T1,
    pub metric_value: f64,
    pub unit: T2,
}
#[derive(Clone, Copy, Debug)]
pub struct CalculateFailureRateParams {
    pub project_id: Option<uuid::Uuid>,
    pub jobset_id: Option<uuid::Uuid>,
    pub window_minutes: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct GetBuildStatsTimeseriesParams {
    pub bucket_minutes: i32,
    pub hours: f64,
    pub project_id: Option<uuid::Uuid>,
    pub jobset_id: Option<uuid::Uuid>,
}
#[derive(Clone, Copy, Debug)]
pub struct GetDurationPercentilesTimeseriesParams {
    pub bucket_minutes: i32,
    pub hours: f64,
    pub project_id: Option<uuid::Uuid>,
    pub jobset_id: Option<uuid::Uuid>,
}
#[derive(Clone, Copy, Debug)]
pub struct GetQueueDepthTimeseriesParams {
    pub bucket_minutes: i32,
    pub hours: f64,
}
#[derive(Clone, Copy, Debug)]
pub struct GetSystemDistributionParams {
    pub hours: f64,
    pub project_id: Option<uuid::Uuid>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BuildMetricRow {
    pub id: uuid::Uuid,
    pub build_id: uuid::Uuid,
    pub metric_name: String,
    pub metric_value: f64,
    pub unit: String,
    pub collected_at: chrono::DateTime<chrono::Utc>,
}
pub struct BuildMetricRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub build_id: uuid::Uuid,
    pub metric_name: &'a str,
    pub metric_value: f64,
    pub unit: &'a str,
    pub collected_at: chrono::DateTime<chrono::Utc>,
}
impl<'a> From<BuildMetricRowBorrowed<'a>> for BuildMetricRow {
    fn from(
        BuildMetricRowBorrowed {
            id,
            build_id,
            metric_name,
            metric_value,
            unit,
            collected_at,
        }: BuildMetricRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            build_id,
            metric_name: metric_name.into(),
            metric_value,
            unit: unit.into(),
            collected_at,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct CalculateFailureRate {
    pub id: uuid::Uuid,
    pub status: String,
}
pub struct CalculateFailureRateBorrowed<'a> {
    pub id: uuid::Uuid,
    pub status: &'a str,
}
impl<'a> From<CalculateFailureRateBorrowed<'a>> for CalculateFailureRate {
    fn from(CalculateFailureRateBorrowed { id, status }: CalculateFailureRateBorrowed<'a>) -> Self {
        Self {
            id,
            status: status.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct GetBuildStatsTimeseries {
    pub bucket_time: chrono::DateTime<chrono::Utc>,
    pub total_builds: i64,
    pub failed_builds: i64,
    pub avg_duration: Option<rust_decimal::Decimal>,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct GetDurationPercentilesTimeseries {
    pub bucket_time: chrono::DateTime<chrono::Utc>,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub p99: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct GetQueueDepthTimeseries {
    pub bucket_time: chrono::DateTime<chrono::Utc>,
    pub pending_count: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct GetSystemDistribution {
    pub system: String,
    pub build_count: i64,
}
pub struct GetSystemDistributionBorrowed<'a> {
    pub system: &'a str,
    pub build_count: i64,
}
impl<'a> From<GetSystemDistributionBorrowed<'a>> for GetSystemDistribution {
    fn from(
        GetSystemDistributionBorrowed {
            system,
            build_count,
        }: GetSystemDistributionBorrowed<'a>,
    ) -> Self {
        Self {
            system: system.into(),
            build_count,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationsByStatus {
    pub status: String,
    pub count: i64,
}
pub struct EvaluationsByStatusBorrowed<'a> {
    pub status: &'a str,
    pub count: i64,
}
impl<'a> From<EvaluationsByStatusBorrowed<'a>> for EvaluationsByStatus {
    fn from(
        EvaluationsByStatusBorrowed { status, count }: EvaluationsByStatusBorrowed<'a>,
    ) -> Self {
        Self {
            status: status.into(),
            count,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct OverviewCounts {
    pub project_count: i64,
    pub channel_count: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PerProjectBuildCounts {
    pub name: String,
    pub succeeded_count: i64,
    pub failed_count: i64,
}
pub struct PerProjectBuildCountsBorrowed<'a> {
    pub name: &'a str,
    pub succeeded_count: i64,
    pub failed_count: i64,
}
impl<'a> From<PerProjectBuildCountsBorrowed<'a>> for PerProjectBuildCounts {
    fn from(
        PerProjectBuildCountsBorrowed {
            name,
            succeeded_count,
            failed_count,
        }: PerProjectBuildCountsBorrowed<'a>,
    ) -> Self {
        Self {
            name: name.into(),
            succeeded_count,
            failed_count,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct DurationPercentilesOverall {
    pub duration_p50: Option<f64>,
    pub duration_p95: Option<f64>,
    pub duration_p99: Option<f64>,
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct BuildMetricRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<BuildMetricRowBorrowed, tokio_postgres::Error>,
    mapper: fn(BuildMetricRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BuildMetricRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(BuildMetricRowBorrowed) -> R,
    ) -> BuildMetricRowQuery<'c, 'a, 's, C, R, N> {
        BuildMetricRowQuery {
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
pub struct CalculateFailureRateQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<CalculateFailureRateBorrowed, tokio_postgres::Error>,
    mapper: fn(CalculateFailureRateBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> CalculateFailureRateQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(CalculateFailureRateBorrowed) -> R,
    ) -> CalculateFailureRateQuery<'c, 'a, 's, C, R, N> {
        CalculateFailureRateQuery {
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
pub struct GetBuildStatsTimeseriesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<GetBuildStatsTimeseries, tokio_postgres::Error>,
    mapper: fn(GetBuildStatsTimeseries) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> GetBuildStatsTimeseriesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(GetBuildStatsTimeseries) -> R,
    ) -> GetBuildStatsTimeseriesQuery<'c, 'a, 's, C, R, N> {
        GetBuildStatsTimeseriesQuery {
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
pub struct GetDurationPercentilesTimeseriesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<GetDurationPercentilesTimeseries, tokio_postgres::Error>,
    mapper: fn(GetDurationPercentilesTimeseries) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize>
    GetDurationPercentilesTimeseriesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(GetDurationPercentilesTimeseries) -> R,
    ) -> GetDurationPercentilesTimeseriesQuery<'c, 'a, 's, C, R, N> {
        GetDurationPercentilesTimeseriesQuery {
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
pub struct GetQueueDepthTimeseriesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<GetQueueDepthTimeseries, tokio_postgres::Error>,
    mapper: fn(GetQueueDepthTimeseries) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> GetQueueDepthTimeseriesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(GetQueueDepthTimeseries) -> R,
    ) -> GetQueueDepthTimeseriesQuery<'c, 'a, 's, C, R, N> {
        GetQueueDepthTimeseriesQuery {
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
pub struct GetSystemDistributionQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<GetSystemDistributionBorrowed, tokio_postgres::Error>,
    mapper: fn(GetSystemDistributionBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> GetSystemDistributionQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(GetSystemDistributionBorrowed) -> R,
    ) -> GetSystemDistributionQuery<'c, 'a, 's, C, R, N> {
        GetSystemDistributionQuery {
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
pub struct EvaluationsByStatusQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<EvaluationsByStatusBorrowed, tokio_postgres::Error>,
    mapper: fn(EvaluationsByStatusBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> EvaluationsByStatusQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(EvaluationsByStatusBorrowed) -> R,
    ) -> EvaluationsByStatusQuery<'c, 'a, 's, C, R, N> {
        EvaluationsByStatusQuery {
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
pub struct OverviewCountsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<OverviewCounts, tokio_postgres::Error>,
    mapper: fn(OverviewCounts) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> OverviewCountsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(OverviewCounts) -> R,
    ) -> OverviewCountsQuery<'c, 'a, 's, C, R, N> {
        OverviewCountsQuery {
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
pub struct PerProjectBuildCountsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<PerProjectBuildCountsBorrowed, tokio_postgres::Error>,
    mapper: fn(PerProjectBuildCountsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> PerProjectBuildCountsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(PerProjectBuildCountsBorrowed) -> R,
    ) -> PerProjectBuildCountsQuery<'c, 'a, 's, C, R, N> {
        PerProjectBuildCountsQuery {
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
pub struct DurationPercentilesOverallQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<DurationPercentilesOverall, tokio_postgres::Error>,
    mapper: fn(DurationPercentilesOverall) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> DurationPercentilesOverallQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(DurationPercentilesOverall) -> R,
    ) -> DurationPercentilesOverallQuery<'c, 'a, 's, C, R, N> {
        DurationPercentilesOverallQuery {
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
        "INSERT INTO build_metrics (build_id, metric_name, metric_value, unit) VALUES ($1,$2,$3,$4) ON CONFLICT (build_id, metric_name) DO UPDATE SET metric_value = EXCLUDED.metric_value, collected_at = NOW() RETURNING *",
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
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        build_id: &'a uuid::Uuid,
        metric_name: &'a T1,
        metric_value: &'a f64,
        unit: &'a T2,
    ) -> BuildMetricRowQuery<'c, 'a, 's, C, BuildMetricRow, 4> {
        BuildMetricRowQuery {
            client,
            params: [build_id, metric_name, metric_value, unit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuildMetricRowBorrowed, tokio_postgres::Error> {
                Ok(BuildMetricRowBorrowed {
                    id: row.try_get(0)?,
                    build_id: row.try_get(1)?,
                    metric_name: row.try_get(2)?,
                    metric_value: row.try_get(3)?,
                    unit: row.try_get(4)?,
                    collected_at: row.try_get(5)?,
                })
            },
            mapper: |it| BuildMetricRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpsertParams<T1, T2>,
        BuildMetricRowQuery<'c, 'a, 's, C, BuildMetricRow, 4>,
        C,
    > for UpsertStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpsertParams<T1, T2>,
    ) -> BuildMetricRowQuery<'c, 'a, 's, C, BuildMetricRow, 4> {
        self.bind(
            client,
            &params.build_id,
            &params.metric_name,
            &params.metric_value,
            &params.unit,
        )
    }
}
pub struct CalculateFailureRateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn calculate_failure_rate() -> CalculateFailureRateStmt {
    CalculateFailureRateStmt(
        "SELECT b.id, b.status::text AS status FROM builds b JOIN evaluations e ON b.evaluation_id = e.id JOIN jobsets j ON e.jobset_id = j.id WHERE ( $1::uuid IS NULL OR j.project_id =$1 ) AND ( $2::uuid IS NULL OR j.id =$2 ) AND b.completed_at > NOW() - (INTERVAL '1 minute' *$3) ORDER BY b.completed_at DESC",
        None,
    )
}
impl CalculateFailureRateStmt {
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
        jobset_id: &'a Option<uuid::Uuid>,
        window_minutes: &'a f64,
    ) -> CalculateFailureRateQuery<'c, 'a, 's, C, CalculateFailureRate, 3> {
        CalculateFailureRateQuery {
            client,
            params: [project_id, jobset_id, window_minutes],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<CalculateFailureRateBorrowed, tokio_postgres::Error> {
                Ok(CalculateFailureRateBorrowed {
                    id: row.try_get(0)?,
                    status: row.try_get(1)?,
                })
            },
            mapper: |it| CalculateFailureRate::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CalculateFailureRateParams,
        CalculateFailureRateQuery<'c, 'a, 's, C, CalculateFailureRate, 3>,
        C,
    > for CalculateFailureRateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CalculateFailureRateParams,
    ) -> CalculateFailureRateQuery<'c, 'a, 's, C, CalculateFailureRate, 3> {
        self.bind(
            client,
            &params.project_id,
            &params.jobset_id,
            &params.window_minutes,
        )
    }
}
pub struct GetBuildStatsTimeseriesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_build_stats_timeseries() -> GetBuildStatsTimeseriesStmt {
    GetBuildStatsTimeseriesStmt(
        "SELECT date_trunc('minute', b.completed_at) + ( EXTRACT( MINUTE FROM b.completed_at )::int /$1 ) * INTERVAL '1 minute' *$1 AS bucket_time, COUNT(*) AS total_builds, COUNT(*) FILTER ( WHERE b.status = 'failed' ) AS failed_builds, AVG( EXTRACT( EPOCH FROM (b.completed_at - b.started_at) ) ) AS avg_duration FROM builds b JOIN evaluations e ON b.evaluation_id = e.id JOIN jobsets j ON e.jobset_id = j.id WHERE b.completed_at IS NOT NULL AND b.completed_at > NOW() - (INTERVAL '1 hour' *$2) AND ( $3::uuid IS NULL OR j.project_id =$3 ) AND ( $4::uuid IS NULL OR j.id =$4 ) GROUP BY bucket_time ORDER BY bucket_time ASC",
        None,
    )
}
impl GetBuildStatsTimeseriesStmt {
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
        bucket_minutes: &'a i32,
        hours: &'a f64,
        project_id: &'a Option<uuid::Uuid>,
        jobset_id: &'a Option<uuid::Uuid>,
    ) -> GetBuildStatsTimeseriesQuery<'c, 'a, 's, C, GetBuildStatsTimeseries, 4> {
        GetBuildStatsTimeseriesQuery {
            client,
            params: [bucket_minutes, hours, project_id, jobset_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<GetBuildStatsTimeseries, tokio_postgres::Error> {
                Ok(GetBuildStatsTimeseries {
                    bucket_time: row.try_get(0)?,
                    total_builds: row.try_get(1)?,
                    failed_builds: row.try_get(2)?,
                    avg_duration: row.try_get(3)?,
                })
            },
            mapper: |it| GetBuildStatsTimeseries::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetBuildStatsTimeseriesParams,
        GetBuildStatsTimeseriesQuery<'c, 'a, 's, C, GetBuildStatsTimeseries, 4>,
        C,
    > for GetBuildStatsTimeseriesStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetBuildStatsTimeseriesParams,
    ) -> GetBuildStatsTimeseriesQuery<'c, 'a, 's, C, GetBuildStatsTimeseries, 4> {
        self.bind(
            client,
            &params.bucket_minutes,
            &params.hours,
            &params.project_id,
            &params.jobset_id,
        )
    }
}
pub struct GetDurationPercentilesTimeseriesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_duration_percentiles_timeseries() -> GetDurationPercentilesTimeseriesStmt {
    GetDurationPercentilesTimeseriesStmt(
        "SELECT date_trunc('minute', b.completed_at) + ( EXTRACT( MINUTE FROM b.completed_at )::int /$1 ) * INTERVAL '1 minute' *$1 AS bucket_time, PERCENTILE_CONT(0.5) WITHIN GROUP ( ORDER BY EXTRACT( EPOCH FROM (b.completed_at - b.started_at) ) ) AS p50, PERCENTILE_CONT(0.95) WITHIN GROUP ( ORDER BY EXTRACT( EPOCH FROM (b.completed_at - b.started_at) ) ) AS p95, PERCENTILE_CONT(0.99) WITHIN GROUP ( ORDER BY EXTRACT( EPOCH FROM (b.completed_at - b.started_at) ) ) AS p99 FROM builds b JOIN evaluations e ON b.evaluation_id = e.id JOIN jobsets j ON e.jobset_id = j.id WHERE b.completed_at IS NOT NULL AND b.started_at IS NOT NULL AND b.completed_at > NOW() - (INTERVAL '1 hour' *$2) AND ( $3::uuid IS NULL OR j.project_id =$3 ) AND ( $4::uuid IS NULL OR j.id =$4 ) GROUP BY bucket_time ORDER BY bucket_time ASC",
        None,
    )
}
impl GetDurationPercentilesTimeseriesStmt {
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
        bucket_minutes: &'a i32,
        hours: &'a f64,
        project_id: &'a Option<uuid::Uuid>,
        jobset_id: &'a Option<uuid::Uuid>,
    ) -> GetDurationPercentilesTimeseriesQuery<'c, 'a, 's, C, GetDurationPercentilesTimeseries, 4>
    {
        GetDurationPercentilesTimeseriesQuery {
            client,
            params: [bucket_minutes, hours, project_id, jobset_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<GetDurationPercentilesTimeseries, tokio_postgres::Error> {
                Ok(GetDurationPercentilesTimeseries {
                    bucket_time: row.try_get(0)?,
                    p50: row.try_get(1)?,
                    p95: row.try_get(2)?,
                    p99: row.try_get(3)?,
                })
            },
            mapper: |it| GetDurationPercentilesTimeseries::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetDurationPercentilesTimeseriesParams,
        GetDurationPercentilesTimeseriesQuery<'c, 'a, 's, C, GetDurationPercentilesTimeseries, 4>,
        C,
    > for GetDurationPercentilesTimeseriesStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetDurationPercentilesTimeseriesParams,
    ) -> GetDurationPercentilesTimeseriesQuery<'c, 'a, 's, C, GetDurationPercentilesTimeseries, 4>
    {
        self.bind(
            client,
            &params.bucket_minutes,
            &params.hours,
            &params.project_id,
            &params.jobset_id,
        )
    }
}
pub struct GetQueueDepthTimeseriesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_queue_depth_timeseries() -> GetQueueDepthTimeseriesStmt {
    GetQueueDepthTimeseriesStmt(
        "SELECT date_trunc('minute', created_at) + ( EXTRACT( MINUTE FROM created_at )::int /$1 ) * INTERVAL '1 minute' *$1 AS bucket_time, COUNT(*) FILTER ( WHERE status = 'pending' ) AS pending_count FROM builds WHERE created_at > NOW() - (INTERVAL '1 hour' *$2) GROUP BY bucket_time ORDER BY bucket_time ASC",
        None,
    )
}
impl GetQueueDepthTimeseriesStmt {
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
        bucket_minutes: &'a i32,
        hours: &'a f64,
    ) -> GetQueueDepthTimeseriesQuery<'c, 'a, 's, C, GetQueueDepthTimeseries, 2> {
        GetQueueDepthTimeseriesQuery {
            client,
            params: [bucket_minutes, hours],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<GetQueueDepthTimeseries, tokio_postgres::Error> {
                Ok(GetQueueDepthTimeseries {
                    bucket_time: row.try_get(0)?,
                    pending_count: row.try_get(1)?,
                })
            },
            mapper: |it| GetQueueDepthTimeseries::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetQueueDepthTimeseriesParams,
        GetQueueDepthTimeseriesQuery<'c, 'a, 's, C, GetQueueDepthTimeseries, 2>,
        C,
    > for GetQueueDepthTimeseriesStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetQueueDepthTimeseriesParams,
    ) -> GetQueueDepthTimeseriesQuery<'c, 'a, 's, C, GetQueueDepthTimeseries, 2> {
        self.bind(client, &params.bucket_minutes, &params.hours)
    }
}
pub struct GetSystemDistributionStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_system_distribution() -> GetSystemDistributionStmt {
    GetSystemDistributionStmt(
        "SELECT COALESCE(b.system, 'unknown') AS system, COUNT(*) AS build_count FROM builds b JOIN evaluations e ON b.evaluation_id = e.id JOIN jobsets j ON e.jobset_id = j.id WHERE b.completed_at > NOW() - (INTERVAL '1 hour' *$1) AND ( $2::uuid IS NULL OR j.project_id =$2 ) GROUP BY b.system ORDER BY build_count DESC",
        None,
    )
}
impl GetSystemDistributionStmt {
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
        hours: &'a f64,
        project_id: &'a Option<uuid::Uuid>,
    ) -> GetSystemDistributionQuery<'c, 'a, 's, C, GetSystemDistribution, 2> {
        GetSystemDistributionQuery {
            client,
            params: [hours, project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<GetSystemDistributionBorrowed, tokio_postgres::Error> {
                Ok(GetSystemDistributionBorrowed {
                    system: row.try_get(0)?,
                    build_count: row.try_get(1)?,
                })
            },
            mapper: |it| GetSystemDistribution::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetSystemDistributionParams,
        GetSystemDistributionQuery<'c, 'a, 's, C, GetSystemDistribution, 2>,
        C,
    > for GetSystemDistributionStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetSystemDistributionParams,
    ) -> GetSystemDistributionQuery<'c, 'a, 's, C, GetSystemDistribution, 2> {
        self.bind(client, &params.hours, &params.project_id)
    }
}
pub struct CountEvaluationsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_evaluations() -> CountEvaluationsStmt {
    CountEvaluationsStmt("SELECT COUNT(*) FROM evaluations", None)
}
impl CountEvaluationsStmt {
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
pub struct EvaluationsByStatusStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn evaluations_by_status() -> EvaluationsByStatusStmt {
    EvaluationsByStatusStmt(
        "SELECT status::text AS status, COUNT(*) AS count FROM evaluations GROUP BY status",
        None,
    )
}
impl EvaluationsByStatusStmt {
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
    ) -> EvaluationsByStatusQuery<'c, 'a, 's, C, EvaluationsByStatus, 0> {
        EvaluationsByStatusQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<EvaluationsByStatusBorrowed, tokio_postgres::Error> {
                Ok(EvaluationsByStatusBorrowed {
                    status: row.try_get(0)?,
                    count: row.try_get(1)?,
                })
            },
            mapper: |it| EvaluationsByStatus::from(it),
        }
    }
}
pub struct OverviewCountsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn overview_counts() -> OverviewCountsStmt {
    OverviewCountsStmt(
        "SELECT ( SELECT COUNT(*) FROM projects ) AS project_count, ( SELECT COUNT(*) FROM channels ) AS channel_count",
        None,
    )
}
impl OverviewCountsStmt {
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
    ) -> OverviewCountsQuery<'c, 'a, 's, C, OverviewCounts, 0> {
        OverviewCountsQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<OverviewCounts, tokio_postgres::Error> {
                    Ok(OverviewCounts {
                        project_count: row.try_get(0)?,
                        channel_count: row.try_get(1)?,
                    })
                },
            mapper: |it| OverviewCounts::from(it),
        }
    }
}
pub struct PerProjectBuildCountsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn per_project_build_counts() -> PerProjectBuildCountsStmt {
    PerProjectBuildCountsStmt(
        "SELECT p.name, COUNT(*) FILTER ( WHERE b.status = 'succeeded' ) AS succeeded_count, COUNT(*) FILTER ( WHERE b.status = 'failed' ) AS failed_count FROM builds b JOIN evaluations e ON b.evaluation_id = e.id JOIN jobsets j ON e.jobset_id = j.id JOIN projects p ON j.project_id = p.id GROUP BY p.name",
        None,
    )
}
impl PerProjectBuildCountsStmt {
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
    ) -> PerProjectBuildCountsQuery<'c, 'a, 's, C, PerProjectBuildCounts, 0> {
        PerProjectBuildCountsQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<PerProjectBuildCountsBorrowed, tokio_postgres::Error> {
                Ok(PerProjectBuildCountsBorrowed {
                    name: row.try_get(0)?,
                    succeeded_count: row.try_get(1)?,
                    failed_count: row.try_get(2)?,
                })
            },
            mapper: |it| PerProjectBuildCounts::from(it),
        }
    }
}
pub struct DurationPercentilesOverallStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn duration_percentiles_overall() -> DurationPercentilesOverallStmt {
    DurationPercentilesOverallStmt(
        "SELECT PERCENTILE_CONT(0.5) WITHIN GROUP ( ORDER BY EXTRACT( EPOCH FROM (completed_at - started_at) ) ) AS duration_p50, PERCENTILE_CONT(0.95) WITHIN GROUP ( ORDER BY EXTRACT( EPOCH FROM (completed_at - started_at) ) ) AS duration_p95, PERCENTILE_CONT(0.99) WITHIN GROUP ( ORDER BY EXTRACT( EPOCH FROM (completed_at - started_at) ) ) AS duration_p99 FROM builds WHERE completed_at IS NOT NULL AND started_at IS NOT NULL",
        None,
    )
}
impl DurationPercentilesOverallStmt {
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
    ) -> DurationPercentilesOverallQuery<'c, 'a, 's, C, DurationPercentilesOverall, 0> {
        DurationPercentilesOverallQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<DurationPercentilesOverall, tokio_postgres::Error> {
                Ok(DurationPercentilesOverall {
                    duration_p50: row.try_get(0)?,
                    duration_p95: row.try_get(1)?,
                    duration_p99: row.try_get(2)?,
                })
            },
            mapper: |it| DurationPercentilesOverall::from(it),
        }
    }
}
