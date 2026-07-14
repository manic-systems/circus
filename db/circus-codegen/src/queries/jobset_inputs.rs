// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
> {
    pub jobset_id: uuid::Uuid,
    pub name: T1,
    pub input_type: T2,
    pub value: T3,
    pub revision: Option<T4>,
}
#[derive(Debug)]
pub struct UpsertParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
> {
    pub jobset_id: uuid::Uuid,
    pub name: T1,
    pub input_type: T2,
    pub value: T3,
    pub revision: Option<T4>,
}
#[derive(Debug)]
pub struct SyncForJobsetDeleteParams<T1: crate::StringSql, T2: crate::ArraySql<Item = T1>> {
    pub jobset_id: uuid::Uuid,
    pub names: T2,
}
#[derive(Debug, Clone, PartialEq)]
pub struct JobsetInputRow {
    pub id: uuid::Uuid,
    pub jobset_id: uuid::Uuid,
    pub name: String,
    pub input_type: String,
    pub value: String,
    pub revision: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
pub struct JobsetInputRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub jobset_id: uuid::Uuid,
    pub name: &'a str,
    pub input_type: &'a str,
    pub value: &'a str,
    pub revision: Option<&'a str>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
impl<'a> From<JobsetInputRowBorrowed<'a>> for JobsetInputRow {
    fn from(
        JobsetInputRowBorrowed {
            id,
            jobset_id,
            name,
            input_type,
            value,
            revision,
            created_at,
        }: JobsetInputRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            jobset_id,
            name: name.into(),
            input_type: input_type.into(),
            value: value.into(),
            revision: revision.map(|v| v.into()),
            created_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct JobsetInputRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<JobsetInputRowBorrowed, tokio_postgres::Error>,
    mapper: fn(JobsetInputRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> JobsetInputRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(JobsetInputRowBorrowed) -> R,
    ) -> JobsetInputRowQuery<'c, 'a, 's, C, R, N> {
        JobsetInputRowQuery {
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
        "INSERT INTO jobset_inputs (jobset_id, name, input_type, value, revision) VALUES ($1,$2,$3,$4,$5) RETURNING *",
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
        jobset_id: &'a uuid::Uuid,
        name: &'a T1,
        input_type: &'a T2,
        value: &'a T3,
        revision: &'a Option<T4>,
    ) -> JobsetInputRowQuery<'c, 'a, 's, C, JobsetInputRow, 5> {
        JobsetInputRowQuery {
            client,
            params: [jobset_id, name, input_type, value, revision],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<JobsetInputRowBorrowed, tokio_postgres::Error> {
                Ok(JobsetInputRowBorrowed {
                    id: row.try_get(0)?,
                    jobset_id: row.try_get(1)?,
                    name: row.try_get(2)?,
                    input_type: row.try_get(3)?,
                    value: row.try_get(4)?,
                    revision: row.try_get(5)?,
                    created_at: row.try_get(6)?,
                })
            },
            mapper: |it| JobsetInputRow::from(it),
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
        JobsetInputRowQuery<'c, 'a, 's, C, JobsetInputRow, 5>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1, T2, T3, T4>,
    ) -> JobsetInputRowQuery<'c, 'a, 's, C, JobsetInputRow, 5> {
        self.bind(
            client,
            &params.jobset_id,
            &params.name,
            &params.input_type,
            &params.value,
            &params.revision,
        )
    }
}
pub struct ListForJobsetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_jobset() -> ListForJobsetStmt {
    ListForJobsetStmt(
        "SELECT * FROM jobset_inputs WHERE jobset_id =$1 ORDER BY name ASC",
        None,
    )
}
impl ListForJobsetStmt {
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
        jobset_id: &'a uuid::Uuid,
    ) -> JobsetInputRowQuery<'c, 'a, 's, C, JobsetInputRow, 1> {
        JobsetInputRowQuery {
            client,
            params: [jobset_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<JobsetInputRowBorrowed, tokio_postgres::Error> {
                Ok(JobsetInputRowBorrowed {
                    id: row.try_get(0)?,
                    jobset_id: row.try_get(1)?,
                    name: row.try_get(2)?,
                    input_type: row.try_get(3)?,
                    value: row.try_get(4)?,
                    revision: row.try_get(5)?,
                    created_at: row.try_get(6)?,
                })
            },
            mapper: |it| JobsetInputRow::from(it),
        }
    }
}
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("DELETE FROM jobset_inputs WHERE id =$1", None)
}
impl DeleteStmt {
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
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[id]).await
    }
}
pub struct UpsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert() -> UpsertStmt {
    UpsertStmt(
        "INSERT INTO jobset_inputs (jobset_id, name, input_type, value, revision) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (jobset_id, name) DO UPDATE SET input_type = EXCLUDED.input_type, value = EXCLUDED.value, revision = EXCLUDED.revision RETURNING *",
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
        jobset_id: &'a uuid::Uuid,
        name: &'a T1,
        input_type: &'a T2,
        value: &'a T3,
        revision: &'a Option<T4>,
    ) -> JobsetInputRowQuery<'c, 'a, 's, C, JobsetInputRow, 5> {
        JobsetInputRowQuery {
            client,
            params: [jobset_id, name, input_type, value, revision],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<JobsetInputRowBorrowed, tokio_postgres::Error> {
                Ok(JobsetInputRowBorrowed {
                    id: row.try_get(0)?,
                    jobset_id: row.try_get(1)?,
                    name: row.try_get(2)?,
                    input_type: row.try_get(3)?,
                    value: row.try_get(4)?,
                    revision: row.try_get(5)?,
                    created_at: row.try_get(6)?,
                })
            },
            mapper: |it| JobsetInputRow::from(it),
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
        UpsertParams<T1, T2, T3, T4>,
        JobsetInputRowQuery<'c, 'a, 's, C, JobsetInputRow, 5>,
        C,
    > for UpsertStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpsertParams<T1, T2, T3, T4>,
    ) -> JobsetInputRowQuery<'c, 'a, 's, C, JobsetInputRow, 5> {
        self.bind(
            client,
            &params.jobset_id,
            &params.name,
            &params.input_type,
            &params.value,
            &params.revision,
        )
    }
}
pub struct SyncForJobsetDeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn sync_for_jobset_delete() -> SyncForJobsetDeleteStmt {
    SyncForJobsetDeleteStmt(
        "DELETE FROM jobset_inputs WHERE jobset_id =$1 AND name != ALL ($2::text[])",
        None,
    )
}
impl SyncForJobsetDeleteStmt {
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
        jobset_id: &'a uuid::Uuid,
        names: &'a T2,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[jobset_id, names]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql, T2: crate::ArraySql<Item = T1>>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SyncForJobsetDeleteParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SyncForJobsetDeleteStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SyncForJobsetDeleteParams<T1, T2>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.jobset_id, &params.names))
    }
}
