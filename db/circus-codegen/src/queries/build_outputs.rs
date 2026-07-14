// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub build: uuid::Uuid,
    pub name: T1,
    pub path: Option<T2>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BuildOutputRow {
    pub build: uuid::Uuid,
    pub name: String,
    pub path: Option<String>,
}
pub struct BuildOutputRowBorrowed<'a> {
    pub build: uuid::Uuid,
    pub name: &'a str,
    pub path: Option<&'a str>,
}
impl<'a> From<BuildOutputRowBorrowed<'a>> for BuildOutputRow {
    fn from(BuildOutputRowBorrowed { build, name, path }: BuildOutputRowBorrowed<'a>) -> Self {
        Self {
            build,
            name: name.into(),
            path: path.map(|v| v.into()),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct BuildOutputRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<BuildOutputRowBorrowed, tokio_postgres::Error>,
    mapper: fn(BuildOutputRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BuildOutputRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(BuildOutputRowBorrowed) -> R,
    ) -> BuildOutputRowQuery<'c, 'a, 's, C, R, N> {
        BuildOutputRowQuery {
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
        "INSERT INTO build_outputs (build, name, path) VALUES ($1,$2,$3) RETURNING *",
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
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        build: &'a uuid::Uuid,
        name: &'a T1,
        path: &'a Option<T2>,
    ) -> BuildOutputRowQuery<'c, 'a, 's, C, BuildOutputRow, 3> {
        BuildOutputRowQuery {
            client,
            params: [build, name, path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuildOutputRowBorrowed, tokio_postgres::Error> {
                Ok(BuildOutputRowBorrowed {
                    build: row.try_get(0)?,
                    name: row.try_get(1)?,
                    path: row.try_get(2)?,
                })
            },
            mapper: |it| BuildOutputRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1, T2>,
        BuildOutputRowQuery<'c, 'a, 's, C, BuildOutputRow, 3>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1, T2>,
    ) -> BuildOutputRowQuery<'c, 'a, 's, C, BuildOutputRow, 3> {
        self.bind(client, &params.build, &params.name, &params.path)
    }
}
pub struct ListForBuildStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_build() -> ListForBuildStmt {
    ListForBuildStmt(
        "SELECT * FROM build_outputs WHERE build =$1 ORDER BY name ASC",
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
        build: &'a uuid::Uuid,
    ) -> BuildOutputRowQuery<'c, 'a, 's, C, BuildOutputRow, 1> {
        BuildOutputRowQuery {
            client,
            params: [build],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuildOutputRowBorrowed, tokio_postgres::Error> {
                Ok(BuildOutputRowBorrowed {
                    build: row.try_get(0)?,
                    name: row.try_get(1)?,
                    path: row.try_get(2)?,
                })
            },
            mapper: |it| BuildOutputRow::from(it),
        }
    }
}
pub struct FindByPathStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn find_by_path() -> FindByPathStmt {
    FindByPathStmt(
        "SELECT * FROM build_outputs WHERE path =$1 ORDER BY build, name",
        None,
    )
}
impl FindByPathStmt {
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
        path: &'a T1,
    ) -> BuildOutputRowQuery<'c, 'a, 's, C, BuildOutputRow, 1> {
        BuildOutputRowQuery {
            client,
            params: [path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuildOutputRowBorrowed, tokio_postgres::Error> {
                Ok(BuildOutputRowBorrowed {
                    build: row.try_get(0)?,
                    name: row.try_get(1)?,
                    path: row.try_get(2)?,
                })
            },
            mapper: |it| BuildOutputRow::from(it),
        }
    }
}
pub struct DeleteForBuildStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_for_build() -> DeleteForBuildStmt {
    DeleteForBuildStmt("DELETE FROM build_outputs WHERE build =$1", None)
}
impl DeleteForBuildStmt {
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
        build: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[build]).await
    }
}
