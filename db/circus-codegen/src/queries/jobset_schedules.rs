// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct UpsertParams<T1: crate::StringSql, T2: crate::JsonSql, T3: crate::StringSql> {
    pub jobset_id: uuid::Uuid,
    pub name: T1,
    pub when_spec: T2,
    pub commit_hash: T3,
    pub next_due_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug)]
pub struct DeleteExceptParams<T1: crate::StringSql, T2: crate::ArraySql<Item = T1>> {
    pub jobset_id: uuid::Uuid,
    pub names: T2,
}
#[derive(Debug)]
pub struct MarkFiredParams<T1: crate::StringSql> {
    pub next_due_at: chrono::DateTime<chrono::Utc>,
    pub jobset_id: uuid::Uuid,
    pub name: T1,
    pub previous_due_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct JobsetScheduleRow {
    pub jobset_id: uuid::Uuid,
    pub name: String,
    pub when_spec: serde_json::Value,
    pub commit_hash: String,
    pub next_due_at: chrono::DateTime<chrono::Utc>,
    pub last_fired_at: Option<chrono::DateTime<chrono::Utc>>,
}
pub struct JobsetScheduleRowBorrowed<'a> {
    pub jobset_id: uuid::Uuid,
    pub name: &'a str,
    pub when_spec: postgres_types::Json<&'a serde_json::value::RawValue>,
    pub commit_hash: &'a str,
    pub next_due_at: chrono::DateTime<chrono::Utc>,
    pub last_fired_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl<'a> From<JobsetScheduleRowBorrowed<'a>> for JobsetScheduleRow {
    fn from(
        JobsetScheduleRowBorrowed {
            jobset_id,
            name,
            when_spec,
            commit_hash,
            next_due_at,
            last_fired_at,
        }: JobsetScheduleRowBorrowed<'a>,
    ) -> Self {
        Self {
            jobset_id,
            name: name.into(),
            when_spec: serde_json::from_str(when_spec.0.get()).unwrap(),
            commit_hash: commit_hash.into(),
            next_due_at,
            last_fired_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct JobsetScheduleRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<JobsetScheduleRowBorrowed, tokio_postgres::Error>,
    mapper: fn(JobsetScheduleRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> JobsetScheduleRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(JobsetScheduleRowBorrowed) -> R,
    ) -> JobsetScheduleRowQuery<'c, 'a, 's, C, R, N> {
        JobsetScheduleRowQuery {
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
pub struct StringQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<&str, tokio_postgres::Error>,
    mapper: fn(&str) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> StringQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(&str) -> R) -> StringQuery<'c, 'a, 's, C, R, N> {
        StringQuery {
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
pub struct ListForJobsetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_jobset() -> ListForJobsetStmt {
    ListForJobsetStmt(
        "SELECT jobset_id, name, when_spec, commit_hash, next_due_at, last_fired_at FROM jobset_schedules WHERE jobset_id = $1",
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
    ) -> JobsetScheduleRowQuery<'c, 'a, 's, C, JobsetScheduleRow, 1> {
        JobsetScheduleRowQuery {
            client,
            params: [jobset_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<JobsetScheduleRowBorrowed, tokio_postgres::Error> {
                Ok(JobsetScheduleRowBorrowed {
                    jobset_id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    when_spec: row.try_get(2)?,
                    commit_hash: row.try_get(3)?,
                    next_due_at: row.try_get(4)?,
                    last_fired_at: row.try_get(5)?,
                })
            },
            mapper: |it| JobsetScheduleRow::from(it),
        }
    }
}
pub struct UpsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert() -> UpsertStmt {
    UpsertStmt(
        "INSERT INTO jobset_schedules (jobset_id, name, when_spec, commit_hash, next_due_at) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (jobset_id, name) DO UPDATE SET when_spec = EXCLUDED.when_spec, commit_hash = EXCLUDED.commit_hash, next_due_at = EXCLUDED.next_due_at",
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
        T2: crate::JsonSql,
        T3: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        jobset_id: &'a uuid::Uuid,
        name: &'a T1,
        when_spec: &'a T2,
        commit_hash: &'a T3,
        next_due_at: &'a chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[jobset_id, name, when_spec, commit_hash, next_due_at],
            )
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::JsonSql,
    T3: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpsertParams<T1, T2, T3>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpsertStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpsertParams<T1, T2, T3>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.jobset_id,
            &params.name,
            &params.when_spec,
            &params.commit_hash,
            &params.next_due_at,
        ))
    }
}
pub struct DeleteExceptStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_except() -> DeleteExceptStmt {
    DeleteExceptStmt(
        "DELETE FROM jobset_schedules WHERE jobset_id = $1 AND NOT (name = ANY($2))",
        None,
    )
}
impl DeleteExceptStmt {
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
        DeleteExceptParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for DeleteExceptStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a DeleteExceptParams<T1, T2>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.jobset_id, &params.names))
    }
}
pub struct ListDueStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_due() -> ListDueStmt {
    ListDueStmt(
        "SELECT jobset_id, name, when_spec, commit_hash, next_due_at, last_fired_at FROM jobset_schedules WHERE next_due_at <= NOW() ORDER BY next_due_at",
        None,
    )
}
impl ListDueStmt {
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
    ) -> JobsetScheduleRowQuery<'c, 'a, 's, C, JobsetScheduleRow, 0> {
        JobsetScheduleRowQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<JobsetScheduleRowBorrowed, tokio_postgres::Error> {
                Ok(JobsetScheduleRowBorrowed {
                    jobset_id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    when_spec: row.try_get(2)?,
                    commit_hash: row.try_get(3)?,
                    next_due_at: row.try_get(4)?,
                    last_fired_at: row.try_get(5)?,
                })
            },
            mapper: |it| JobsetScheduleRow::from(it),
        }
    }
}
pub struct MarkFiredStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn mark_fired() -> MarkFiredStmt {
    MarkFiredStmt(
        "UPDATE jobset_schedules SET last_fired_at = NOW(), next_due_at = $1 WHERE jobset_id = $2 AND name = $3 AND next_due_at = $4 RETURNING name",
        None,
    )
}
impl MarkFiredStmt {
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
        next_due_at: &'a chrono::DateTime<chrono::Utc>,
        jobset_id: &'a uuid::Uuid,
        name: &'a T1,
        previous_due_at: &'a chrono::DateTime<chrono::Utc>,
    ) -> StringQuery<'c, 'a, 's, C, String, 4> {
        StringQuery {
            client,
            params: [next_due_at, jobset_id, name, previous_due_at],
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
        MarkFiredParams<T1>,
        StringQuery<'c, 'a, 's, C, String, 4>,
        C,
    > for MarkFiredStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a MarkFiredParams<T1>,
    ) -> StringQuery<'c, 'a, 's, C, String, 4> {
        self.bind(
            client,
            &params.next_due_at,
            &params.jobset_id,
            &params.name,
            &params.previous_due_at,
        )
    }
}
