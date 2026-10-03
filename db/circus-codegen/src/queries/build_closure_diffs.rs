// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct UpsertParams<T1: crate::JsonSql> {
    pub build_id: uuid::Uuid,
    pub against_build_id: uuid::Uuid,
    pub changes: T1,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Get {
    pub against_build_id: uuid::Uuid,
    pub against_commit: String,
    pub changes: serde_json::Value,
}
pub struct GetBorrowed<'a> {
    pub against_build_id: uuid::Uuid,
    pub against_commit: &'a str,
    pub changes: postgres_types::Json<&'a serde_json::value::RawValue>,
}
impl<'a> From<GetBorrowed<'a>> for Get {
    fn from(
        GetBorrowed {
            against_build_id,
            against_commit,
            changes,
        }: GetBorrowed<'a>,
    ) -> Self {
        Self {
            against_build_id,
            against_commit: against_commit.into(),
            changes: serde_json::from_str(changes.0.get()).unwrap(),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct GetQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<GetBorrowed, tokio_postgres::Error>,
    mapper: fn(GetBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> GetQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(GetBorrowed) -> R) -> GetQuery<'c, 'a, 's, C, R, N> {
        GetQuery {
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
        "INSERT INTO build_closure_diffs (build_id, against_build_id, changes) VALUES ($1, $2, $3) ON CONFLICT (build_id) DO UPDATE SET against_build_id = EXCLUDED.against_build_id, changes = EXCLUDED.changes",
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
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::JsonSql>(
        &'s self,
        client: &'c C,
        build_id: &'a uuid::Uuid,
        against_build_id: &'a uuid::Uuid,
        changes: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[build_id, against_build_id, changes])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::JsonSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpsertParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpsertStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpsertParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.build_id,
            &params.against_build_id,
            &params.changes,
        ))
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt(
        "SELECT d.against_build_id, e.commit_hash AS against_commit, d.changes FROM build_closure_diffs d JOIN builds b ON b.id = d.against_build_id JOIN evaluations e ON e.id = b.evaluation_id WHERE d.build_id = $1",
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
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c C,
        build_id: &'a uuid::Uuid,
    ) -> GetQuery<'c, 'a, 's, C, Get, 1> {
        GetQuery {
            client,
            params: [build_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &tokio_postgres::Row| -> Result<GetBorrowed, tokio_postgres::Error> {
                Ok(GetBorrowed {
                    against_build_id: row.try_get(0)?,
                    against_commit: row.try_get(1)?,
                    changes: row.try_get(2)?,
                })
            },
            mapper: |it| Get::from(it),
        }
    }
}
