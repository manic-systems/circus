// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
> {
    pub username: T1,
    pub email: T2,
    pub full_name: Option<T3>,
    pub password_hash: T4,
    pub role: T5,
}
#[derive(Clone, Copy, Debug)]
pub struct ListParams {
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct UpdateEmailParams<T1: crate::StringSql> {
    pub email: T1,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpdateFullNameParams<T1: crate::StringSql> {
    pub full_name: Option<T1>,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpdatePasswordParams<T1: crate::StringSql> {
    pub password_hash: T1,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpdateRoleParams<T1: crate::StringSql> {
    pub role: T1,
    pub id: uuid::Uuid,
}
#[derive(Clone, Copy, Debug)]
pub struct SetEnabledParams {
    pub enabled: bool,
    pub id: uuid::Uuid,
}
#[derive(Clone, Copy, Debug)]
pub struct SetPublicDashboardParams {
    pub public_dashboard: bool,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpsertOauthUserUpdateEmailParams<T1: crate::StringSql> {
    pub email: T1,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpsertOauthUserInsertParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
> {
    pub username: T1,
    pub email: T2,
    pub user_type: T3,
}
#[derive(Debug)]
pub struct CreateSessionParams<T1: crate::StringSql> {
    pub user_id: uuid::Uuid,
    pub session_token_hash: T1,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct UserRow {
    pub id: uuid::Uuid,
    pub username: String,
    pub email: String,
    pub full_name: Option<String>,
    pub password_hash: Option<String>,
    pub user_type: String,
    pub role: String,
    pub enabled: bool,
    pub email_verified: bool,
    pub public_dashboard: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub last_login_at: Option<chrono::DateTime<chrono::Utc>>,
}
pub struct UserRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub username: &'a str,
    pub email: &'a str,
    pub full_name: Option<&'a str>,
    pub password_hash: Option<&'a str>,
    pub user_type: &'a str,
    pub role: &'a str,
    pub enabled: bool,
    pub email_verified: bool,
    pub public_dashboard: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub last_login_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl<'a> From<UserRowBorrowed<'a>> for UserRow {
    fn from(
        UserRowBorrowed {
            id,
            username,
            email,
            full_name,
            password_hash,
            user_type,
            role,
            enabled,
            email_verified,
            public_dashboard,
            created_at,
            updated_at,
            last_login_at,
        }: UserRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            username: username.into(),
            email: email.into(),
            full_name: full_name.map(|v| v.into()),
            password_hash: password_hash.map(|v| v.into()),
            user_type: user_type.into(),
            role: role.into(),
            enabled,
            email_verified,
            public_dashboard,
            created_at,
            updated_at,
            last_login_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct UserRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<UserRowBorrowed, tokio_postgres::Error>,
    mapper: fn(UserRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> UserRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(UserRowBorrowed) -> R) -> UserRowQuery<'c, 'a, 's, C, R, N> {
        UserRowQuery {
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
pub struct UuidUuidQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<uuid::Uuid, tokio_postgres::Error>,
    mapper: fn(uuid::Uuid) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> UuidUuidQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(uuid::Uuid) -> R) -> UuidUuidQuery<'c, 'a, 's, C, R, N> {
        UuidUuidQuery {
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
        "INSERT INTO users (username, email, full_name, password_hash, role) VALUES ($1, $2, $3, $4, $5) RETURNING *",
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
        T5: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        username: &'a T1,
        email: &'a T2,
        full_name: &'a Option<T3>,
        password_hash: &'a T4,
        role: &'a T5,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 5> {
        UserRowQuery {
            client,
            params: [username, email, full_name, password_hash, role],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserRowBorrowed, tokio_postgres::Error> {
                    Ok(UserRowBorrowed {
                        id: row.try_get(0)?,
                        username: row.try_get(1)?,
                        email: row.try_get(2)?,
                        full_name: row.try_get(3)?,
                        password_hash: row.try_get(4)?,
                        user_type: row.try_get(5)?,
                        role: row.try_get(6)?,
                        enabled: row.try_get(7)?,
                        email_verified: row.try_get(8)?,
                        public_dashboard: row.try_get(9)?,
                        created_at: row.try_get(10)?,
                        updated_at: row.try_get(11)?,
                        last_login_at: row.try_get(12)?,
                    })
                },
            mapper: |it| UserRow::from(it),
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
    T5: crate::StringSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1, T2, T3, T4, T5>,
        UserRowQuery<'c, 'a, 's, C, UserRow, 5>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1, T2, T3, T4, T5>,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 5> {
        self.bind(
            client,
            &params.username,
            &params.email,
            &params.full_name,
            &params.password_hash,
            &params.role,
        )
    }
}
pub struct AuthenticateFetchStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn authenticate_fetch() -> AuthenticateFetchStmt {
    AuthenticateFetchStmt(
        "SELECT * FROM users WHERE username = $1 AND enabled = true",
        None,
    )
}
impl AuthenticateFetchStmt {
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
        username: &'a T1,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 1> {
        UserRowQuery {
            client,
            params: [username],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserRowBorrowed, tokio_postgres::Error> {
                    Ok(UserRowBorrowed {
                        id: row.try_get(0)?,
                        username: row.try_get(1)?,
                        email: row.try_get(2)?,
                        full_name: row.try_get(3)?,
                        password_hash: row.try_get(4)?,
                        user_type: row.try_get(5)?,
                        role: row.try_get(6)?,
                        enabled: row.try_get(7)?,
                        email_verified: row.try_get(8)?,
                        public_dashboard: row.try_get(9)?,
                        created_at: row.try_get(10)?,
                        updated_at: row.try_get(11)?,
                        last_login_at: row.try_get(12)?,
                    })
                },
            mapper: |it| UserRow::from(it),
        }
    }
}
pub struct AuthenticateTouchStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn authenticate_touch() -> AuthenticateTouchStmt {
    AuthenticateTouchStmt("UPDATE users SET last_login_at = NOW() WHERE id = $1", None)
}
impl AuthenticateTouchStmt {
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
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM users WHERE id = $1", None)
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
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 1> {
        UserRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserRowBorrowed, tokio_postgres::Error> {
                    Ok(UserRowBorrowed {
                        id: row.try_get(0)?,
                        username: row.try_get(1)?,
                        email: row.try_get(2)?,
                        full_name: row.try_get(3)?,
                        password_hash: row.try_get(4)?,
                        user_type: row.try_get(5)?,
                        role: row.try_get(6)?,
                        enabled: row.try_get(7)?,
                        email_verified: row.try_get(8)?,
                        public_dashboard: row.try_get(9)?,
                        created_at: row.try_get(10)?,
                        updated_at: row.try_get(11)?,
                        last_login_at: row.try_get(12)?,
                    })
                },
            mapper: |it| UserRow::from(it),
        }
    }
}
pub struct GetByUsernameStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_by_username() -> GetByUsernameStmt {
    GetByUsernameStmt("SELECT * FROM users WHERE username = $1", None)
}
impl GetByUsernameStmt {
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
        username: &'a T1,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 1> {
        UserRowQuery {
            client,
            params: [username],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserRowBorrowed, tokio_postgres::Error> {
                    Ok(UserRowBorrowed {
                        id: row.try_get(0)?,
                        username: row.try_get(1)?,
                        email: row.try_get(2)?,
                        full_name: row.try_get(3)?,
                        password_hash: row.try_get(4)?,
                        user_type: row.try_get(5)?,
                        role: row.try_get(6)?,
                        enabled: row.try_get(7)?,
                        email_verified: row.try_get(8)?,
                        public_dashboard: row.try_get(9)?,
                        created_at: row.try_get(10)?,
                        updated_at: row.try_get(11)?,
                        last_login_at: row.try_get(12)?,
                    })
                },
            mapper: |it| UserRow::from(it),
        }
    }
}
pub struct GetByEmailStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_by_email() -> GetByEmailStmt {
    GetByEmailStmt("SELECT * FROM users WHERE email = $1", None)
}
impl GetByEmailStmt {
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
        email: &'a T1,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 1> {
        UserRowQuery {
            client,
            params: [email],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserRowBorrowed, tokio_postgres::Error> {
                    Ok(UserRowBorrowed {
                        id: row.try_get(0)?,
                        username: row.try_get(1)?,
                        email: row.try_get(2)?,
                        full_name: row.try_get(3)?,
                        password_hash: row.try_get(4)?,
                        user_type: row.try_get(5)?,
                        role: row.try_get(6)?,
                        enabled: row.try_get(7)?,
                        email_verified: row.try_get(8)?,
                        public_dashboard: row.try_get(9)?,
                        created_at: row.try_get(10)?,
                        updated_at: row.try_get(11)?,
                        last_login_at: row.try_get(12)?,
                    })
                },
            mapper: |it| UserRow::from(it),
        }
    }
}
pub struct ListStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list() -> ListStmt {
    ListStmt(
        "SELECT * FROM users ORDER BY created_at DESC LIMIT $1 OFFSET $2",
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
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 2> {
        UserRowQuery {
            client,
            params: [limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserRowBorrowed, tokio_postgres::Error> {
                    Ok(UserRowBorrowed {
                        id: row.try_get(0)?,
                        username: row.try_get(1)?,
                        email: row.try_get(2)?,
                        full_name: row.try_get(3)?,
                        password_hash: row.try_get(4)?,
                        user_type: row.try_get(5)?,
                        role: row.try_get(6)?,
                        enabled: row.try_get(7)?,
                        email_verified: row.try_get(8)?,
                        public_dashboard: row.try_get(9)?,
                        created_at: row.try_get(10)?,
                        updated_at: row.try_get(11)?,
                        last_login_at: row.try_get(12)?,
                    })
                },
            mapper: |it| UserRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListParams,
        UserRowQuery<'c, 'a, 's, C, UserRow, 2>,
        C,
    > for ListStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListParams,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 2> {
        self.bind(client, &params.limit, &params.offset)
    }
}
pub struct CountStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count() -> CountStmt {
    CountStmt("SELECT COUNT(*) FROM users", None)
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
pub struct UpdateEmailStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_email() -> UpdateEmailStmt {
    UpdateEmailStmt(
        "UPDATE users SET email = $1 WHERE id = $2 RETURNING *",
        None,
    )
}
impl UpdateEmailStmt {
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
        email: &'a T1,
        id: &'a uuid::Uuid,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 2> {
        UserRowQuery {
            client,
            params: [email, id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserRowBorrowed, tokio_postgres::Error> {
                    Ok(UserRowBorrowed {
                        id: row.try_get(0)?,
                        username: row.try_get(1)?,
                        email: row.try_get(2)?,
                        full_name: row.try_get(3)?,
                        password_hash: row.try_get(4)?,
                        user_type: row.try_get(5)?,
                        role: row.try_get(6)?,
                        enabled: row.try_get(7)?,
                        email_verified: row.try_get(8)?,
                        public_dashboard: row.try_get(9)?,
                        created_at: row.try_get(10)?,
                        updated_at: row.try_get(11)?,
                        last_login_at: row.try_get(12)?,
                    })
                },
            mapper: |it| UserRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpdateEmailParams<T1>,
        UserRowQuery<'c, 'a, 's, C, UserRow, 2>,
        C,
    > for UpdateEmailStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpdateEmailParams<T1>,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 2> {
        self.bind(client, &params.email, &params.id)
    }
}
pub struct UpdateFullNameStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_full_name() -> UpdateFullNameStmt {
    UpdateFullNameStmt("UPDATE users SET full_name = $1 WHERE id = $2", None)
}
impl UpdateFullNameStmt {
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
        full_name: &'a Option<T1>,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[full_name, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpdateFullNameParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpdateFullNameStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpdateFullNameParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.full_name, &params.id))
    }
}
pub struct UpdatePasswordStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_password() -> UpdatePasswordStmt {
    UpdatePasswordStmt("UPDATE users SET password_hash = $1 WHERE id = $2", None)
}
impl UpdatePasswordStmt {
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
        password_hash: &'a T1,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[password_hash, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpdatePasswordParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpdatePasswordStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpdatePasswordParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.password_hash, &params.id))
    }
}
pub struct UpdateRoleStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_role() -> UpdateRoleStmt {
    UpdateRoleStmt("UPDATE users SET role = $1 WHERE id = $2", None)
}
impl UpdateRoleStmt {
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
        role: &'a T1,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[role, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpdateRoleParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpdateRoleStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpdateRoleParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.role, &params.id))
    }
}
pub struct SetEnabledStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_enabled() -> SetEnabledStmt {
    SetEnabledStmt("UPDATE users SET enabled = $1 WHERE id = $2", None)
}
impl SetEnabledStmt {
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
        enabled: &'a bool,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[enabled, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetEnabledParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetEnabledStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SetEnabledParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.enabled, &params.id))
    }
}
pub struct SetPublicDashboardStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_public_dashboard() -> SetPublicDashboardStmt {
    SetPublicDashboardStmt("UPDATE users SET public_dashboard = $1 WHERE id = $2", None)
}
impl SetPublicDashboardStmt {
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
        public_dashboard: &'a bool,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[public_dashboard, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetPublicDashboardParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetPublicDashboardStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SetPublicDashboardParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.public_dashboard, &params.id))
    }
}
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("DELETE FROM users WHERE id = $1", None)
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
pub struct UpsertOauthUserFetchStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert_oauth_user_fetch() -> UpsertOauthUserFetchStmt {
    UpsertOauthUserFetchStmt("SELECT * FROM users WHERE username = $1", None)
}
impl UpsertOauthUserFetchStmt {
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
        username: &'a T1,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 1> {
        UserRowQuery {
            client,
            params: [username],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserRowBorrowed, tokio_postgres::Error> {
                    Ok(UserRowBorrowed {
                        id: row.try_get(0)?,
                        username: row.try_get(1)?,
                        email: row.try_get(2)?,
                        full_name: row.try_get(3)?,
                        password_hash: row.try_get(4)?,
                        user_type: row.try_get(5)?,
                        role: row.try_get(6)?,
                        enabled: row.try_get(7)?,
                        email_verified: row.try_get(8)?,
                        public_dashboard: row.try_get(9)?,
                        created_at: row.try_get(10)?,
                        updated_at: row.try_get(11)?,
                        last_login_at: row.try_get(12)?,
                    })
                },
            mapper: |it| UserRow::from(it),
        }
    }
}
pub struct UpsertOauthUserUpdateEmailStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert_oauth_user_update_email() -> UpsertOauthUserUpdateEmailStmt {
    UpsertOauthUserUpdateEmailStmt(
        "UPDATE users SET email = $1, last_login_at = NOW(), updated_at = NOW() WHERE id = $2",
        None,
    )
}
impl UpsertOauthUserUpdateEmailStmt {
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
        email: &'a T1,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[email, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        UpsertOauthUserUpdateEmailParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for UpsertOauthUserUpdateEmailStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a UpsertOauthUserUpdateEmailParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.email, &params.id))
    }
}
pub struct UpsertOauthUserTouchStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert_oauth_user_touch() -> UpsertOauthUserTouchStmt {
    UpsertOauthUserTouchStmt(
        "UPDATE users SET last_login_at = NOW(), updated_at = NOW() WHERE id = $1",
        None,
    )
}
impl UpsertOauthUserTouchStmt {
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
pub struct UpsertOauthUserInsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert_oauth_user_insert() -> UpsertOauthUserInsertStmt {
    UpsertOauthUserInsertStmt(
        "INSERT INTO users (username, email, user_type, password_hash, role) VALUES ($1, $2, $3, NULL, 'read-only') RETURNING *",
        None,
    )
}
impl UpsertOauthUserInsertStmt {
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
        username: &'a T1,
        email: &'a T2,
        user_type: &'a T3,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 3> {
        UserRowQuery {
            client,
            params: [username, email, user_type],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserRowBorrowed, tokio_postgres::Error> {
                    Ok(UserRowBorrowed {
                        id: row.try_get(0)?,
                        username: row.try_get(1)?,
                        email: row.try_get(2)?,
                        full_name: row.try_get(3)?,
                        password_hash: row.try_get(4)?,
                        user_type: row.try_get(5)?,
                        role: row.try_get(6)?,
                        enabled: row.try_get(7)?,
                        email_verified: row.try_get(8)?,
                        public_dashboard: row.try_get(9)?,
                        created_at: row.try_get(10)?,
                        updated_at: row.try_get(11)?,
                        last_login_at: row.try_get(12)?,
                    })
                },
            mapper: |it| UserRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpsertOauthUserInsertParams<T1, T2, T3>,
        UserRowQuery<'c, 'a, 's, C, UserRow, 3>,
        C,
    > for UpsertOauthUserInsertStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpsertOauthUserInsertParams<T1, T2, T3>,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 3> {
        self.bind(client, &params.username, &params.email, &params.user_type)
    }
}
pub struct CreateSessionStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create_session() -> CreateSessionStmt {
    CreateSessionStmt(
        "INSERT INTO user_sessions (user_id, session_token_hash, expires_at) VALUES ($1, $2, $3) RETURNING id",
        None,
    )
}
impl CreateSessionStmt {
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
        user_id: &'a uuid::Uuid,
        session_token_hash: &'a T1,
        expires_at: &'a chrono::DateTime<chrono::Utc>,
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 3> {
        UuidUuidQuery {
            client,
            params: [user_id, session_token_hash, expires_at],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateSessionParams<T1>,
        UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 3>,
        C,
    > for CreateSessionStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateSessionParams<T1>,
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 3> {
        self.bind(
            client,
            &params.user_id,
            &params.session_token_hash,
            &params.expires_at,
        )
    }
}
pub struct ValidateSessionFetchStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn validate_session_fetch() -> ValidateSessionFetchStmt {
    ValidateSessionFetchStmt(
        "SELECT u.* FROM users u JOIN user_sessions s ON u.id = s.user_id WHERE s.session_token_hash = $1 AND s.expires_at > NOW() AND u.enabled = true",
        None,
    )
}
impl ValidateSessionFetchStmt {
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
        session_token_hash: &'a T1,
    ) -> UserRowQuery<'c, 'a, 's, C, UserRow, 1> {
        UserRowQuery {
            client,
            params: [session_token_hash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<UserRowBorrowed, tokio_postgres::Error> {
                    Ok(UserRowBorrowed {
                        id: row.try_get(0)?,
                        username: row.try_get(1)?,
                        email: row.try_get(2)?,
                        full_name: row.try_get(3)?,
                        password_hash: row.try_get(4)?,
                        user_type: row.try_get(5)?,
                        role: row.try_get(6)?,
                        enabled: row.try_get(7)?,
                        email_verified: row.try_get(8)?,
                        public_dashboard: row.try_get(9)?,
                        created_at: row.try_get(10)?,
                        updated_at: row.try_get(11)?,
                        last_login_at: row.try_get(12)?,
                    })
                },
            mapper: |it| UserRow::from(it),
        }
    }
}
pub struct ValidateSessionTouchStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn validate_session_touch() -> ValidateSessionTouchStmt {
    ValidateSessionTouchStmt(
        "UPDATE user_sessions SET last_used_at = NOW() WHERE session_token_hash = $1",
        None,
    )
}
impl ValidateSessionTouchStmt {
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
        session_token_hash: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[session_token_hash]).await
    }
}
pub struct DeleteSessionStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_session() -> DeleteSessionStmt {
    DeleteSessionStmt(
        "DELETE FROM user_sessions WHERE session_token_hash = $1",
        None,
    )
}
impl DeleteSessionStmt {
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
        session_token_hash: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[session_token_hash]).await
    }
}
