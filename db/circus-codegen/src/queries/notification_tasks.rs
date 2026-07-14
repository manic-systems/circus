// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql, T2: crate::JsonSql> {
    pub notification_type: T1,
    pub payload: T2,
    pub max_attempts: i32,
}
#[derive(Debug)]
pub struct MarkFailedAndRetryParams<T1: crate::StringSql> {
    pub error: Option<T1>,
    pub task_id: uuid::Uuid,
}
#[derive(Debug, Clone, PartialEq)]
pub struct NotificationTaskRow {
    pub id: uuid::Uuid,
    pub notification_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub next_retry_at: chrono::DateTime<chrono::Utc>,
    pub last_error: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}
pub struct NotificationTaskRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub notification_type: &'a str,
    pub payload: postgres_types::Json<&'a serde_json::value::RawValue>,
    pub status: &'a str,
    pub attempts: i32,
    pub max_attempts: i32,
    pub next_retry_at: chrono::DateTime<chrono::Utc>,
    pub last_error: Option<&'a str>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl<'a> From<NotificationTaskRowBorrowed<'a>> for NotificationTaskRow {
    fn from(
        NotificationTaskRowBorrowed {
            id,
            notification_type,
            payload,
            status,
            attempts,
            max_attempts,
            next_retry_at,
            last_error,
            created_at,
            completed_at,
        }: NotificationTaskRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            notification_type: notification_type.into(),
            payload: serde_json::from_str(payload.0.get()).unwrap(),
            status: status.into(),
            attempts,
            max_attempts,
            next_retry_at,
            last_error: last_error.map(|v| v.into()),
            created_at,
            completed_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct NotificationTaskRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor:
        fn(&tokio_postgres::Row) -> Result<NotificationTaskRowBorrowed, tokio_postgres::Error>,
    mapper: fn(NotificationTaskRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> NotificationTaskRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(NotificationTaskRowBorrowed) -> R,
    ) -> NotificationTaskRowQuery<'c, 'a, 's, C, R, N> {
        NotificationTaskRowQuery {
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
pub struct CreateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create() -> CreateStmt {
    CreateStmt(
        "INSERT INTO notification_tasks (notification_type, payload, max_attempts) VALUES ($1, $2, $3) RETURNING *",
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
        notification_type: &'a T1,
        payload: &'a T2,
        max_attempts: &'a i32,
    ) -> NotificationTaskRowQuery<'c, 'a, 's, C, NotificationTaskRow, 3> {
        NotificationTaskRowQuery {
            client,
            params: [notification_type, payload, max_attempts],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NotificationTaskRowBorrowed, tokio_postgres::Error> {
                Ok(NotificationTaskRowBorrowed {
                    id: row.try_get(0)?,
                    notification_type: row.try_get(1)?,
                    payload: row.try_get(2)?,
                    status: row.try_get(3)?,
                    attempts: row.try_get(4)?,
                    max_attempts: row.try_get(5)?,
                    next_retry_at: row.try_get(6)?,
                    last_error: row.try_get(7)?,
                    created_at: row.try_get(8)?,
                    completed_at: row.try_get(9)?,
                })
            },
            mapper: |it| NotificationTaskRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::JsonSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1, T2>,
        NotificationTaskRowQuery<'c, 'a, 's, C, NotificationTaskRow, 3>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1, T2>,
    ) -> NotificationTaskRowQuery<'c, 'a, 's, C, NotificationTaskRow, 3> {
        self.bind(
            client,
            &params.notification_type,
            &params.payload,
            &params.max_attempts,
        )
    }
}
pub struct ListPendingStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_pending() -> ListPendingStmt {
    ListPendingStmt(
        "SELECT * FROM notification_tasks WHERE status = 'pending' AND next_retry_at <= NOW() ORDER BY next_retry_at ASC LIMIT $1",
        None,
    )
}
impl ListPendingStmt {
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
    ) -> NotificationTaskRowQuery<'c, 'a, 's, C, NotificationTaskRow, 1> {
        NotificationTaskRowQuery {
            client,
            params: [limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NotificationTaskRowBorrowed, tokio_postgres::Error> {
                Ok(NotificationTaskRowBorrowed {
                    id: row.try_get(0)?,
                    notification_type: row.try_get(1)?,
                    payload: row.try_get(2)?,
                    status: row.try_get(3)?,
                    attempts: row.try_get(4)?,
                    max_attempts: row.try_get(5)?,
                    next_retry_at: row.try_get(6)?,
                    last_error: row.try_get(7)?,
                    created_at: row.try_get(8)?,
                    completed_at: row.try_get(9)?,
                })
            },
            mapper: |it| NotificationTaskRow::from(it),
        }
    }
}
pub struct ClaimPendingStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn claim_pending() -> ClaimPendingStmt {
    ClaimPendingStmt(
        "WITH claimed AS ( SELECT id FROM notification_tasks WHERE status = 'pending' AND next_retry_at <= NOW() ORDER BY next_retry_at ASC LIMIT $1 FOR UPDATE SKIP LOCKED ) UPDATE notification_tasks nt SET status = 'running', attempts = attempts + 1 FROM claimed WHERE nt.id = claimed.id RETURNING nt.*",
        None,
    )
}
impl ClaimPendingStmt {
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
    ) -> NotificationTaskRowQuery<'c, 'a, 's, C, NotificationTaskRow, 1> {
        NotificationTaskRowQuery {
            client,
            params: [limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NotificationTaskRowBorrowed, tokio_postgres::Error> {
                Ok(NotificationTaskRowBorrowed {
                    id: row.try_get(0)?,
                    notification_type: row.try_get(1)?,
                    payload: row.try_get(2)?,
                    status: row.try_get(3)?,
                    attempts: row.try_get(4)?,
                    max_attempts: row.try_get(5)?,
                    next_retry_at: row.try_get(6)?,
                    last_error: row.try_get(7)?,
                    created_at: row.try_get(8)?,
                    completed_at: row.try_get(9)?,
                })
            },
            mapper: |it| NotificationTaskRow::from(it),
        }
    }
}
pub struct ListRecentStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_recent() -> ListRecentStmt {
    ListRecentStmt(
        "SELECT * FROM notification_tasks ORDER BY created_at DESC LIMIT $1",
        None,
    )
}
impl ListRecentStmt {
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
    ) -> NotificationTaskRowQuery<'c, 'a, 's, C, NotificationTaskRow, 1> {
        NotificationTaskRowQuery {
            client,
            params: [limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NotificationTaskRowBorrowed, tokio_postgres::Error> {
                Ok(NotificationTaskRowBorrowed {
                    id: row.try_get(0)?,
                    notification_type: row.try_get(1)?,
                    payload: row.try_get(2)?,
                    status: row.try_get(3)?,
                    attempts: row.try_get(4)?,
                    max_attempts: row.try_get(5)?,
                    next_retry_at: row.try_get(6)?,
                    last_error: row.try_get(7)?,
                    created_at: row.try_get(8)?,
                    completed_at: row.try_get(9)?,
                })
            },
            mapper: |it| NotificationTaskRow::from(it),
        }
    }
}
pub struct MarkRunningStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn mark_running() -> MarkRunningStmt {
    MarkRunningStmt(
        "UPDATE notification_tasks SET status = 'running', attempts = attempts + 1 WHERE id = $1",
        None,
    )
}
impl MarkRunningStmt {
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
        task_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[task_id]).await
    }
}
pub struct MarkCompletedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn mark_completed() -> MarkCompletedStmt {
    MarkCompletedStmt(
        "UPDATE notification_tasks SET status = 'completed', completed_at = NOW() WHERE id = $1",
        None,
    )
}
impl MarkCompletedStmt {
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
        task_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[task_id]).await
    }
}
pub struct MarkFailedAndRetryStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn mark_failed_and_retry() -> MarkFailedAndRetryStmt {
    MarkFailedAndRetryStmt(
        "UPDATE notification_tasks SET status = CASE WHEN attempts >= max_attempts THEN 'failed'::varchar ELSE 'pending'::varchar END, last_error = $1, next_retry_at = CASE WHEN attempts >= max_attempts THEN NOW() ELSE NOW() + (POWER(2, attempts - 1) || ' seconds')::interval END, completed_at = CASE WHEN attempts >= max_attempts THEN NOW() ELSE NULL END WHERE id = $2",
        None,
    )
}
impl MarkFailedAndRetryStmt {
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
        error: &'a Option<T1>,
        task_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[error, task_id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        MarkFailedAndRetryParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for MarkFailedAndRetryStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a MarkFailedAndRetryParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.error, &params.task_id))
    }
}
pub struct RequeueFailedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn requeue_failed() -> RequeueFailedStmt {
    RequeueFailedStmt(
        "UPDATE notification_tasks SET status = 'pending', attempts = 0, next_retry_at = NOW(), last_error = NULL, completed_at = NULL WHERE id = $1 AND status = 'failed' RETURNING *",
        None,
    )
}
impl RequeueFailedStmt {
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
        task_id: &'a uuid::Uuid,
    ) -> NotificationTaskRowQuery<'c, 'a, 's, C, NotificationTaskRow, 1> {
        NotificationTaskRowQuery {
            client,
            params: [task_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NotificationTaskRowBorrowed, tokio_postgres::Error> {
                Ok(NotificationTaskRowBorrowed {
                    id: row.try_get(0)?,
                    notification_type: row.try_get(1)?,
                    payload: row.try_get(2)?,
                    status: row.try_get(3)?,
                    attempts: row.try_get(4)?,
                    max_attempts: row.try_get(5)?,
                    next_retry_at: row.try_get(6)?,
                    last_error: row.try_get(7)?,
                    created_at: row.try_get(8)?,
                    completed_at: row.try_get(9)?,
                })
            },
            mapper: |it| NotificationTaskRow::from(it),
        }
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM notification_tasks WHERE id = $1", None)
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
        task_id: &'a uuid::Uuid,
    ) -> NotificationTaskRowQuery<'c, 'a, 's, C, NotificationTaskRow, 1> {
        NotificationTaskRowQuery {
            client,
            params: [task_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<NotificationTaskRowBorrowed, tokio_postgres::Error> {
                Ok(NotificationTaskRowBorrowed {
                    id: row.try_get(0)?,
                    notification_type: row.try_get(1)?,
                    payload: row.try_get(2)?,
                    status: row.try_get(3)?,
                    attempts: row.try_get(4)?,
                    max_attempts: row.try_get(5)?,
                    next_retry_at: row.try_get(6)?,
                    last_error: row.try_get(7)?,
                    created_at: row.try_get(8)?,
                    completed_at: row.try_get(9)?,
                })
            },
            mapper: |it| NotificationTaskRow::from(it),
        }
    }
}
pub struct CleanupOldTasksStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn cleanup_old_tasks() -> CleanupOldTasksStmt {
    CleanupOldTasksStmt(
        "DELETE FROM notification_tasks WHERE status IN ('completed', 'failed') AND (completed_at < NOW() - ($1 || ' days')::interval OR created_at < NOW() - ($1 || ' days')::interval)",
        None,
    )
}
impl CleanupOldTasksStmt {
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
        retention_days: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[retention_days]).await
    }
}
pub struct CountPendingStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_pending() -> CountPendingStmt {
    CountPendingStmt(
        "SELECT COUNT(*) FROM notification_tasks WHERE status = 'pending'",
        None,
    )
}
impl CountPendingStmt {
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
pub struct CountFailedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_failed() -> CountFailedStmt {
    CountFailedStmt(
        "SELECT COUNT(*) FROM notification_tasks WHERE status = 'failed'",
        None,
    )
}
impl CountFailedStmt {
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
