// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql, T2: crate::JsonSql> {
    pub project_id: uuid::Uuid,
    pub notification_type: T1,
    pub config: T2,
}
#[derive(Clone, Copy, Debug)]
pub struct DeleteForProjectParams {
    pub project_id: uuid::Uuid,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpsertParams<T1: crate::StringSql, T2: crate::JsonSql> {
    pub project_id: uuid::Uuid,
    pub notification_type: T1,
    pub config: T2,
    pub enabled: bool,
}
#[derive(Debug)]
pub struct SyncForProjectDeleteParams<T1: crate::StringSql, T2: crate::ArraySql<Item = T1>> {
    pub project_id: uuid::Uuid,
    pub notification_types: T2,
}
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationConfigRow {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub notification_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
pub struct NotificationConfigRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub notification_type: &'a str,
    pub config: postgres_types::Json<&'a serde_json::value::RawValue>,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
impl<'a> From<NotificationConfigRowBorrowed<'a>> for NotificationConfigRow {
    fn from(
        NotificationConfigRowBorrowed {
            id,
            project_id,
            notification_type,
            config,
            enabled,
            created_at,
        }: NotificationConfigRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            project_id,
            notification_type: notification_type.into(),
            config: serde_json::from_str(config.0.get()).unwrap(),
            enabled,
            created_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct NotificationConfigRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<NotificationConfigRowBorrowed, tokio_postgres::Error>,
    mapper: fn(NotificationConfigRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> NotificationConfigRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(NotificationConfigRowBorrowed) -> R,
    ) -> NotificationConfigRowQuery<'c, 'a, 's, C, R, N> {
        NotificationConfigRowQuery {
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
        "INSERT INTO notification_configs (project_id, notification_type, config) VALUES ($1,$2,$3) RETURNING *",
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
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::JsonSql>(
        &'s self,
        client: &'c C,
        project_id: &'a uuid::Uuid,
        notification_type: &'a T1,
        config: &'a T2,
    ) -> NotificationConfigRowQuery<'c, 'a, 's, C, NotificationConfigRow, 3> {
        NotificationConfigRowQuery {
            client,
            params: [project_id, notification_type, config],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NotificationConfigRowBorrowed, tokio_postgres::Error> {
                Ok(NotificationConfigRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    notification_type: row.try_get(2)?,
                    config: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    created_at: row.try_get(5)?,
                })
            },
            mapper: |it| NotificationConfigRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::JsonSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1, T2>,
        NotificationConfigRowQuery<'c, 'a, 's, C, NotificationConfigRow, 3>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1, T2>,
    ) -> NotificationConfigRowQuery<'c, 'a, 's, C, NotificationConfigRow, 3> {
        self.bind(
            client,
            &params.project_id,
            &params.notification_type,
            &params.config,
        )
    }
}
pub struct ListForProjectStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_project() -> ListForProjectStmt {
    ListForProjectStmt(
        "SELECT * FROM notification_configs WHERE project_id =$1 AND enabled = true ORDER BY created_at DESC",
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
    ) -> NotificationConfigRowQuery<'c, 'a, 's, C, NotificationConfigRow, 1> {
        NotificationConfigRowQuery {
            client,
            params: [project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NotificationConfigRowBorrowed, tokio_postgres::Error> {
                Ok(NotificationConfigRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    notification_type: row.try_get(2)?,
                    config: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    created_at: row.try_get(5)?,
                })
            },
            mapper: |it| NotificationConfigRow::from(it),
        }
    }
}
pub struct DeleteForProjectStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_for_project() -> DeleteForProjectStmt {
    DeleteForProjectStmt(
        "DELETE FROM notification_configs WHERE project_id =$1 AND id =$2",
        None,
    )
}
impl DeleteForProjectStmt {
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
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[project_id, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        DeleteForProjectParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for DeleteForProjectStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a DeleteForProjectParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.project_id, &params.id))
    }
}
pub struct UpsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert() -> UpsertStmt {
    UpsertStmt(
        "INSERT INTO notification_configs (project_id, notification_type, config, enabled) VALUES ($1,$2,$3,$4) ON CONFLICT (project_id, notification_type) DO UPDATE SET config = EXCLUDED.config, enabled = EXCLUDED.enabled RETURNING *",
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
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::JsonSql>(
        &'s self,
        client: &'c C,
        project_id: &'a uuid::Uuid,
        notification_type: &'a T1,
        config: &'a T2,
        enabled: &'a bool,
    ) -> NotificationConfigRowQuery<'c, 'a, 's, C, NotificationConfigRow, 4> {
        NotificationConfigRowQuery {
            client,
            params: [project_id, notification_type, config, enabled],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NotificationConfigRowBorrowed, tokio_postgres::Error> {
                Ok(NotificationConfigRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    notification_type: row.try_get(2)?,
                    config: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    created_at: row.try_get(5)?,
                })
            },
            mapper: |it| NotificationConfigRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::JsonSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpsertParams<T1, T2>,
        NotificationConfigRowQuery<'c, 'a, 's, C, NotificationConfigRow, 4>,
        C,
    > for UpsertStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpsertParams<T1, T2>,
    ) -> NotificationConfigRowQuery<'c, 'a, 's, C, NotificationConfigRow, 4> {
        self.bind(
            client,
            &params.project_id,
            &params.notification_type,
            &params.config,
            &params.enabled,
        )
    }
}
pub struct SyncForProjectDeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn sync_for_project_delete() -> SyncForProjectDeleteStmt {
    SyncForProjectDeleteStmt(
        "DELETE FROM notification_configs WHERE project_id =$1 AND notification_type != ALL ($2::text[])",
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
        notification_types: &'a T2,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[project_id, notification_types])
            .await
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
        Box::pin(self.bind(client, &params.project_id, &params.notification_types))
    }
}
