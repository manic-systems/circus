// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct RecordParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::JsonSql,
    T7: crate::StringSql,
> {
    pub actor_kind: T1,
    pub actor_id: Option<uuid::Uuid>,
    pub actor_name: Option<T2>,
    pub action: T3,
    pub target_kind: Option<T4>,
    pub target_id: Option<T5>,
    pub details: T6,
    pub remote_addr: Option<T7>,
}
#[derive(Clone, Copy, Debug)]
pub struct ListParams {
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AuditLogRow {
    pub id: uuid::Uuid,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub actor_kind: String,
    pub actor_id: Option<uuid::Uuid>,
    pub actor_name: Option<String>,
    pub action: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub details: serde_json::Value,
    pub remote_addr: Option<String>,
}
pub struct AuditLogRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub actor_kind: &'a str,
    pub actor_id: Option<uuid::Uuid>,
    pub actor_name: Option<&'a str>,
    pub action: &'a str,
    pub target_kind: Option<&'a str>,
    pub target_id: Option<&'a str>,
    pub details: postgres_types::Json<&'a serde_json::value::RawValue>,
    pub remote_addr: Option<&'a str>,
}
impl<'a> From<AuditLogRowBorrowed<'a>> for AuditLogRow {
    fn from(
        AuditLogRowBorrowed {
            id,
            occurred_at,
            actor_kind,
            actor_id,
            actor_name,
            action,
            target_kind,
            target_id,
            details,
            remote_addr,
        }: AuditLogRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            occurred_at,
            actor_kind: actor_kind.into(),
            actor_id,
            actor_name: actor_name.map(|v| v.into()),
            action: action.into(),
            target_kind: target_kind.map(|v| v.into()),
            target_id: target_id.map(|v| v.into()),
            details: serde_json::from_str(details.0.get()).unwrap(),
            remote_addr: remote_addr.map(|v| v.into()),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct AuditLogRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<AuditLogRowBorrowed, tokio_postgres::Error>,
    mapper: fn(AuditLogRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> AuditLogRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(AuditLogRowBorrowed) -> R,
    ) -> AuditLogRowQuery<'c, 'a, 's, C, R, N> {
        AuditLogRowQuery {
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
pub struct RecordStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record() -> RecordStmt {
    RecordStmt(
        "INSERT INTO audit_log ( actor_kind, actor_id, actor_name, action, target_kind, target_id, details, remote_addr ) VALUES ( $1, $2, $3, $4, $5, $6, $7, $8 )",
        None,
    )
}
impl RecordStmt {
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
        T2: crate::StringSql,
        T3: crate::StringSql,
        T4: crate::StringSql,
        T5: crate::StringSql,
        T6: crate::JsonSql,
        T7: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        actor_kind: &'a T1,
        actor_id: &'a Option<uuid::Uuid>,
        actor_name: &'a Option<T2>,
        action: &'a T3,
        target_kind: &'a Option<T4>,
        target_id: &'a Option<T5>,
        details: &'a T6,
        remote_addr: &'a Option<T7>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    actor_kind,
                    actor_id,
                    actor_name,
                    action,
                    target_kind,
                    target_id,
                    details,
                    remote_addr,
                ],
            )
            .await
    }
}
impl<
    'a,
    C: GenericClient + Send + Sync,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::JsonSql,
    T7: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        RecordParams<T1, T2, T3, T4, T5, T6, T7>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for RecordStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a RecordParams<T1, T2, T3, T4, T5, T6, T7>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.actor_kind,
            &params.actor_id,
            &params.actor_name,
            &params.action,
            &params.target_kind,
            &params.target_id,
            &params.details,
            &params.remote_addr,
        ))
    }
}
pub struct ListStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list() -> ListStmt {
    ListStmt(
        "SELECT id, occurred_at, actor_kind, actor_id, actor_name, action, target_kind, target_id, details, remote_addr FROM audit_log ORDER BY occurred_at DESC LIMIT $1 OFFSET $2",
        None,
    )
}
impl ListStmt {
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
    ) -> AuditLogRowQuery<'c, 'a, 's, C, AuditLogRow, 2> {
        AuditLogRowQuery {
            client,
            params: [limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<AuditLogRowBorrowed, tokio_postgres::Error> {
                    Ok(AuditLogRowBorrowed {
                        id: row.try_get(0)?,
                        occurred_at: row.try_get(1)?,
                        actor_kind: row.try_get(2)?,
                        actor_id: row.try_get(3)?,
                        actor_name: row.try_get(4)?,
                        action: row.try_get(5)?,
                        target_kind: row.try_get(6)?,
                        target_id: row.try_get(7)?,
                        details: row.try_get(8)?,
                        remote_addr: row.try_get(9)?,
                    })
                },
            mapper: |it| AuditLogRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListParams,
        AuditLogRowQuery<'c, 'a, 's, C, AuditLogRow, 2>,
        C,
    > for ListStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListParams,
    ) -> AuditLogRowQuery<'c, 'a, 's, C, AuditLogRow, 2> {
        self.bind(client, &params.limit, &params.offset)
    }
}
pub struct CountStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count() -> CountStmt {
    CountStmt("SELECT COUNT(*) FROM audit_log", None)
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
