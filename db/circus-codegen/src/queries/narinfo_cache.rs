// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct UpsertParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
    T8: crate::ArraySql<Item = T7>,
    T9: crate::StringSql,
    T10: crate::StringSql,
> {
    pub store_path: T1,
    pub nar_hash: T2,
    pub nar_size: i64,
    pub file_hash: Option<T3>,
    pub file_size: Option<i64>,
    pub compression: T4,
    pub url: T5,
    pub deriver: Option<T6>,
    pub references: T8,
    pub sig: Option<T9>,
    pub ca: Option<T10>,
    pub build_id: Option<uuid::Uuid>,
    pub project_id: Option<uuid::Uuid>,
}
#[derive(Debug)]
pub struct UpsertProjectOwnerParams<T1: crate::StringSql> {
    pub store_path: T1,
    pub project_id: uuid::Uuid,
    pub build_id: Option<uuid::Uuid>,
}
#[derive(Debug)]
pub struct GetByHashPartParams<T1: crate::StringSql> {
    pub hash_part_pattern: T1,
    pub project_id: Option<uuid::Uuid>,
}
#[derive(Debug)]
pub struct GetByUrlParams<T1: crate::StringSql> {
    pub url: T1,
    pub project_id: Option<uuid::Uuid>,
}
#[derive(Debug)]
pub struct ListFilteredParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub project_id: Option<uuid::Uuid>,
    pub hash_prefix: Option<T1>,
    pub package_query: Option<T2>,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct CountFilteredParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub project_id: Option<uuid::Uuid>,
    pub hash_prefix: Option<T1>,
    pub package_query: Option<T2>,
}
#[derive(Clone, Copy, Debug)]
pub struct ListGcCandidatesParams {
    pub cutoff: Option<chrono::DateTime<chrono::Utc>>,
    pub max_size_bytes: Option<i64>,
    pub target_size_bytes: Option<i64>,
}
#[derive(Clone, Copy, Debug)]
pub struct DeleteStaleProjectOwnersParams {
    pub project_id: uuid::Uuid,
    pub cutoff: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Clone, Copy, Debug)]
