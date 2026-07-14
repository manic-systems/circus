// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql> {
    pub project_id: uuid::Uuid,
    pub name: T1,
    pub jobset_id: uuid::Uuid,
}
#[derive(Clone, Copy, Debug)]
pub struct PromoteParams {
    pub evaluation_id: uuid::Uuid,
    pub channel_id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpsertParams<T1: crate::StringSql> {
    pub project_id: uuid::Uuid,
    pub name: T1,
    pub jobset_id: uuid::Uuid,
}
#[derive(Debug)]
pub struct SyncForProjectDeleteParams<T1: crate::StringSql, T2: crate::ArraySql<Item = T1>> {
    pub project_id: uuid::Uuid,
    pub names: T2,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelRow {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub name: String,
    pub jobset_id: uuid::Uuid,
    pub current_evaluation_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
pub struct ChannelRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub name: &'a str,
    pub jobset_id: uuid::Uuid,
    pub current_evaluation_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
impl<'a> From<ChannelRowBorrowed<'a>> for ChannelRow {
    fn from(
        ChannelRowBorrowed {
            id,
            project_id,
            name,
            jobset_id,
            current_evaluation_id,
            created_at,
            updated_at,
        }: ChannelRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            project_id,
            name: name.into(),
            jobset_id,
            current_evaluation_id,
            created_at,
            updated_at,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct AutoPromoteCount {
    pub total: Option<i64>,
    pub completed: Option<i64>,
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct ChannelRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ChannelRowBorrowed, tokio_postgres::Error>,
    mapper: fn(ChannelRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ChannelRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(ChannelRowBorrowed) -> R,
    ) -> ChannelRowQuery<'c, 'a, 's, C, R, N> {
        ChannelRowQuery {
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
pub struct AutoPromoteCountQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<AutoPromoteCount, tokio_postgres::Error>,
    mapper: fn(AutoPromoteCount) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> AutoPromoteCountQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(AutoPromoteCount) -> R,
    ) -> AutoPromoteCountQuery<'c, 'a, 's, C, R, N> {
        AutoPromoteCountQuery {
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
        "INSERT INTO channels (project_id, name, jobset_id) VALUES ($1,$2,$3) RETURNING *",
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
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        project_id: &'a uuid::Uuid,
        name: &'a T1,
        jobset_id: &'a uuid::Uuid,
    ) -> ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 3> {
        ChannelRowQuery {
            client,
            params: [project_id, name, jobset_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ChannelRowBorrowed, tokio_postgres::Error> {
                    Ok(ChannelRowBorrowed {
                        id: row.try_get(0)?,
                        project_id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        jobset_id: row.try_get(3)?,
                        current_evaluation_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        updated_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ChannelRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1>,
        ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 3>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1>,
    ) -> ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 3> {
        self.bind(client, &params.project_id, &params.name, &params.jobset_id)
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM channels WHERE id =$1", None)
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
    ) -> ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 1> {
        ChannelRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ChannelRowBorrowed, tokio_postgres::Error> {
                    Ok(ChannelRowBorrowed {
                        id: row.try_get(0)?,
                        project_id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        jobset_id: row.try_get(3)?,
                        current_evaluation_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        updated_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ChannelRow::from(it),
        }
    }
}
pub struct ListForProjectStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_project() -> ListForProjectStmt {
    ListForProjectStmt(
        "SELECT * FROM channels WHERE project_id =$1 ORDER BY name",
        None,
    )
}
impl ListForProjectStmt {
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
    ) -> ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 1> {
        ChannelRowQuery {
            client,
            params: [project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ChannelRowBorrowed, tokio_postgres::Error> {
                    Ok(ChannelRowBorrowed {
                        id: row.try_get(0)?,
                        project_id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        jobset_id: row.try_get(3)?,
                        current_evaluation_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        updated_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ChannelRow::from(it),
        }
    }
}
pub struct ListAllStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_all() -> ListAllStmt {
    ListAllStmt("SELECT * FROM channels ORDER BY name", None)
}
impl ListAllStmt {
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
    ) -> ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 0> {
        ChannelRowQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ChannelRowBorrowed, tokio_postgres::Error> {
                    Ok(ChannelRowBorrowed {
                        id: row.try_get(0)?,
                        project_id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        jobset_id: row.try_get(3)?,
                        current_evaluation_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        updated_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ChannelRow::from(it),
        }
    }
}
pub struct CountStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count() -> CountStmt {
    CountStmt("SELECT COUNT(*) FROM channels", None)
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
pub struct GetByNameStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_by_name() -> GetByNameStmt {
    GetByNameStmt(
        "SELECT * FROM channels WHERE name =$1 ORDER BY created_at DESC, id DESC LIMIT 1",
        None,
    )
}
impl GetByNameStmt {
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
        name: &'a T1,
    ) -> ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 1> {
        ChannelRowQuery {
            client,
            params: [name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ChannelRowBorrowed, tokio_postgres::Error> {
                    Ok(ChannelRowBorrowed {
                        id: row.try_get(0)?,
                        project_id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        jobset_id: row.try_get(3)?,
                        current_evaluation_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        updated_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ChannelRow::from(it),
        }
    }
}
pub struct PromoteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn promote() -> PromoteStmt {
    PromoteStmt(
        "UPDATE channels SET current_evaluation_id =$1, updated_at = NOW() WHERE id =$2 RETURNING *",
        None,
    )
}
impl PromoteStmt {
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
        evaluation_id: &'a uuid::Uuid,
        channel_id: &'a uuid::Uuid,
    ) -> ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 2> {
        ChannelRowQuery {
            client,
            params: [evaluation_id, channel_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ChannelRowBorrowed, tokio_postgres::Error> {
                    Ok(ChannelRowBorrowed {
                        id: row.try_get(0)?,
                        project_id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        jobset_id: row.try_get(3)?,
                        current_evaluation_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        updated_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ChannelRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        PromoteParams,
        ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 2>,
        C,
    > for PromoteStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a PromoteParams,
    ) -> ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 2> {
        self.bind(client, &params.evaluation_id, &params.channel_id)
    }
}
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("DELETE FROM channels WHERE id =$1", None)
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
        "INSERT INTO channels (project_id, name, jobset_id) VALUES ($1,$2,$3) ON CONFLICT (project_id, name) DO UPDATE SET jobset_id = EXCLUDED.jobset_id RETURNING *",
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
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c C,
        project_id: &'a uuid::Uuid,
        name: &'a T1,
        jobset_id: &'a uuid::Uuid,
    ) -> ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 3> {
        ChannelRowQuery {
            client,
            params: [project_id, name, jobset_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ChannelRowBorrowed, tokio_postgres::Error> {
                    Ok(ChannelRowBorrowed {
                        id: row.try_get(0)?,
                        project_id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        jobset_id: row.try_get(3)?,
                        current_evaluation_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        updated_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ChannelRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpsertParams<T1>,
        ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 3>,
        C,
    > for UpsertStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpsertParams<T1>,
    ) -> ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 3> {
        self.bind(client, &params.project_id, &params.name, &params.jobset_id)
    }
}
pub struct SyncForProjectDeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn sync_for_project_delete() -> SyncForProjectDeleteStmt {
    SyncForProjectDeleteStmt(
        "DELETE FROM channels WHERE project_id =$1 AND name != ALL ($2::text[])",
        None,
    )
}
impl SyncForProjectDeleteStmt {
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
        project_id: &'a uuid::Uuid,
        names: &'a T2,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[project_id, names]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql, T2: crate::ArraySql<Item = T1>>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SyncForProjectDeleteParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SyncForProjectDeleteStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SyncForProjectDeleteParams<T1, T2>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.project_id, &params.names))
    }
}
pub struct AutoPromoteCountStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn auto_promote_count() -> AutoPromoteCountStmt {
    AutoPromoteCountStmt(
        "SELECT COUNT(*) AS total, COUNT(*) FILTER ( WHERE status = 'succeeded' ) AS completed FROM builds WHERE evaluation_id =$1",
        None,
    )
}
impl AutoPromoteCountStmt {
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
        evaluation_id: &'a uuid::Uuid,
    ) -> AutoPromoteCountQuery<'c, 'a, 's, C, AutoPromoteCount, 1> {
        AutoPromoteCountQuery {
            client,
            params: [evaluation_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<AutoPromoteCount, tokio_postgres::Error> {
                    Ok(AutoPromoteCount {
                        total: row.try_get(0)?,
                        completed: row.try_get(1)?,
                    })
                },
            mapper: |it| AutoPromoteCount::from(it),
        }
    }
}
pub struct AutoPromoteChannelsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn auto_promote_channels() -> AutoPromoteChannelsStmt {
    AutoPromoteChannelsStmt("SELECT * FROM channels WHERE jobset_id =$1", None)
}
impl AutoPromoteChannelsStmt {
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
    ) -> ChannelRowQuery<'c, 'a, 's, C, ChannelRow, 1> {
        ChannelRowQuery {
            client,
            params: [jobset_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ChannelRowBorrowed, tokio_postgres::Error> {
                    Ok(ChannelRowBorrowed {
                        id: row.try_get(0)?,
                        project_id: row.try_get(1)?,
                        name: row.try_get(2)?,
                        jobset_id: row.try_get(3)?,
                        current_evaluation_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        updated_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ChannelRow::from(it),
        }
    }
}
