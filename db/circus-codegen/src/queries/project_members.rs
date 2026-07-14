// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql> {
    pub project_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub role: T1,
}
#[derive(Clone, Copy, Debug)]
pub struct GetByProjectAndUserParams {
    pub project_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpdateParams<T1: crate::StringSql> {
    pub role: T1,
    pub id: uuid::Uuid,
}
#[derive(Clone, Copy, Debug)]
pub struct DeleteByProjectAndUserParams {
    pub project_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpsertParams<T1: crate::StringSql> {
    pub project_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub role: T1,
}
#[derive(Debug)]
pub struct SyncDeleteRemovedParams<T1: crate::ArraySql<Item = uuid::Uuid>> {
    pub project_id: uuid::Uuid,
    pub user_ids: T1,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectMemberRow {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub role: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
pub struct ProjectMemberRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub role: &'a str,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
impl<'a> From<ProjectMemberRowBorrowed<'a>> for ProjectMemberRow {
    fn from(
        ProjectMemberRowBorrowed {
            id,
            project_id,
            user_id,
            role,
            created_at,
        }: ProjectMemberRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            project_id,
            user_id,
            role: role.into(),
            created_at,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct ProjectMemberRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<ProjectMemberRowBorrowed, tokio_postgres::Error>,
    mapper: fn(ProjectMemberRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ProjectMemberRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(ProjectMemberRowBorrowed) -> R,
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, R, N> {
        ProjectMemberRowQuery {
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
        "INSERT INTO project_members (project_id, user_id, role) VALUES ($1,$2,$3) RETURNING *",
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
        user_id: &'a uuid::Uuid,
        role: &'a T1,
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 3> {
        ProjectMemberRowQuery {
            client,
            params: [project_id, user_id, role],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ProjectMemberRowBorrowed, tokio_postgres::Error> {
                Ok(ProjectMemberRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    user_id: row.try_get(2)?,
                    role: row.try_get(3)?,
                    created_at: row.try_get(4)?,
                })
            },
            mapper: |it| ProjectMemberRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1>,
        ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 3>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1>,
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 3> {
        self.bind(client, &params.project_id, &params.user_id, &params.role)
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM project_members WHERE id =$1", None)
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
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 1> {
        ProjectMemberRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ProjectMemberRowBorrowed, tokio_postgres::Error> {
                Ok(ProjectMemberRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    user_id: row.try_get(2)?,
                    role: row.try_get(3)?,
                    created_at: row.try_get(4)?,
                })
            },
            mapper: |it| ProjectMemberRow::from(it),
        }
    }
}
pub struct GetByProjectAndUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_by_project_and_user() -> GetByProjectAndUserStmt {
    GetByProjectAndUserStmt(
        "SELECT * FROM project_members WHERE project_id =$1 AND user_id =$2",
        None,
    )
}
impl GetByProjectAndUserStmt {
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
        user_id: &'a uuid::Uuid,
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 2> {
        ProjectMemberRowQuery {
            client,
            params: [project_id, user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ProjectMemberRowBorrowed, tokio_postgres::Error> {
                Ok(ProjectMemberRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    user_id: row.try_get(2)?,
                    role: row.try_get(3)?,
                    created_at: row.try_get(4)?,
                })
            },
            mapper: |it| ProjectMemberRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetByProjectAndUserParams,
        ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 2>,
        C,
    > for GetByProjectAndUserStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetByProjectAndUserParams,
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 2> {
        self.bind(client, &params.project_id, &params.user_id)
    }
}
pub struct ListForProjectStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_project() -> ListForProjectStmt {
    ListForProjectStmt(
        "SELECT * FROM project_members WHERE project_id =$1 ORDER BY created_at",
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
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 1> {
        ProjectMemberRowQuery {
            client,
            params: [project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ProjectMemberRowBorrowed, tokio_postgres::Error> {
                Ok(ProjectMemberRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    user_id: row.try_get(2)?,
                    role: row.try_get(3)?,
                    created_at: row.try_get(4)?,
                })
            },
            mapper: |it| ProjectMemberRow::from(it),
        }
    }
}
pub struct ListForUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_user() -> ListForUserStmt {
    ListForUserStmt(
        "SELECT * FROM project_members WHERE user_id =$1 ORDER BY created_at",
        None,
    )
}
impl ListForUserStmt {
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
        user_id: &'a uuid::Uuid,
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 1> {
        ProjectMemberRowQuery {
            client,
            params: [user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ProjectMemberRowBorrowed, tokio_postgres::Error> {
                Ok(ProjectMemberRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    user_id: row.try_get(2)?,
                    role: row.try_get(3)?,
                    created_at: row.try_get(4)?,
                })
            },
            mapper: |it| ProjectMemberRow::from(it),
        }
    }
}
pub struct UpdateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update() -> UpdateStmt {
    UpdateStmt(
        "UPDATE project_members SET role =$1 WHERE id =$2 RETURNING *",
        None,
    )
}
impl UpdateStmt {
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
        role: &'a T1,
        id: &'a uuid::Uuid,
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 2> {
        ProjectMemberRowQuery {
            client,
            params: [role, id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ProjectMemberRowBorrowed, tokio_postgres::Error> {
                Ok(ProjectMemberRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    user_id: row.try_get(2)?,
                    role: row.try_get(3)?,
                    created_at: row.try_get(4)?,
                })
            },
            mapper: |it| ProjectMemberRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpdateParams<T1>,
        ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 2>,
        C,
    > for UpdateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpdateParams<T1>,
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 2> {
        self.bind(client, &params.role, &params.id)
    }
}
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("DELETE FROM project_members WHERE id =$1", None)
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
pub struct DeleteByProjectAndUserStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_by_project_and_user() -> DeleteByProjectAndUserStmt {
    DeleteByProjectAndUserStmt(
        "DELETE FROM project_members WHERE project_id =$1 AND user_id =$2",
        None,
    )
}
impl DeleteByProjectAndUserStmt {
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
        user_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[project_id, user_id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        DeleteByProjectAndUserParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for DeleteByProjectAndUserStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a DeleteByProjectAndUserParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.project_id, &params.user_id))
    }
}
pub struct UpsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert() -> UpsertStmt {
    UpsertStmt(
        "INSERT INTO project_members (project_id, user_id, role) VALUES ($1,$2,$3) ON CONFLICT (project_id, user_id) DO UPDATE SET role = EXCLUDED.role RETURNING *",
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
        user_id: &'a uuid::Uuid,
        role: &'a T1,
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 3> {
        ProjectMemberRowQuery {
            client,
            params: [project_id, user_id, role],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<ProjectMemberRowBorrowed, tokio_postgres::Error> {
                Ok(ProjectMemberRowBorrowed {
                    id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    user_id: row.try_get(2)?,
                    role: row.try_get(3)?,
                    created_at: row.try_get(4)?,
                })
            },
            mapper: |it| ProjectMemberRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpsertParams<T1>,
        ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 3>,
        C,
    > for UpsertStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpsertParams<T1>,
    ) -> ProjectMemberRowQuery<'c, 'a, 's, C, ProjectMemberRow, 3> {
        self.bind(client, &params.project_id, &params.user_id, &params.role)
    }
}
pub struct SyncDeleteRemovedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn sync_delete_removed() -> SyncDeleteRemovedStmt {
    SyncDeleteRemovedStmt(
        "DELETE FROM project_members WHERE project_id =$1 AND user_id != ALL ($2::uuid[])",
        None,
    )
}
impl SyncDeleteRemovedStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = uuid::Uuid>>(
        &'s self,
        client: &'c C,
        project_id: &'a uuid::Uuid,
        user_ids: &'a T1,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[project_id, user_ids]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::ArraySql<Item = uuid::Uuid>>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SyncDeleteRemovedParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SyncDeleteRemovedStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SyncDeleteRemovedParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.project_id, &params.user_ids))
    }
}
