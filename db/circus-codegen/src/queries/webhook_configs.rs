// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub project_id: uuid::Uuid,
    pub forge_type: T1,
    pub secret_hash: Option<T2>,
}
#[derive(Debug)]
pub struct GetByProjectAndForgeParams<T1: crate::StringSql> {
    pub project_id: uuid::Uuid,
    pub forge_type: T1,
}
#[derive(Debug)]
pub struct UpsertParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub project_id: uuid::Uuid,
    pub forge_type: T1,
    pub secret_hash: Option<T2>,
    pub enabled: bool,
}
#[derive(Debug)]
pub struct SyncForProjectDeleteParams<T1: crate::StringSql, T2: crate::ArraySql<Item = T1>> {
    pub project_id: uuid::Uuid,
    pub forge_types: T2,
}
#[derive(Debug, Clone, PartialEq)]
pub struct WebhookConfigRow {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub forge_type: String,
    pub secret_hash: Option<String>,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
pub struct WebhookConfigRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub forge_type: &'a str,
    pub secret_hash: Option<&'a str>,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
impl<'a> From<WebhookConfigRowBorrowed<'a>> for WebhookConfigRow {
    fn from(
        WebhookConfigRowBorrowed {
            id,
            project_id,
            forge_type,
            secret_hash,
            enabled,
            created_at,
        }: WebhookConfigRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            project_id,
            forge_type: forge_type.into(),
            secret_hash: secret_hash.map(|v| v.into()),
            enabled,
            created_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct WebhookConfigRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<WebhookConfigRowBorrowed, tokio_postgres::Error>,
    mapper: fn(WebhookConfigRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> WebhookConfigRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(WebhookConfigRowBorrowed) -> R,
    ) -> WebhookConfigRowQuery<'c, 'a, 's, C, R, N> {
        WebhookConfigRowQuery {
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
        "INSERT INTO webhook_configs (project_id, forge_type, secret_hash) VALUES ($1,$2,$3) RETURNING *",
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
        project_id: &'a uuid::Uuid,
        forge_type: &'a T1,
        secret_hash: &'a Option<T2>,
    ) -> WebhookConfigRowQuery<'c, 'a, 's, C, WebhookConfigRow, 3> {
        WebhookConfigRowQuery {
            client,
            params: [project_id, forge_type, secret_hash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<WebhookConfigRowBorrowed, tokio_postgres::Error> {
                Ok(WebhookConfigRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    forge_type: row.try_get(2)?,
                    secret_hash: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    created_at: row.try_get(5)?,
                })
            },
            mapper: |it| WebhookConfigRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1, T2>,
        WebhookConfigRowQuery<'c, 'a, 's, C, WebhookConfigRow, 3>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1, T2>,
    ) -> WebhookConfigRowQuery<'c, 'a, 's, C, WebhookConfigRow, 3> {
        self.bind(
            client,
            &params.project_id,
            &params.forge_type,
            &params.secret_hash,
        )
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM webhook_configs WHERE id =$1", None)
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
    ) -> WebhookConfigRowQuery<'c, 'a, 's, C, WebhookConfigRow, 1> {
        WebhookConfigRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<WebhookConfigRowBorrowed, tokio_postgres::Error> {
                Ok(WebhookConfigRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    forge_type: row.try_get(2)?,
                    secret_hash: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    created_at: row.try_get(5)?,
                })
            },
            mapper: |it| WebhookConfigRow::from(it),
        }
    }
}
pub struct ListForProjectStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_project() -> ListForProjectStmt {
    ListForProjectStmt(
        "SELECT * FROM webhook_configs WHERE project_id =$1 ORDER BY created_at DESC",
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
    ) -> WebhookConfigRowQuery<'c, 'a, 's, C, WebhookConfigRow, 1> {
        WebhookConfigRowQuery {
            client,
            params: [project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<WebhookConfigRowBorrowed, tokio_postgres::Error> {
                Ok(WebhookConfigRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    forge_type: row.try_get(2)?,
                    secret_hash: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    created_at: row.try_get(5)?,
                })
            },
            mapper: |it| WebhookConfigRow::from(it),
        }
    }
}
pub struct GetByProjectAndForgeStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_by_project_and_forge() -> GetByProjectAndForgeStmt {
    GetByProjectAndForgeStmt(
        "SELECT * FROM webhook_configs WHERE project_id =$1 AND forge_type =$2 AND enabled = true",
        None,
    )
}
impl GetByProjectAndForgeStmt {
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
        forge_type: &'a T1,
    ) -> WebhookConfigRowQuery<'c, 'a, 's, C, WebhookConfigRow, 2> {
        WebhookConfigRowQuery {
            client,
            params: [project_id, forge_type],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<WebhookConfigRowBorrowed, tokio_postgres::Error> {
                Ok(WebhookConfigRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    forge_type: row.try_get(2)?,
                    secret_hash: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    created_at: row.try_get(5)?,
                })
            },
            mapper: |it| WebhookConfigRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetByProjectAndForgeParams<T1>,
        WebhookConfigRowQuery<'c, 'a, 's, C, WebhookConfigRow, 2>,
        C,
    > for GetByProjectAndForgeStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetByProjectAndForgeParams<T1>,
    ) -> WebhookConfigRowQuery<'c, 'a, 's, C, WebhookConfigRow, 2> {
        self.bind(client, &params.project_id, &params.forge_type)
    }
}
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("DELETE FROM webhook_configs WHERE id =$1", None)
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
        "INSERT INTO webhook_configs (project_id, forge_type, secret_hash, enabled) VALUES ($1,$2,$3,$4) ON CONFLICT (project_id, forge_type) DO UPDATE SET secret_hash = COALESCE(EXCLUDED.secret_hash, webhook_configs.secret_hash), enabled = EXCLUDED.enabled RETURNING *",
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
        project_id: &'a uuid::Uuid,
        forge_type: &'a T1,
        secret_hash: &'a Option<T2>,
        enabled: &'a bool,
    ) -> WebhookConfigRowQuery<'c, 'a, 's, C, WebhookConfigRow, 4> {
        WebhookConfigRowQuery {
            client,
            params: [project_id, forge_type, secret_hash, enabled],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<WebhookConfigRowBorrowed, tokio_postgres::Error> {
                Ok(WebhookConfigRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    forge_type: row.try_get(2)?,
                    secret_hash: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    created_at: row.try_get(5)?,
                })
            },
            mapper: |it| WebhookConfigRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpsertParams<T1, T2>,
        WebhookConfigRowQuery<'c, 'a, 's, C, WebhookConfigRow, 4>,
        C,
    > for UpsertStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpsertParams<T1, T2>,
    ) -> WebhookConfigRowQuery<'c, 'a, 's, C, WebhookConfigRow, 4> {
        self.bind(
            client,
            &params.project_id,
            &params.forge_type,
            &params.secret_hash,
            &params.enabled,
        )
    }
}
pub struct SyncForProjectDeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn sync_for_project_delete() -> SyncForProjectDeleteStmt {
    SyncForProjectDeleteStmt(
        "DELETE FROM webhook_configs WHERE project_id =$1 AND forge_type != ALL ($2::text[])",
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
        forge_types: &'a T2,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[project_id, forge_types]).await
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
        Box::pin(self.bind(client, &params.project_id, &params.forge_types))
    }
}
