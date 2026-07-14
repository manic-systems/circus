// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<T1: crate::StringSql> {
    pub build_id: uuid::Uuid,
    pub step_number: i32,
    pub command: T1,
}
#[derive(Debug)]
pub struct CompleteParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub exit_code: i32,
    pub output: Option<T1>,
    pub error_output: Option<T2>,
    pub id: uuid::Uuid,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BuildStepRow {
    pub id: uuid::Uuid,
    pub build_id: uuid::Uuid,
    pub step_number: i32,
    pub command: String,
    pub output: Option<String>,
    pub error_output: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub exit_code: Option<i32>,
}
pub struct BuildStepRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub build_id: uuid::Uuid,
    pub step_number: i32,
    pub command: &'a str,
    pub output: Option<&'a str>,
    pub error_output: Option<&'a str>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub exit_code: Option<i32>,
}
impl<'a> From<BuildStepRowBorrowed<'a>> for BuildStepRow {
    fn from(
        BuildStepRowBorrowed {
            id,
            build_id,
            step_number,
            command,
            output,
            error_output,
            started_at,
            completed_at,
            exit_code,
        }: BuildStepRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            build_id,
            step_number,
            command: command.into(),
            output: output.map(|v| v.into()),
            error_output: error_output.map(|v| v.into()),
            started_at,
            completed_at,
            exit_code,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct BuildStepRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<BuildStepRowBorrowed, tokio_postgres::Error>,
    mapper: fn(BuildStepRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BuildStepRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(BuildStepRowBorrowed) -> R,
    ) -> BuildStepRowQuery<'c, 'a, 's, C, R, N> {
        BuildStepRowQuery {
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
        "INSERT INTO build_steps (build_id, step_number, command) VALUES ($1,$2,$3) RETURNING *",
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
        build_id: &'a uuid::Uuid,
        step_number: &'a i32,
        command: &'a T1,
    ) -> BuildStepRowQuery<'c, 'a, 's, C, BuildStepRow, 3> {
        BuildStepRowQuery {
            client,
            params: [build_id, step_number, command],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildStepRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildStepRowBorrowed {
                        id: row.try_get(0)?,
                        build_id: row.try_get(1)?,
                        step_number: row.try_get(2)?,
                        command: row.try_get(3)?,
                        output: row.try_get(4)?,
                        error_output: row.try_get(5)?,
                        started_at: row.try_get(6)?,
                        completed_at: row.try_get(7)?,
                        exit_code: row.try_get(8)?,
                    })
                },
            mapper: |it| BuildStepRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1>,
        BuildStepRowQuery<'c, 'a, 's, C, BuildStepRow, 3>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1>,
    ) -> BuildStepRowQuery<'c, 'a, 's, C, BuildStepRow, 3> {
        self.bind(
            client,
            &params.build_id,
            &params.step_number,
            &params.command,
        )
    }
}
pub struct CompleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn complete() -> CompleteStmt {
    CompleteStmt(
        "UPDATE build_steps SET completed_at = NOW(), exit_code =$1, output =$2, error_output =$3 WHERE id =$4 RETURNING *",
        None,
    )
}
impl CompleteStmt {
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
        exit_code: &'a i32,
        output: &'a Option<T1>,
        error_output: &'a Option<T2>,
        id: &'a uuid::Uuid,
    ) -> BuildStepRowQuery<'c, 'a, 's, C, BuildStepRow, 4> {
        BuildStepRowQuery {
            client,
            params: [exit_code, output, error_output, id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildStepRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildStepRowBorrowed {
                        id: row.try_get(0)?,
                        build_id: row.try_get(1)?,
                        step_number: row.try_get(2)?,
                        command: row.try_get(3)?,
                        output: row.try_get(4)?,
                        error_output: row.try_get(5)?,
                        started_at: row.try_get(6)?,
                        completed_at: row.try_get(7)?,
                        exit_code: row.try_get(8)?,
                    })
                },
            mapper: |it| BuildStepRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CompleteParams<T1, T2>,
        BuildStepRowQuery<'c, 'a, 's, C, BuildStepRow, 4>,
        C,
    > for CompleteStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CompleteParams<T1, T2>,
    ) -> BuildStepRowQuery<'c, 'a, 's, C, BuildStepRow, 4> {
        self.bind(
            client,
            &params.exit_code,
            &params.output,
            &params.error_output,
            &params.id,
        )
    }
}
pub struct ListForBuildStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_build() -> ListForBuildStmt {
    ListForBuildStmt(
        "SELECT * FROM build_steps WHERE build_id =$1 ORDER BY step_number ASC",
        None,
    )
}
impl ListForBuildStmt {
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
        build_id: &'a uuid::Uuid,
    ) -> BuildStepRowQuery<'c, 'a, 's, C, BuildStepRow, 1> {
        BuildStepRowQuery {
            client,
            params: [build_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<BuildStepRowBorrowed, tokio_postgres::Error> {
                    Ok(BuildStepRowBorrowed {
                        id: row.try_get(0)?,
                        build_id: row.try_get(1)?,
                        step_number: row.try_get(2)?,
                        command: row.try_get(3)?,
                        output: row.try_get(4)?,
                        error_output: row.try_get(5)?,
                        started_at: row.try_get(6)?,
                        completed_at: row.try_get(7)?,
                        exit_code: row.try_get(8)?,
                    })
                },
            mapper: |it| BuildStepRow::from(it),
        }
    }
}
pub struct DeleteForBuildStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete_for_build() -> DeleteForBuildStmt {
    DeleteForBuildStmt("DELETE FROM build_steps WHERE build_id =$1", None)
}
impl DeleteForBuildStmt {
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
        build_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[build_id]).await
    }
}