pub struct DeleteStaleForProjectParams {
    pub project_id: uuid::Uuid,
    pub cutoff: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct NarinfoCacheRow {
    pub store_path: String,
    pub nar_hash: String,
    pub nar_size: i64,
    pub file_hash: Option<String>,
    pub file_size: Option<i64>,
    pub compression: String,
    pub url: String,
    pub deriver: Option<String>,
    pub references: Vec<String>,
    pub sig: Option<String>,
    pub ca: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub build_id: Option<uuid::Uuid>,
    pub project_id: Option<uuid::Uuid>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}
pub struct NarinfoCacheRowBorrowed<'a> {
    pub store_path: &'a str,
    pub nar_hash: &'a str,
    pub nar_size: i64,
    pub file_hash: Option<&'a str>,
    pub file_size: Option<i64>,
    pub compression: &'a str,
    pub url: &'a str,
    pub deriver: Option<&'a str>,
    pub references: crate::ArrayIterator<'a, &'a str>,
    pub sig: Option<&'a str>,
    pub ca: Option<&'a str>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub build_id: Option<uuid::Uuid>,
    pub project_id: Option<uuid::Uuid>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl<'a> From<NarinfoCacheRowBorrowed<'a>> for NarinfoCacheRow {
    fn from(
        NarinfoCacheRowBorrowed {
            store_path,
            nar_hash,
            nar_size,
            file_hash,
            file_size,
            compression,
            url,
            deriver,
            references,
            sig,
            ca,
            created_at,
            updated_at,
            build_id,
            project_id,
            last_fetched_at,
        }: NarinfoCacheRowBorrowed<'a>,
    ) -> Self {
        Self {
            store_path: store_path.into(),
            nar_hash: nar_hash.into(),
            nar_size,
            file_hash: file_hash.map(|v| v.into()),
            file_size,
            compression: compression.into(),
            url: url.into(),
            deriver: deriver.map(|v| v.into()),
            references: references.map(|v| v.into()).collect(),
            sig: sig.map(|v| v.into()),
            ca: ca.map(|v| v.into()),
            created_at,
            updated_at,
            build_id,
            project_id,
            last_fetched_at,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct StorageSummary {
    pub nar_count: i64,
    pub uncompressed_bytes: i64,
    pub compressed_bytes: i64,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct StorageExtremes {
    pub last_uploaded: Option<chrono::DateTime<chrono::Utc>>,
    pub oldest_fetched: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ListFiltered {
    pub store_path: String,
    pub package_name: String,
    pub nar_size: i64,
    pub file_size: Option<i64>,
    pub compression: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}
pub struct ListFilteredBorrowed<'a> {
    pub store_path: &'a str,
    pub package_name: &'a str,
    pub nar_size: i64,
    pub file_size: Option<i64>,
    pub compression: &'a str,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_fetched_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl<'a> From<ListFilteredBorrowed<'a>> for ListFiltered {
    fn from(
        ListFilteredBorrowed {
            store_path,
            package_name,
            nar_size,
            file_size,
            compression,
            created_at,
            last_fetched_at,
        }: ListFilteredBorrowed<'a>,
    ) -> Self {
        Self {
            store_path: store_path.into(),
            package_name: package_name.into(),
            nar_size,
            file_size,
            compression: compression.into(),
            created_at,
            last_fetched_at,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct CacheGcCandidateRow {
    pub store_path: String,
    pub url: String,
    pub bytes: i64,
}
pub struct CacheGcCandidateRowBorrowed<'a> {
    pub store_path: &'a str,
    pub url: &'a str,
    pub bytes: i64,
}
impl<'a> From<CacheGcCandidateRowBorrowed<'a>> for CacheGcCandidateRow {
    fn from(
        CacheGcCandidateRowBorrowed {
            store_path,
            url,
            bytes,
        }: CacheGcCandidateRowBorrowed<'a>,
    ) -> Self {
        Self {
            store_path: store_path.into(),
            url: url.into(),
            bytes,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct DeletedNarRow {
    pub store_path: String,
    pub url: String,
    pub bytes: i64,
}
pub struct DeletedNarRowBorrowed<'a> {
    pub store_path: &'a str,
    pub url: &'a str,
    pub bytes: i64,
}
impl<'a> From<DeletedNarRowBorrowed<'a>> for DeletedNarRow {
    fn from(
        DeletedNarRowBorrowed {
            store_path,
            url,
            bytes,
        }: DeletedNarRowBorrowed<'a>,
    ) -> Self {
        Self {
            store_path: store_path.into(),
            url: url.into(),
            bytes,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct NarinfoCacheRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<NarinfoCacheRowBorrowed, tokio_postgres::Error>,
    mapper: fn(NarinfoCacheRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> NarinfoCacheRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(NarinfoCacheRowBorrowed) -> R,
    ) -> NarinfoCacheRowQuery<'c, 'a, 's, C, R, N> {
        NarinfoCacheRowQuery {
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
pub struct StorageSummaryQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<StorageSummary, tokio_postgres::Error>,
    mapper: fn(StorageSummary) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> StorageSummaryQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(StorageSummary) -> R,
    ) -> StorageSummaryQuery<'c, 'a, 's, C, R, N> {
        StorageSummaryQuery {
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
pub struct StorageExtremesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<StorageExtremes, tokio_postgres::Error>,
    mapper: fn(StorageExtremes) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> StorageExtremesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(StorageExtremes) -> R,
    ) -> StorageExtremesQuery<'c, 'a, 's, C, R, N> {
        StorageExtremesQuery {
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
pub struct ListFilteredQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ListFilteredBorrowed, tokio_postgres::Error>,
    mapper: fn(ListFilteredBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ListFilteredQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(ListFilteredBorrowed) -> R,
    ) -> ListFilteredQuery<'c, 'a, 's, C, R, N> {
        ListFilteredQuery {
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
pub struct CacheGcCandidateRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<CacheGcCandidateRowBorrowed, tokio_postgres::Error>,
    mapper: fn(CacheGcCandidateRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> CacheGcCandidateRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(CacheGcCandidateRowBorrowed) -> R,
    ) -> CacheGcCandidateRowQuery<'c, 'a, 's, C, R, N> {
        CacheGcCandidateRowQuery {
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
pub struct DeletedNarRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<DeletedNarRowBorrowed, tokio_postgres::Error>,
    mapper: fn(DeletedNarRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> DeletedNarRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(DeletedNarRowBorrowed) -> R,
    ) -> DeletedNarRowQuery<'c, 'a, 's, C, R, N> {
        DeletedNarRowQuery {
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
        "INSERT INTO narinfo_cache ( store_path, nar_hash, nar_size, file_hash, file_size, compression, url, deriver, \"references\", sig, ca, build_id, project_id, updated_at ) VALUES ( $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, NOW() ) ON CONFLICT (store_path) DO UPDATE SET nar_hash = EXCLUDED.nar_hash, nar_size = EXCLUDED.nar_size, file_hash = EXCLUDED.file_hash, file_size = EXCLUDED.file_size, compression = EXCLUDED.compression, url = EXCLUDED.url, deriver = EXCLUDED.deriver, \"references\" = EXCLUDED.\"references\", sig = EXCLUDED.sig, ca = EXCLUDED.ca, build_id = COALESCE(narinfo_cache.build_id, EXCLUDED.build_id), project_id = COALESCE(narinfo_cache.project_id, EXCLUDED.project_id), updated_at = NOW()",
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
    pub async fn bind<
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
        T8: crate::ArraySql<Item = T7>,
        T9: crate::StringSql,
        T10: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        store_path: &'a T1,
        nar_hash: &'a T2,
        nar_size: &'a i64,
        file_hash: &'a Option<T3>,
        file_size: &'a Option<i64>,
        compression: &'a T4,
        url: &'a T5,
        deriver: &'a Option<T6>,
        references: &'a T8,
        sig: &'a Option<T9>,
        ca: &'a Option<T10>,
        build_id: &'a Option<uuid::Uuid>,
        project_id: &'a Option<uuid::Uuid>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    store_path,
                    nar_hash,
                    nar_size,
                    file_hash,
                    file_size,
                    compression,
                    url,
                    deriver,
                    references,
                    sig,
                    ca,
                    build_id,
                    project_id,
                ],
            )
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
    T8: crate::ArraySql<Item = T7>,
    T9: crate::StringSql,
    T10: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpsertParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpsertStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpsertParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.store_path,
            &params.nar_hash,
            &params.nar_size,
            &params.file_hash,
            &params.file_size,
            &params.compression,
            &params.url,
            &params.deriver,
            &params.references,
            &params.sig,
            &params.ca,
            &params.build_id,
            &params.project_id,
        ))
    }
}
pub struct UpsertProjectOwnerStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert_project_owner() -> UpsertProjectOwnerStmt {
    UpsertProjectOwnerStmt(
        "INSERT INTO narinfo_cache_projects (store_path, project_id, build_id, updated_at) VALUES ($1, $2, $3, NOW()) ON CONFLICT (store_path, project_id) DO UPDATE SET build_id = COALESCE(EXCLUDED.build_id, narinfo_cache_projects.build_id), updated_at = NOW()",
        None,
    )
}
impl UpsertProjectOwnerStmt {
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
        store_path: &'a T1,
        project_id: &'a uuid::Uuid,
        build_id: &'a Option<uuid::Uuid>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[store_path, project_id, build_id])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpsertProjectOwnerParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpsertProjectOwnerStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpsertProjectOwnerParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.store_path,
            &params.project_id,
            &params.build_id,
        ))
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM narinfo_cache WHERE store_path = $1", None)
}
impl GetStmt {
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
        store_path: &'a T1,
    ) -> NarinfoCacheRowQuery<'c, 'a, 's, C, NarinfoCacheRow, 1> {
        NarinfoCacheRowQuery {
            client,
            params: [store_path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NarinfoCacheRowBorrowed, tokio_postgres::Error> {
                Ok(NarinfoCacheRowBorrowed {
                    store_path: row.try_get(0)?,
                    nar_hash: row.try_get(1)?,
                    nar_size: row.try_get(2)?,
                    file_hash: row.try_get(3)?,
                    file_size: row.try_get(4)?,
                    compression: row.try_get(5)?,
                    url: row.try_get(6)?,
                    deriver: row.try_get(7)?,
                    references: row.try_get(8)?,
                    sig: row.try_get(9)?,
                    ca: row.try_get(10)?,
                    created_at: row.try_get(11)?,
                    updated_at: row.try_get(12)?,
                    build_id: row.try_get(13)?,
                    project_id: row.try_get(14)?,
                    last_fetched_at: row.try_get(15)?,
                })
            },
            mapper: |it| NarinfoCacheRow::from(it),
        }
    }
}
pub struct GetByHashPartStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_by_hash_part() -> GetByHashPartStmt {
    GetByHashPartStmt(
        "SELECT * FROM narinfo_cache n WHERE n.store_path LIKE $1 AND ($2::uuid IS NULL OR n.project_id = $2 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $2)) ORDER BY n.updated_at DESC LIMIT 1",
        None,
    )
}
impl GetByHashPartStmt {
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
        hash_part_pattern: &'a T1,
        project_id: &'a Option<uuid::Uuid>,
    ) -> NarinfoCacheRowQuery<'c, 'a, 's, C, NarinfoCacheRow, 2> {
        NarinfoCacheRowQuery {
            client,
            params: [hash_part_pattern, project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NarinfoCacheRowBorrowed, tokio_postgres::Error> {
                Ok(NarinfoCacheRowBorrowed {
                    store_path: row.try_get(0)?,
                    nar_hash: row.try_get(1)?,
                    nar_size: row.try_get(2)?,
                    file_hash: row.try_get(3)?,
                    file_size: row.try_get(4)?,
                    compression: row.try_get(5)?,
                    url: row.try_get(6)?,
                    deriver: row.try_get(7)?,
                    references: row.try_get(8)?,
                    sig: row.try_get(9)?,
                    ca: row.try_get(10)?,
                    created_at: row.try_get(11)?,
                    updated_at: row.try_get(12)?,
                    build_id: row.try_get(13)?,
                    project_id: row.try_get(14)?,
                    last_fetched_at: row.try_get(15)?,
                })
            },
            mapper: |it| NarinfoCacheRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetByHashPartParams<T1>,
        NarinfoCacheRowQuery<'c, 'a, 's, C, NarinfoCacheRow, 2>,
        C,
    > for GetByHashPartStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetByHashPartParams<T1>,
    ) -> NarinfoCacheRowQuery<'c, 'a, 's, C, NarinfoCacheRow, 2> {
        self.bind(client, &params.hash_part_pattern, &params.project_id)
    }
}
pub struct GetByUrlStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_by_url() -> GetByUrlStmt {
    GetByUrlStmt(
        "SELECT * FROM narinfo_cache n WHERE n.url = $1 AND ($2::uuid IS NULL OR n.project_id = $2 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $2)) ORDER BY n.updated_at DESC LIMIT 1",
        None,
    )
}
impl GetByUrlStmt {
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
        url: &'a T1,
        project_id: &'a Option<uuid::Uuid>,
    ) -> NarinfoCacheRowQuery<'c, 'a, 's, C, NarinfoCacheRow, 2> {
        NarinfoCacheRowQuery {
            client,
            params: [url, project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NarinfoCacheRowBorrowed, tokio_postgres::Error> {
                Ok(NarinfoCacheRowBorrowed {
                    store_path: row.try_get(0)?,
                    nar_hash: row.try_get(1)?,
                    nar_size: row.try_get(2)?,
                    file_hash: row.try_get(3)?,
                    file_size: row.try_get(4)?,
                    compression: row.try_get(5)?,
                    url: row.try_get(6)?,
                    deriver: row.try_get(7)?,
                    references: row.try_get(8)?,
                    sig: row.try_get(9)?,
                    ca: row.try_get(10)?,
                    created_at: row.try_get(11)?,
                    updated_at: row.try_get(12)?,
                    build_id: row.try_get(13)?,
                    project_id: row.try_get(14)?,
                    last_fetched_at: row.try_get(15)?,
                })
            },
            mapper: |it| NarinfoCacheRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetByUrlParams<T1>,
        NarinfoCacheRowQuery<'c, 'a, 's, C, NarinfoCacheRow, 2>,
        C,
    > for GetByUrlStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetByUrlParams<T1>,
    ) -> NarinfoCacheRowQuery<'c, 'a, 's, C, NarinfoCacheRow, 2> {
        self.bind(client, &params.url, &params.project_id)
    }
}
pub struct CountStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count() -> CountStmt {
    CountStmt("SELECT COUNT(*) FROM narinfo_cache", None)
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
pub struct StorageSummaryStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn storage_summary() -> StorageSummaryStmt {
    StorageSummaryStmt(
        "WITH uploaded AS ( SELECT store_path, nar_size, file_size FROM narinfo_cache n WHERE ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $1)) ), local AS ( SELECT DISTINCT ON (path) path AS store_path, COALESCE(file_size, 0) AS nar_size, NULL::bigint AS file_size FROM ( SELECT bp.path, bp.file_size, bp.created_at FROM build_products bp JOIN builds b ON b.id = bp.build_id JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND ($1::uuid IS NULL OR j.project_id = $1) UNION ALL SELECT b.build_output_path AS path, NULL::bigint AS file_size, COALESCE(b.completed_at, b.created_at) AS created_at FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND b.build_output_path IS NOT NULL AND ($1::uuid IS NULL OR j.project_id = $1) ) candidates WHERE NOT EXISTS ( SELECT 1 FROM narinfo_cache n WHERE n.store_path = candidates.path AND ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $1)) ) ORDER BY path, created_at DESC ), inventory AS (SELECT * FROM uploaded UNION ALL SELECT * FROM local) SELECT COUNT(*) AS nar_count, COALESCE(SUM(nar_size), 0)::bigint AS uncompressed_bytes, COALESCE(SUM(COALESCE(file_size, nar_size)), 0)::bigint AS compressed_bytes FROM inventory",
        None,
    )
}
impl StorageSummaryStmt {
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
    ) -> StorageSummaryQuery<'c, 'a, 's, C, StorageSummary, 1> {
        StorageSummaryQuery {
            client,
            params: [project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<StorageSummary, tokio_postgres::Error> {
                    Ok(StorageSummary {
                        nar_count: row.try_get(0)?,
                        uncompressed_bytes: row.try_get(1)?,
                        compressed_bytes: row.try_get(2)?,
                    })
                },
            mapper: |it| StorageSummary::from(it),
        }
    }
}
pub struct StorageExtremesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn storage_extremes() -> StorageExtremesStmt {
    StorageExtremesStmt(
        "WITH uploaded AS ( SELECT store_path, created_at, last_fetched_at FROM narinfo_cache n WHERE ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $1)) ), local AS ( SELECT DISTINCT ON (path) path AS store_path, created_at, NULL::timestamptz AS last_fetched_at FROM ( SELECT bp.path, bp.created_at FROM build_products bp JOIN builds b ON b.id = bp.build_id JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND ($1::uuid IS NULL OR j.project_id = $1) UNION ALL SELECT b.build_output_path AS path, COALESCE(b.completed_at, b.created_at) AS created_at FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND b.build_output_path IS NOT NULL AND ($1::uuid IS NULL OR j.project_id = $1) ) candidates WHERE NOT EXISTS ( SELECT 1 FROM narinfo_cache n WHERE n.store_path = candidates.path AND ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $1)) ) ORDER BY path, created_at DESC ), inventory AS (SELECT * FROM uploaded UNION ALL SELECT * FROM local) SELECT MAX(created_at) AS last_uploaded, MIN(last_fetched_at) AS oldest_fetched FROM inventory",
        None,
    )
}
impl StorageExtremesStmt {
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
    ) -> StorageExtremesQuery<'c, 'a, 's, C, StorageExtremes, 1> {
        StorageExtremesQuery {
            client,
            params: [project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<StorageExtremes, tokio_postgres::Error> {
                    Ok(StorageExtremes {
                        last_uploaded: row.try_get(0)?,
                        oldest_fetched: row.try_get(1)?,
                    })
                },
            mapper: |it| StorageExtremes::from(it),
        }
    }
}
pub struct ListFilteredStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_filtered() -> ListFilteredStmt {
    ListFilteredStmt(
        "WITH uploaded AS ( SELECT store_path, nar_size, file_size, compression, created_at, last_fetched_at FROM narinfo_cache n WHERE ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $1)) ), local AS ( SELECT DISTINCT ON (path) path AS store_path, COALESCE(file_size, 0) AS nar_size, NULL::bigint AS file_size, 'none' AS compression, created_at, NULL::timestamptz AS last_fetched_at FROM ( SELECT bp.path, bp.file_size, bp.created_at FROM build_products bp JOIN builds b ON b.id = bp.build_id JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND ($1::uuid IS NULL OR j.project_id = $1) UNION ALL SELECT b.build_output_path AS path, NULL::bigint AS file_size, COALESCE(b.completed_at, b.created_at) AS created_at FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND b.build_output_path IS NOT NULL AND ($1::uuid IS NULL OR j.project_id = $1) ) candidates WHERE NOT EXISTS ( SELECT 1 FROM narinfo_cache n WHERE n.store_path = candidates.path AND ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $1)) ) ORDER BY path, created_at DESC ), inventory AS (SELECT * FROM uploaded UNION ALL SELECT * FROM local) SELECT store_path, COALESCE(substring(store_path FROM '^/nix/store/[^-]+-(.*)$'), store_path) AS package_name, nar_size, file_size, compression, created_at, last_fetched_at FROM inventory WHERE ($2::text IS NULL OR store_path LIKE '/nix/store/' || $2 || '%') AND ($3::text IS NULL OR store_path LIKE '%-%' || $3 || '%') ORDER BY created_at DESC LIMIT $4 OFFSET $5",
        None,
    )
}
impl ListFilteredStmt {
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
        project_id: &'a Option<uuid::Uuid>,
        hash_prefix: &'a Option<T1>,
        package_query: &'a Option<T2>,
        limit: &'a i64,
        offset: &'a i64,
    ) -> ListFilteredQuery<'c, 'a, 's, C, ListFiltered, 5> {
        ListFilteredQuery {
            client,
            params: [project_id, hash_prefix, package_query, limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ListFilteredBorrowed, tokio_postgres::Error> {
                    Ok(ListFilteredBorrowed {
                        store_path: row.try_get(0)?,
                        package_name: row.try_get(1)?,
                        nar_size: row.try_get(2)?,
                        file_size: row.try_get(3)?,
                        compression: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        last_fetched_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ListFiltered::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListFilteredParams<T1, T2>,
        ListFilteredQuery<'c, 'a, 's, C, ListFiltered, 5>,
        C,
    > for ListFilteredStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListFilteredParams<T1, T2>,
    ) -> ListFilteredQuery<'c, 'a, 's, C, ListFiltered, 5> {
        self.bind(
            client,
            &params.project_id,
            &params.hash_prefix,
            &params.package_query,
            &params.limit,
            &params.offset,
        )
    }
}
pub struct CountFilteredStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_filtered() -> CountFilteredStmt {
    CountFilteredStmt(
        "WITH uploaded AS ( SELECT store_path FROM narinfo_cache n WHERE ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $1)) ), local AS ( SELECT DISTINCT ON (path) path AS store_path FROM ( SELECT bp.path, bp.created_at FROM build_products bp JOIN builds b ON b.id = bp.build_id JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND ($1::uuid IS NULL OR j.project_id = $1) UNION ALL SELECT b.build_output_path AS path, COALESCE(b.completed_at, b.created_at) AS created_at FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND b.build_output_path IS NOT NULL AND ($1::uuid IS NULL OR j.project_id = $1) ) candidates WHERE NOT EXISTS ( SELECT 1 FROM narinfo_cache n WHERE n.store_path = candidates.path AND ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $1)) ) ORDER BY path, created_at DESC ), inventory AS (SELECT * FROM uploaded UNION ALL SELECT * FROM local) SELECT COUNT(*) FROM inventory WHERE ($2::text IS NULL OR store_path LIKE '/nix/store/' || $2 || '%') AND ($3::text IS NULL OR store_path LIKE '%-%' || $3 || '%')",
        None,
    )
}
impl CountFilteredStmt {
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
        project_id: &'a Option<uuid::Uuid>,
        hash_prefix: &'a Option<T1>,
        package_query: &'a Option<T2>,
    ) -> I64Query<'c, 'a, 's, C, i64, 3> {
        I64Query {
            client,
            params: [project_id, hash_prefix, package_query],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CountFilteredParams<T1, T2>,
        I64Query<'c, 'a, 's, C, i64, 3>,
        C,
    > for CountFilteredStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CountFilteredParams<T1, T2>,
    ) -> I64Query<'c, 'a, 's, C, i64, 3> {
        self.bind(
            client,
            &params.project_id,
            &params.hash_prefix,
            &params.package_query,
        )
    }
}
pub struct TouchLastFetchedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn touch_last_fetched() -> TouchLastFetchedStmt {
    TouchLastFetchedStmt(
        "UPDATE narinfo_cache SET last_fetched_at = NOW() WHERE store_path = $1",
        None,
    )
}
impl TouchLastFetchedStmt {
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
        store_path: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[store_path]).await
    }
}
pub struct ListGcCandidatesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_gc_candidates() -> ListGcCandidatesStmt {
    ListGcCandidatesStmt(
        "WITH uploaded AS ( SELECT store_path, url, GREATEST(COALESCE(file_size, nar_size), 0)::bigint AS bytes, COALESCE(last_fetched_at, created_at) AS last_used_at FROM narinfo_cache WHERE file_hash IS NOT NULL ), aged AS ( SELECT * FROM uploaded WHERE $1::timestamptz IS NOT NULL AND last_used_at < $1 ), remaining AS ( SELECT uploaded.* FROM uploaded WHERE NOT EXISTS ( SELECT 1 FROM aged WHERE aged.store_path = uploaded.store_path ) ), remaining_total AS ( SELECT COALESCE(SUM(bytes), 0)::bigint AS bytes FROM remaining ), ranked AS ( SELECT remaining.*, remaining_total.bytes AS total_bytes, (SUM(remaining.bytes) OVER ( ORDER BY remaining.last_used_at, remaining.store_path ))::bigint AS reclaimed_bytes FROM remaining CROSS JOIN remaining_total ), quota AS ( SELECT store_path, url, bytes, last_used_at FROM ranked WHERE $2::bigint IS NOT NULL AND total_bytes > $2 AND reclaimed_bytes - bytes < total_bytes - COALESCE($3, $2) ), selected AS ( SELECT store_path, url, bytes, last_used_at FROM aged UNION ALL SELECT store_path, url, bytes, last_used_at FROM quota ) SELECT store_path, url, bytes FROM selected ORDER BY last_used_at, store_path",
        None,
    )
}
impl ListGcCandidatesStmt {
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
        cutoff: &'a Option<chrono::DateTime<chrono::Utc>>,
        max_size_bytes: &'a Option<i64>,
        target_size_bytes: &'a Option<i64>,
    ) -> CacheGcCandidateRowQuery<'c, 'a, 's, C, CacheGcCandidateRow, 3> {
        CacheGcCandidateRowQuery {
            client,
            params: [cutoff, max_size_bytes, target_size_bytes],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<CacheGcCandidateRowBorrowed, tokio_postgres::Error> {
                Ok(CacheGcCandidateRowBorrowed {
                    store_path: row.try_get(0)?,
                    url: row.try_get(1)?,
                    bytes: row.try_get(2)?,
                })
            },
            mapper: |it| CacheGcCandidateRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListGcCandidatesParams,
        CacheGcCandidateRowQuery<'c, 'a, 's, C, CacheGcCandidateRow, 3>,
        C,
    > for ListGcCandidatesStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListGcCandidatesParams,
    ) -> CacheGcCandidateRowQuery<'c, 'a, 's, C, CacheGcCandidateRow, 3> {
        self.bind(
            client,
            &params.cutoff,
            &params.max_size_bytes,
            &params.target_size_bytes,
        )
    }
}
pub struct DeleteGcCandidatesStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_gc_candidates() -> DeleteGcCandidatesStmt {
    DeleteGcCandidatesStmt(
        "DELETE FROM narinfo_cache WHERE file_hash IS NOT NULL AND store_path = ANY($1)",
        None,
    )
}
impl DeleteGcCandidatesStmt {
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
    >(
        &'s self,
        client: &'c C,
        store_paths: &'a T2,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[store_paths]).await
    }
}
pub struct DeleteStaleProjectOwnersStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_stale_project_owners() -> DeleteStaleProjectOwnersStmt {
    DeleteStaleProjectOwnersStmt(
        "DELETE FROM narinfo_cache_projects ncp USING narinfo_cache n WHERE ncp.project_id = $1 AND n.store_path = ncp.store_path AND ($2::timestamptz IS NULL OR COALESCE(n.last_fetched_at, n.created_at) < $2)",
        None,
    )
}
impl DeleteStaleProjectOwnersStmt {
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
        project_id: &'a uuid::Uuid,
        cutoff: &'a Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[project_id, cutoff]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        DeleteStaleProjectOwnersParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for DeleteStaleProjectOwnersStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a DeleteStaleProjectOwnersParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.project_id, &params.cutoff))
    }
}
pub struct DeleteStaleForProjectStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_stale_for_project() -> DeleteStaleForProjectStmt {
    DeleteStaleForProjectStmt(
        "DELETE FROM narinfo_cache n WHERE n.project_id = $1 AND ($2::timestamptz IS NULL OR COALESCE(n.last_fetched_at, n.created_at) < $2) AND NOT EXISTS ( SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path ) RETURNING store_path, url, COALESCE(file_size, nar_size)::bigint AS bytes",
        None,
    )
}
impl DeleteStaleForProjectStmt {
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
        cutoff: &'a Option<chrono::DateTime<chrono::Utc>>,
    ) -> DeletedNarRowQuery<'c, 'a, 's, C, DeletedNarRow, 2> {
        DeletedNarRowQuery {
            client,
            params: [project_id, cutoff],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<DeletedNarRowBorrowed, tokio_postgres::Error> {
                    Ok(DeletedNarRowBorrowed {
                        store_path: row.try_get(0)?,
                        url: row.try_get(1)?,
                        bytes: row.try_get(2)?,
                    })
                },
            mapper: |it| DeletedNarRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        DeleteStaleForProjectParams,
        DeletedNarRowQuery<'c, 'a, 's, C, DeletedNarRow, 2>,
        C,
    > for DeleteStaleForProjectStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a DeleteStaleForProjectParams,
    ) -> DeletedNarRowQuery<'c, 'a, 's, C, DeletedNarRow, 2> {
        self.bind(client, &params.project_id, &params.cutoff)
    }
}
pub struct DeleteStaleGlobalStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_stale_global() -> DeleteStaleGlobalStmt {
    DeleteStaleGlobalStmt(
        "DELETE FROM narinfo_cache WHERE $1::timestamptz IS NULL OR COALESCE(last_fetched_at, created_at) < $1 RETURNING store_path, url, COALESCE(file_size, nar_size)::bigint AS bytes",
        None,
    )
}
impl DeleteStaleGlobalStmt {
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
        cutoff: &'a Option<chrono::DateTime<chrono::Utc>>,
    ) -> DeletedNarRowQuery<'c, 'a, 's, C, DeletedNarRow, 1> {
        DeletedNarRowQuery {
            client,
            params: [cutoff],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<DeletedNarRowBorrowed, tokio_postgres::Error> {
                    Ok(DeletedNarRowBorrowed {
                        store_path: row.try_get(0)?,
                        url: row.try_get(1)?,
                        bytes: row.try_get(2)?,
                    })
                },
            mapper: |it| DeletedNarRow::from(it),
        }
    }
}
