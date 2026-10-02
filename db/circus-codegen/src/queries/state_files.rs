// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct GetParams<T1: crate::StringSql> {
    pub project_id: uuid::Uuid,
    pub name: T1,
}
#[derive(Debug)]
pub struct PutParams<T1: crate::StringSql, T2: crate::BytesSql> {
    pub project_id: uuid::Uuid,
    pub name: T1,
    pub data: T2,
    pub build_id: uuid::Uuid,
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct Vecu8Query<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<&[u8], tokio_postgres::Error>,
    mapper: fn(&[u8]) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> Vecu8Query<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(&[u8]) -> R) -> Vecu8Query<'c, 'a, 's, C, R, N> {
        Vecu8Query {
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
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt(
        "SELECT data FROM project_state_files WHERE project_id = $1 AND name = $2",
        None,
    )
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
        project_id: &'a uuid::Uuid,
        name: &'a T1,
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 2> {
        Vecu8Query {
            client,
            params: [project_id, name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetParams<T1>,
        Vecu8Query<'c, 'a, 's, C, Vec<u8>, 2>,
        C,
    > for GetStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetParams<T1>,
    ) -> Vecu8Query<'c, 'a, 's, C, Vec<u8>, 2> {
        self.bind(client, &params.project_id, &params.name)
    }
}
pub struct PutStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn put() -> PutStmt {
    PutStmt(
        "INSERT INTO project_state_files (project_id, name, data, updated_by_build) VALUES ($1, $2, $3, $4) ON CONFLICT (project_id, name) DO UPDATE SET data = EXCLUDED.data, updated_at = NOW(), updated_by_build = EXCLUDED.updated_by_build",
        None,
    )
}
impl PutStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::BytesSql>(
        &'s self,
        client: &'c C,
        project_id: &'a uuid::Uuid,
        name: &'a T1,
        data: &'a T2,
        build_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[project_id, name, data, build_id])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql, T2: crate::BytesSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        PutParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for PutStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a PutParams<T1, T2>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.project_id,
            &params.name,
            &params.data,
            &params.build_id,
        ))
    }
}
