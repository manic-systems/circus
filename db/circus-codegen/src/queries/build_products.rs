// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
> {
    pub build_id: uuid::Uuid,
    pub name: T1,
    pub path: T2,
    pub sha256_hash: Option<T3>,
    pub file_size: Option<i64>,
    pub content_type: Option<T4>,
    pub is_directory: bool,
}
#[derive(Debug)]
pub struct SetGcRootPathParams<T1: crate::StringSql> {
    pub gc_root_path: Option<T1>,
    pub id: uuid::Uuid,
}
#[derive(Clone, Copy, Debug)]
pub struct ListPinnedParams {
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BuildProductRow {
    pub id: uuid::Uuid,
    pub build_id: uuid::Uuid,
    pub name: String,
    pub path: String,
    pub sha256_hash: Option<String>,
    pub file_size: Option<i64>,
    pub content_type: Option<String>,
    pub is_directory: bool,
    pub gc_root_path: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
pub struct BuildProductRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub build_id: uuid::Uuid,
    pub name: &'a str,
    pub path: &'a str,
    pub sha256_hash: Option<&'a str>,
    pub file_size: Option<i64>,
    pub content_type: Option<&'a str>,
    pub is_directory: bool,
    pub gc_root_path: Option<&'a str>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
impl<'a> From<BuildProductRowBorrowed<'a>> for BuildProductRow {
    fn from(
        BuildProductRowBorrowed {
            id,
            build_id,
            name,
            path,
            sha256_hash,
            file_size,
            content_type,
            is_directory,
            gc_root_path,
            created_at,
        }: BuildProductRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            build_id,
            name: name.into(),
            path: path.into(),
            sha256_hash: sha256_hash.map(|v| v.into()),
            file_size,
            content_type: content_type.map(|v| v.into()),
            is_directory,
            gc_root_path: gc_root_path.map(|v| v.into()),
            created_at,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ListPinned {
    pub build_id: uuid::Uuid,
    pub job_name: String,
    pub system: Option<String>,
    pub status: Option<String>,
    pub build_created_at: chrono::DateTime<chrono::Utc>,
    pub product_id: uuid::Uuid,
    pub product_name: String,
    pub path: String,
    pub gc_root_path: Option<String>,
    pub product_created_at: chrono::DateTime<chrono::Utc>,
}
pub struct ListPinnedBorrowed<'a> {
    pub build_id: uuid::Uuid,
    pub job_name: &'a str,
    pub system: Option<&'a str>,
    pub status: Option<&'a str>,
    pub build_created_at: chrono::DateTime<chrono::Utc>,
    pub product_id: uuid::Uuid,
    pub product_name: &'a str,
    pub path: &'a str,
    pub gc_root_path: Option<&'a str>,
    pub product_created_at: chrono::DateTime<chrono::Utc>,
}
impl<'a> From<ListPinnedBorrowed<'a>> for ListPinned {
    fn from(
        ListPinnedBorrowed {
            build_id,
            job_name,
            system,
            status,
            build_created_at,
            product_id,
            product_name,
            path,
            gc_root_path,
            product_created_at,
        }: ListPinnedBorrowed<'a>,
    ) -> Self {
        Self {
            build_id,
            job_name: job_name.into(),
            system: system.map(|v| v.into()),
            status: status.map(|v| v.into()),
            build_created_at,
            product_id,
            product_name: product_name.into(),
            path: path.into(),
            gc_root_path: gc_root_path.map(|v| v.into()),
            product_created_at,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ListPinnedForGc {
    pub build_id: uuid::Uuid,
    pub job_name: String,
    pub system: Option<String>,
    pub status: Option<String>,
    pub build_created_at: chrono::DateTime<chrono::Utc>,
    pub product_id: uuid::Uuid,
    pub product_name: String,
    pub path: String,
    pub gc_root_path: Option<String>,
    pub product_created_at: chrono::DateTime<chrono::Utc>,
}
pub struct ListPinnedForGcBorrowed<'a> {
    pub build_id: uuid::Uuid,
    pub job_name: &'a str,
    pub system: Option<&'a str>,
    pub status: Option<&'a str>,
    pub build_created_at: chrono::DateTime<chrono::Utc>,
    pub product_id: uuid::Uuid,
    pub product_name: &'a str,
    pub path: &'a str,
    pub gc_root_path: Option<&'a str>,
    pub product_created_at: chrono::DateTime<chrono::Utc>,
}
impl<'a> From<ListPinnedForGcBorrowed<'a>> for ListPinnedForGc {
    fn from(
        ListPinnedForGcBorrowed {
            build_id,
            job_name,
            system,
            status,
            build_created_at,
            product_id,
            product_name,
            path,
            gc_root_path,
            product_created_at,
        }: ListPinnedForGcBorrowed<'a>,
    ) -> Self {
        Self {
            build_id,
            job_name: job_name.into(),
            system: system.map(|v| v.into()),
            status: status.map(|v| v.into()),
            build_created_at,
            product_id,
            product_name: product_name.into(),
            path: path.into(),
            gc_root_path: gc_root_path.map(|v| v.into()),
            product_created_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct BuildProductRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<BuildProductRowBorrowed, tokio_postgres::Error>,
    mapper: fn(BuildProductRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BuildProductRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(BuildProductRowBorrowed) -> R,
    ) -> BuildProductRowQuery<'c, 'a, 's, C, R, N> {
        BuildProductRowQuery {
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
pub struct ListPinnedQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ListPinnedBorrowed, tokio_postgres::Error>,
    mapper: fn(ListPinnedBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ListPinnedQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(ListPinnedBorrowed) -> R,
    ) -> ListPinnedQuery<'c, 'a, 's, C, R, N> {
        ListPinnedQuery {
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
pub struct ListPinnedForGcQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ListPinnedForGcBorrowed, tokio_postgres::Error>,
    mapper: fn(ListPinnedForGcBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ListPinnedForGcQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(ListPinnedForGcBorrowed) -> R,
    ) -> ListPinnedForGcQuery<'c, 'a, 's, C, R, N> {
        ListPinnedForGcQuery {
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
        "INSERT INTO build_products (build_id, name, path, sha256_hash, file_size, content_type, is_directory) VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING *",
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
    >(
        &'s self,
        client: &'c C,
        build_id: &'a uuid::Uuid,
        name: &'a T1,
        path: &'a T2,
        sha256_hash: &'a Option<T3>,
        file_size: &'a Option<i64>,
        content_type: &'a Option<T4>,
        is_directory: &'a bool,
    ) -> BuildProductRowQuery<'c, 'a, 's, C, BuildProductRow, 7> {
        BuildProductRowQuery {
            client,
            params: [
                build_id,
                name,
                path,
                sha256_hash,
                file_size,
                content_type,
                is_directory,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuildProductRowBorrowed, tokio_postgres::Error> {
                Ok(BuildProductRowBorrowed {
                    id: row.try_get(0)?,
                    build_id: row.try_get(1)?,
                    name: row.try_get(2)?,
                    path: row.try_get(3)?,
                    sha256_hash: row.try_get(4)?,
                    file_size: row.try_get(5)?,
                    content_type: row.try_get(6)?,
                    is_directory: row.try_get(7)?,
                    gc_root_path: row.try_get(8)?,
                    created_at: row.try_get(9)?,
                })
            },
            mapper: |it| BuildProductRow::from(it),
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
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1, T2, T3, T4>,
        BuildProductRowQuery<'c, 'a, 's, C, BuildProductRow, 7>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1, T2, T3, T4>,
    ) -> BuildProductRowQuery<'c, 'a, 's, C, BuildProductRow, 7> {
        self.bind(
            client,
            &params.build_id,
            &params.name,
            &params.path,
            &params.sha256_hash,
            &params.file_size,
            &params.content_type,
            &params.is_directory,
        )
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM build_products WHERE id = $1", None)
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
    ) -> BuildProductRowQuery<'c, 'a, 's, C, BuildProductRow, 1> {
        BuildProductRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuildProductRowBorrowed, tokio_postgres::Error> {
                Ok(BuildProductRowBorrowed {
                    id: row.try_get(0)?,
                    build_id: row.try_get(1)?,
                    name: row.try_get(2)?,
                    path: row.try_get(3)?,
                    sha256_hash: row.try_get(4)?,
                    file_size: row.try_get(5)?,
                    content_type: row.try_get(6)?,
                    is_directory: row.try_get(7)?,
                    gc_root_path: row.try_get(8)?,
                    created_at: row.try_get(9)?,
                })
            },
            mapper: |it| BuildProductRow::from(it),
        }
    }
}
pub struct ListForBuildStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_build() -> ListForBuildStmt {
    ListForBuildStmt(
        "SELECT * FROM build_products WHERE build_id = $1 ORDER BY created_at ASC",
        None,
    )
}
impl ListForBuildStmt {
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
    ) -> BuildProductRowQuery<'c, 'a, 's, C, BuildProductRow, 1> {
        BuildProductRowQuery {
            client,
            params: [build_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuildProductRowBorrowed, tokio_postgres::Error> {
                Ok(BuildProductRowBorrowed {
                    id: row.try_get(0)?,
                    build_id: row.try_get(1)?,
                    name: row.try_get(2)?,
                    path: row.try_get(3)?,
                    sha256_hash: row.try_get(4)?,
                    file_size: row.try_get(5)?,
                    content_type: row.try_get(6)?,
                    is_directory: row.try_get(7)?,
                    gc_root_path: row.try_get(8)?,
                    created_at: row.try_get(9)?,
                })
            },
            mapper: |it| BuildProductRow::from(it),
        }
    }
}
pub struct SetGcRootPathStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_gc_root_path() -> SetGcRootPathStmt {
    SetGcRootPathStmt(
        "UPDATE build_products SET gc_root_path = $1 WHERE id = $2",
        None,
    )
}
impl SetGcRootPathStmt {
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
        gc_root_path: &'a Option<T1>,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[gc_root_path, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetGcRootPathParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetGcRootPathStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SetGcRootPathParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.gc_root_path, &params.id))
    }
}
pub struct ListPinnedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_pinned() -> ListPinnedStmt {
    ListPinnedStmt(
        "SELECT b.id AS build_id, b.job_name, b.system, b.status, b.created_at AS build_created_at, bp.id AS product_id, bp.name AS product_name, bp.path, bp.gc_root_path, bp.created_at AS product_created_at FROM builds b JOIN build_products bp ON bp.build_id = b.id WHERE b.keep = true ORDER BY b.created_at DESC, bp.created_at ASC LIMIT $1 OFFSET $2",
        None,
    )
}
impl ListPinnedStmt {
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
        offset: &'a i64,
    ) -> ListPinnedQuery<'c, 'a, 's, C, ListPinned, 2> {
        ListPinnedQuery {
            client,
            params: [limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ListPinnedBorrowed, tokio_postgres::Error> {
                    Ok(ListPinnedBorrowed {
                        build_id: row.try_get(0)?,
                        job_name: row.try_get(1)?,
                        system: row.try_get(2)?,
                        status: row.try_get(3)?,
                        build_created_at: row.try_get(4)?,
                        product_id: row.try_get(5)?,
                        product_name: row.try_get(6)?,
                        path: row.try_get(7)?,
                        gc_root_path: row.try_get(8)?,
                        product_created_at: row.try_get(9)?,
                    })
                },
            mapper: |it| ListPinned::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListPinnedParams,
        ListPinnedQuery<'c, 'a, 's, C, ListPinned, 2>,
        C,
    > for ListPinnedStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListPinnedParams,
    ) -> ListPinnedQuery<'c, 'a, 's, C, ListPinned, 2> {
        self.bind(client, &params.limit, &params.offset)
    }
}
pub struct CountPinnedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_pinned() -> CountPinnedStmt {
    CountPinnedStmt(
        "SELECT COUNT(*) FROM builds b JOIN build_products bp ON bp.build_id = b.id WHERE b.keep = true",
        None,
    )
}
impl CountPinnedStmt {
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
pub struct ListPinnedForGcStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_pinned_for_gc() -> ListPinnedForGcStmt {
    ListPinnedForGcStmt(
        "SELECT b.id AS build_id, b.job_name, b.system, b.status, b.created_at AS build_created_at, bp.id AS product_id, bp.name AS product_name, bp.path, bp.gc_root_path, bp.created_at AS product_created_at FROM builds b JOIN build_products bp ON bp.build_id = b.id WHERE b.keep = true ORDER BY b.created_at DESC, bp.created_at ASC",
        None,
    )
}
impl ListPinnedForGcStmt {
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
    ) -> ListPinnedForGcQuery<'c, 'a, 's, C, ListPinnedForGc, 0> {
        ListPinnedForGcQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ListPinnedForGcBorrowed, tokio_postgres::Error> {
                Ok(ListPinnedForGcBorrowed {
                    build_id: row.try_get(0)?,
                    job_name: row.try_get(1)?,
                    system: row.try_get(2)?,
                    status: row.try_get(3)?,
                    build_created_at: row.try_get(4)?,
                    product_id: row.try_get(5)?,
                    product_name: row.try_get(6)?,
                    path: row.try_get(7)?,
                    gc_root_path: row.try_get(8)?,
                    product_created_at: row.try_get(9)?,
                })
            },
            mapper: |it| ListPinnedForGc::from(it),
        }
    }
}
