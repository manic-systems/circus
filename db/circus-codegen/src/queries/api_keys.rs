// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub name: T1,
    pub key_hash: T2,
    pub role: T3,
}
#[derive(Debug)]
pub struct UpsertParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub name: T1,
    pub key_hash: T2,
    pub role: T3,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ApiKeyRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub key_hash: String,
    pub role: String,
    pub user_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}
pub struct ApiKeyRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub name: &'a str,
    pub key_hash: &'a str,
    pub role: &'a str,
    pub user_id: Option<uuid::Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl<'a> From<ApiKeyRowBorrowed<'a>> for ApiKeyRow {
    fn from(
        ApiKeyRowBorrowed {
            id,
            name,
            key_hash,
            role,
            user_id,
            created_at,
            last_used_at,
        }: ApiKeyRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            key_hash: key_hash.into(),
            role: role.into(),
            user_id,
            created_at,
            last_used_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct ApiKeyRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ApiKeyRowBorrowed, tokio_postgres::Error>,
    mapper: fn(ApiKeyRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ApiKeyRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ApiKeyRowBorrowed) -> R) -> ApiKeyRowQuery<'c, 'a, 's, C, R, N> {
        ApiKeyRowQuery {
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
        "INSERT INTO api_keys (name, key_hash, role) VALUES ($1,$2,$3) RETURNING *",
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
    >(
        &'s self,
        client: &'c C,
        name: &'a T1,
        key_hash: &'a T2,
        role: &'a T3,
    ) -> ApiKeyRowQuery<'c, 'a, 's, C, ApiKeyRow, 3> {
        ApiKeyRowQuery {
            client,
            params: [name, key_hash, role],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ApiKeyRowBorrowed, tokio_postgres::Error> {
                    Ok(ApiKeyRowBorrowed {
                        id: row.try_get(0)?,
                        name: row.try_get(1)?,
                        key_hash: row.try_get(2)?,
                        role: row.try_get(3)?,
                        user_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        last_used_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ApiKeyRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1, T2, T3>,
        ApiKeyRowQuery<'c, 'a, 's, C, ApiKeyRow, 3>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1, T2, T3>,
    ) -> ApiKeyRowQuery<'c, 'a, 's, C, ApiKeyRow, 3> {
        self.bind(client, &params.name, &params.key_hash, &params.role)
    }
}
pub struct UpsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert() -> UpsertStmt {
    UpsertStmt(
        "INSERT INTO api_keys (name, key_hash, role) VALUES ($1,$2,$3) ON CONFLICT (key_hash) DO UPDATE SET name = EXCLUDED.name, role = EXCLUDED.role RETURNING *",
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
    >(
        &'s self,
        client: &'c C,
        name: &'a T1,
        key_hash: &'a T2,
        role: &'a T3,
    ) -> ApiKeyRowQuery<'c, 'a, 's, C, ApiKeyRow, 3> {
        ApiKeyRowQuery {
            client,
            params: [name, key_hash, role],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ApiKeyRowBorrowed, tokio_postgres::Error> {
                    Ok(ApiKeyRowBorrowed {
                        id: row.try_get(0)?,
                        name: row.try_get(1)?,
                        key_hash: row.try_get(2)?,
                        role: row.try_get(3)?,
                        user_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        last_used_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ApiKeyRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpsertParams<T1, T2, T3>,
        ApiKeyRowQuery<'c, 'a, 's, C, ApiKeyRow, 3>,
        C,
    > for UpsertStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpsertParams<T1, T2, T3>,
    ) -> ApiKeyRowQuery<'c, 'a, 's, C, ApiKeyRow, 3> {
        self.bind(client, &params.name, &params.key_hash, &params.role)
    }
}
pub struct GetByHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_by_hash() -> GetByHashStmt {
    GetByHashStmt("SELECT * FROM api_keys WHERE key_hash =$1", None)
}
impl GetByHashStmt {
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
        key_hash: &'a T1,
    ) -> ApiKeyRowQuery<'c, 'a, 's, C, ApiKeyRow, 1> {
        ApiKeyRowQuery {
            client,
            params: [key_hash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ApiKeyRowBorrowed, tokio_postgres::Error> {
                    Ok(ApiKeyRowBorrowed {
                        id: row.try_get(0)?,
                        name: row.try_get(1)?,
                        key_hash: row.try_get(2)?,
                        role: row.try_get(3)?,
                        user_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        last_used_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ApiKeyRow::from(it),
        }
    }
}
pub struct ListStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list() -> ListStmt {
    ListStmt("SELECT * FROM api_keys ORDER BY created_at DESC", None)
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
    ) -> ApiKeyRowQuery<'c, 'a, 's, C, ApiKeyRow, 0> {
        ApiKeyRowQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<ApiKeyRowBorrowed, tokio_postgres::Error> {
                    Ok(ApiKeyRowBorrowed {
                        id: row.try_get(0)?,
                        name: row.try_get(1)?,
                        key_hash: row.try_get(2)?,
                        role: row.try_get(3)?,
                        user_id: row.try_get(4)?,
                        created_at: row.try_get(5)?,
                        last_used_at: row.try_get(6)?,
                    })
                },
            mapper: |it| ApiKeyRow::from(it),
        }
    }
}
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("DELETE FROM api_keys WHERE id =$1", None)
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
pub struct TouchLastUsedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn touch_last_used() -> TouchLastUsedStmt {
    TouchLastUsedStmt(
        "UPDATE api_keys SET last_used_at = NOW() WHERE id =$1",
        None,
    )
}
impl TouchLastUsedStmt {
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
