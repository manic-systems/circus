// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateWithKindParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
    T6: crate::StringSql,
    T7: crate::StringSql,
    T8: crate::StringSql,
> {
    pub jobset_id: uuid::Uuid,
    pub commit_hash: T1,
    pub status: T2,
    pub trigger_kind: T3,
    pub pr_number: Option<i32>,
    pub pr_head_branch: Option<T4>,
    pub pr_base_branch: Option<T5>,
    pub pr_action: Option<T6>,
    pub source_scope: Option<T7>,
    pub source_base_commit: Option<T8>,
}
#[derive(Clone, Copy, Debug)]
pub struct GetVisibleParams {
    pub id: uuid::Uuid,
    pub include_hidden: bool,
}
#[derive(Debug)]
pub struct ListFilteredWithVisibilityParams<T1: crate::StringSql> {
    pub jobset_id: Option<uuid::Uuid>,
    pub status: Option<T1>,
    pub include_hidden: bool,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct CountFilteredWithVisibilityParams<T1: crate::StringSql> {
    pub jobset_id: Option<uuid::Uuid>,
    pub status: Option<T1>,
    pub include_hidden: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct SetHiddenParams {
    pub hidden: bool,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpdateStatusParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub status: T1,
    pub error_message: Option<T2>,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct SetInputsHashParams<T1: crate::StringSql> {
    pub inputs_hash: T1,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct GetByInputsHashParams<T1: crate::StringSql> {
    pub jobset_id: uuid::Uuid,
    pub inputs_hash: T1,
}
#[derive(Debug)]
pub struct GetSourceHeadParams<T1: crate::StringSql> {
    pub jobset_id: uuid::Uuid,
    pub source_scope: T1,
}
#[derive(Debug)]
pub struct SetSourceHeadParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub jobset_id: uuid::Uuid,
    pub source_scope: T1,
    pub commit_hash: T2,
}
#[derive(Debug)]
pub struct GetByJobsetAndCommitParams<T1: crate::StringSql> {
    pub jobset_id: uuid::Uuid,
    pub commit_hash: T1,
}
#[derive(Debug)]
pub struct FinishRunningParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub status: T1,
    pub error_message: Option<T2>,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct SupersedeSourceEvaluationsParams<T1: crate::StringSql> {
    pub superseded_by: uuid::Uuid,
    pub jobset_id: uuid::Uuid,
    pub source_scope: Option<T1>,
}
#[derive(Debug)]
pub struct CancelSupersededBuildsParams<T1: crate::StringSql> {
    pub superseded_by: uuid::Uuid,
    pub jobset_id: uuid::Uuid,
    pub source_scope: Option<T1>,
}
#[derive(Debug)]
pub struct ListPageFilteredParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
> {
    pub project: Option<T1>,
    pub jobset: Option<T2>,
    pub commit: Option<T3>,
    pub status: Option<T4>,
    pub include_hidden: bool,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Debug)]
pub struct CountPageFilteredParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
> {
    pub project: Option<T1>,
    pub jobset: Option<T2>,
    pub commit: Option<T3>,
    pub status: Option<T4>,
    pub include_hidden: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationRow {
    pub id: uuid::Uuid,
    pub jobset_id: uuid::Uuid,
    pub commit_hash: String,
    pub evaluation_time: chrono::DateTime<chrono::Utc>,
    pub status: String,
    pub error_message: Option<String>,
    pub inputs_hash: Option<String>,
    pub pr_number: Option<i32>,
    pub pr_head_branch: Option<String>,
    pub pr_base_branch: Option<String>,
    pub pr_action: Option<String>,
    pub trigger_kind: String,
    pub hidden: bool,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub orphaned_count: i32,
    pub source_scope: Option<String>,
    pub superseded_by: Option<uuid::Uuid>,
    pub source_base_commit: Option<String>,
}
pub struct EvaluationRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub jobset_id: uuid::Uuid,
    pub commit_hash: &'a str,
    pub evaluation_time: chrono::DateTime<chrono::Utc>,
    pub status: &'a str,
    pub error_message: Option<&'a str>,
    pub inputs_hash: Option<&'a str>,
    pub pr_number: Option<i32>,
    pub pr_head_branch: Option<&'a str>,
    pub pr_base_branch: Option<&'a str>,
    pub pr_action: Option<&'a str>,
    pub trigger_kind: &'a str,
    pub hidden: bool,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub orphaned_count: i32,
    pub source_scope: Option<&'a str>,
    pub superseded_by: Option<uuid::Uuid>,
    pub source_base_commit: Option<&'a str>,
}
impl<'a> From<EvaluationRowBorrowed<'a>> for EvaluationRow {
    fn from(
        EvaluationRowBorrowed {
            id,
            jobset_id,
            commit_hash,
            evaluation_time,
            status,
            error_message,
            inputs_hash,
            pr_number,
            pr_head_branch,
            pr_base_branch,
            pr_action,
            trigger_kind,
            hidden,
            started_at,
            orphaned_count,
            source_scope,
            superseded_by,
            source_base_commit,
        }: EvaluationRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            jobset_id,
            commit_hash: commit_hash.into(),
            evaluation_time,
            status: status.into(),
            error_message: error_message.map(|v| v.into()),
            inputs_hash: inputs_hash.map(|v| v.into()),
            pr_number,
            pr_head_branch: pr_head_branch.map(|v| v.into()),
            pr_base_branch: pr_base_branch.map(|v| v.into()),
            pr_action: pr_action.map(|v| v.into()),
            trigger_kind: trigger_kind.into(),
            hidden,
            started_at,
            orphaned_count,
            source_scope: source_scope.map(|v| v.into()),
            superseded_by,
            source_base_commit: source_base_commit.map(|v| v.into()),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct BuildContextRow {
    pub evaluation_id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub project_name: String,
    pub jobset_id: uuid::Uuid,
    pub jobset_name: String,
}
pub struct BuildContextRowBorrowed<'a> {
    pub evaluation_id: uuid::Uuid,
    pub project_id: uuid::Uuid,
    pub project_name: &'a str,
    pub jobset_id: uuid::Uuid,
    pub jobset_name: &'a str,
}
impl<'a> From<BuildContextRowBorrowed<'a>> for BuildContextRow {
    fn from(
        BuildContextRowBorrowed {
            evaluation_id,
            project_id,
            project_name,
            jobset_id,
            jobset_name,
        }: BuildContextRowBorrowed<'a>,
    ) -> Self {
        Self {
            evaluation_id,
            project_id,
            project_name: project_name.into(),
            jobset_id,
            jobset_name: jobset_name.into(),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct EvaluationRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<EvaluationRowBorrowed, tokio_postgres::Error>,
    mapper: fn(EvaluationRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> EvaluationRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(EvaluationRowBorrowed) -> R,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, R, N> {
        EvaluationRowQuery {
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
pub struct BuildContextRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<BuildContextRowBorrowed, tokio_postgres::Error>,
    mapper: fn(BuildContextRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BuildContextRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(BuildContextRowBorrowed) -> R,
    ) -> BuildContextRowQuery<'c, 'a, 's, C, R, N> {
        BuildContextRowQuery {
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
pub struct CreateWithKindStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn create_with_kind() -> CreateWithKindStmt {
    CreateWithKindStmt(
        "INSERT INTO evaluations ( jobset_id, commit_hash, status, trigger_kind, pr_number, pr_head_branch, pr_base_branch, pr_action, started_at, source_scope, source_base_commit ) VALUES ( $1, $2, $3, $4, $5, $6, $7, $8, CASE WHEN $3::text = 'running' THEN NOW() END, $9, $10 ) RETURNING *",
        None,
    )
}
impl CreateWithKindStmt {
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
        T6: crate::StringSql,
        T7: crate::StringSql,
        T8: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        jobset_id: &'a uuid::Uuid,
        commit_hash: &'a T1,
        status: &'a T2,
        trigger_kind: &'a T3,
        pr_number: &'a Option<i32>,
        pr_head_branch: &'a Option<T4>,
        pr_base_branch: &'a Option<T5>,
        pr_action: &'a Option<T6>,
        source_scope: &'a Option<T7>,
        source_base_commit: &'a Option<T8>,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 10> {
        EvaluationRowQuery {
            client,
            params: [
                jobset_id,
                commit_hash,
                status,
                trigger_kind,
                pr_number,
                pr_head_branch,
                pr_base_branch,
                pr_action,
                source_scope,
                source_base_commit,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
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
    T6: crate::StringSql,
    T7: crate::StringSql,
    T8: crate::StringSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateWithKindParams<T1, T2, T3, T4, T5, T6, T7, T8>,
        EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 10>,
        C,
    > for CreateWithKindStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateWithKindParams<T1, T2, T3, T4, T5, T6, T7, T8>,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 10> {
        self.bind(
            client,
            &params.jobset_id,
            &params.commit_hash,
            &params.status,
            &params.trigger_kind,
            &params.pr_number,
            &params.pr_head_branch,
            &params.pr_base_branch,
            &params.pr_action,
            &params.source_scope,
            &params.source_base_commit,
        )
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM evaluations WHERE id = $1", None)
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
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 1> {
        EvaluationRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
pub struct GetVisibleStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_visible() -> GetVisibleStmt {
    GetVisibleStmt(
        "SELECT * FROM evaluations WHERE id = $1 AND ($2::boolean OR hidden = false)",
        None,
    )
}
impl GetVisibleStmt {
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
        include_hidden: &'a bool,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2> {
        EvaluationRowQuery {
            client,
            params: [id, include_hidden],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetVisibleParams,
        EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2>,
        C,
    > for GetVisibleStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetVisibleParams,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2> {
        self.bind(client, &params.id, &params.include_hidden)
    }
}
pub struct ListForJobsetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_for_jobset() -> ListForJobsetStmt {
    ListForJobsetStmt(
        "SELECT * FROM evaluations WHERE jobset_id = $1 ORDER BY evaluation_time DESC",
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
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 1> {
        EvaluationRowQuery {
            client,
            params: [jobset_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
pub struct ListFilteredWithVisibilityStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_filtered_with_visibility() -> ListFilteredWithVisibilityStmt {
    ListFilteredWithVisibilityStmt(
        "SELECT * FROM evaluations WHERE ($1::uuid IS NULL OR jobset_id = $1) AND ($2::text IS NULL OR status = $2) AND ($3::boolean OR hidden = false) ORDER BY evaluation_time DESC LIMIT $4 OFFSET $5",
        None,
    )
}
impl ListFilteredWithVisibilityStmt {
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
        jobset_id: &'a Option<uuid::Uuid>,
        status: &'a Option<T1>,
        include_hidden: &'a bool,
        limit: &'a i64,
        offset: &'a i64,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 5> {
        EvaluationRowQuery {
            client,
            params: [jobset_id, status, include_hidden, limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListFilteredWithVisibilityParams<T1>,
        EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 5>,
        C,
    > for ListFilteredWithVisibilityStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListFilteredWithVisibilityParams<T1>,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 5> {
        self.bind(
            client,
            &params.jobset_id,
            &params.status,
            &params.include_hidden,
            &params.limit,
            &params.offset,
        )
    }
}
pub struct CountFilteredWithVisibilityStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_filtered_with_visibility() -> CountFilteredWithVisibilityStmt {
    CountFilteredWithVisibilityStmt(
        "SELECT COUNT(*) FROM evaluations WHERE ($1::uuid IS NULL OR jobset_id = $1) AND ($2::text IS NULL OR status = $2) AND ($3::boolean OR hidden = false)",
        None,
    )
}
impl CountFilteredWithVisibilityStmt {
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
        jobset_id: &'a Option<uuid::Uuid>,
        status: &'a Option<T1>,
        include_hidden: &'a bool,
    ) -> I64Query<'c, 'a, 's, C, i64, 3> {
        I64Query {
            client,
            params: [jobset_id, status, include_hidden],
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
        CountFilteredWithVisibilityParams<T1>,
        I64Query<'c, 'a, 's, C, i64, 3>,
        C,
    > for CountFilteredWithVisibilityStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CountFilteredWithVisibilityParams<T1>,
    ) -> I64Query<'c, 'a, 's, C, i64, 3> {
        self.bind(
            client,
            &params.jobset_id,
            &params.status,
            &params.include_hidden,
        )
    }
}
pub struct SetHiddenStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_hidden() -> SetHiddenStmt {
    SetHiddenStmt(
        "UPDATE evaluations SET hidden = $1 WHERE id = $2 RETURNING *",
        None,
    )
}
impl SetHiddenStmt {
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
        hidden: &'a bool,
        id: &'a uuid::Uuid,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2> {
        EvaluationRowQuery {
            client,
            params: [hidden, id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        SetHiddenParams,
        EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2>,
        C,
    > for SetHiddenStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a SetHiddenParams,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2> {
        self.bind(client, &params.hidden, &params.id)
    }
}
pub struct TryClaimPendingStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn try_claim_pending() -> TryClaimPendingStmt {
    TryClaimPendingStmt(
        "UPDATE evaluations SET status = 'running', started_at = NOW() WHERE id = $1 AND status = 'pending' RETURNING *",
        None,
    )
}
impl TryClaimPendingStmt {
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
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 1> {
        EvaluationRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
pub struct UpdateStatusStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update_status() -> UpdateStatusStmt {
    UpdateStatusStmt(
        "UPDATE evaluations SET status = $1, error_message = $2 WHERE id = $3 RETURNING *",
        None,
    )
}
impl UpdateStatusStmt {
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
        status: &'a T1,
        error_message: &'a Option<T2>,
        id: &'a uuid::Uuid,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 3> {
        EvaluationRowQuery {
            client,
            params: [status, error_message, id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpdateStatusParams<T1, T2>,
        EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 3>,
        C,
    > for UpdateStatusStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpdateStatusParams<T1, T2>,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 3> {
        self.bind(client, &params.status, &params.error_message, &params.id)
    }
}
pub struct GetLatestStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_latest() -> GetLatestStmt {
    GetLatestStmt(
        "SELECT * FROM evaluations WHERE jobset_id = $1 AND status = 'completed' ORDER BY evaluation_time DESC LIMIT 1",
        None,
    )
}
impl GetLatestStmt {
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
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 1> {
        EvaluationRowQuery {
            client,
            params: [jobset_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
pub struct SetInputsHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_inputs_hash() -> SetInputsHashStmt {
    SetInputsHashStmt(
        "UPDATE evaluations SET inputs_hash = $1 WHERE id = $2",
        None,
    )
}
impl SetInputsHashStmt {
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
        inputs_hash: &'a T1,
        id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[inputs_hash, id]).await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetInputsHashParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetInputsHashStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SetInputsHashParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(client, &params.inputs_hash, &params.id))
    }
}
pub struct GetByInputsHashStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_by_inputs_hash() -> GetByInputsHashStmt {
    GetByInputsHashStmt(
        "SELECT * FROM evaluations WHERE jobset_id = $1 AND inputs_hash = $2 AND status = 'completed' ORDER BY evaluation_time DESC LIMIT 1",
        None,
    )
}
impl GetByInputsHashStmt {
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
        jobset_id: &'a uuid::Uuid,
        inputs_hash: &'a T1,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2> {
        EvaluationRowQuery {
            client,
            params: [jobset_id, inputs_hash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetByInputsHashParams<T1>,
        EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2>,
        C,
    > for GetByInputsHashStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetByInputsHashParams<T1>,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2> {
        self.bind(client, &params.jobset_id, &params.inputs_hash)
    }
}
pub struct CountStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count() -> CountStmt {
    CountStmt("SELECT COUNT(*) FROM evaluations", None)
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
pub struct ListPendingStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_pending() -> ListPendingStmt {
    ListPendingStmt(
        "SELECT * FROM evaluations WHERE status = 'pending' ORDER BY evaluation_time ASC",
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
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 0> {
        EvaluationRowQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
pub struct ListJobsetsWithPendingStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_jobsets_with_pending() -> ListJobsetsWithPendingStmt {
    ListJobsetsWithPendingStmt(
        "SELECT DISTINCT jobset_id FROM evaluations WHERE status = 'pending'",
        None,
    )
}
impl ListJobsetsWithPendingStmt {
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
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 0> {
        UuidUuidQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct GetSourceHeadStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_source_head() -> GetSourceHeadStmt {
    GetSourceHeadStmt(
        "SELECT commit_hash FROM evaluation_source_heads WHERE jobset_id = $1 AND source_scope = $2",
        None,
    )
}
impl GetSourceHeadStmt {
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
        jobset_id: &'a uuid::Uuid,
        source_scope: &'a T1,
    ) -> StringQuery<'c, 'a, 's, C, String, 2> {
        StringQuery {
            client,
            params: [jobset_id, source_scope],
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
        GetSourceHeadParams<T1>,
        StringQuery<'c, 'a, 's, C, String, 2>,
        C,
    > for GetSourceHeadStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetSourceHeadParams<T1>,
    ) -> StringQuery<'c, 'a, 's, C, String, 2> {
        self.bind(client, &params.jobset_id, &params.source_scope)
    }
}
pub struct SetSourceHeadStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn set_source_head() -> SetSourceHeadStmt {
    SetSourceHeadStmt(
        "INSERT INTO evaluation_source_heads (jobset_id, source_scope, commit_hash) VALUES ($1, $2, $3) ON CONFLICT (jobset_id, source_scope) DO UPDATE SET commit_hash = EXCLUDED.commit_hash",
        None,
    )
}
impl SetSourceHeadStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub async fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c C,
        jobset_id: &'a uuid::Uuid,
        source_scope: &'a T1,
        commit_hash: &'a T2,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[jobset_id, source_scope, commit_hash])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SetSourceHeadParams<T1, T2>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SetSourceHeadStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SetSourceHeadParams<T1, T2>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.jobset_id,
            &params.source_scope,
            &params.commit_hash,
        ))
    }
}
pub struct GetByJobsetAndCommitStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_by_jobset_and_commit() -> GetByJobsetAndCommitStmt {
    GetByJobsetAndCommitStmt(
        "SELECT * FROM evaluations WHERE jobset_id = $1 AND commit_hash = $2 ORDER BY (trigger_kind = 'interval') ASC, evaluation_time DESC LIMIT 1",
        None,
    )
}
impl GetByJobsetAndCommitStmt {
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
        jobset_id: &'a uuid::Uuid,
        commit_hash: &'a T1,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2> {
        EvaluationRowQuery {
            client,
            params: [jobset_id, commit_hash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        GetByJobsetAndCommitParams<T1>,
        EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2>,
        C,
    > for GetByJobsetAndCommitStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a GetByJobsetAndCommitParams<T1>,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 2> {
        self.bind(client, &params.jobset_id, &params.commit_hash)
    }
}
pub struct GetBuildContextsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get_build_contexts() -> GetBuildContextsStmt {
    GetBuildContextsStmt(
        "SELECT e.id AS evaluation_id, p.id AS project_id, p.name AS project_name, j.id AS jobset_id, j.name AS jobset_name FROM evaluations e JOIN jobsets j ON e.jobset_id = j.id JOIN projects p ON j.project_id = p.id WHERE e.id = ANY($1)",
        None,
    )
}
impl GetBuildContextsStmt {
    pub async fn prepare<'a, C: GenericClient>(
        mut self,
        client: &'a C,
    ) -> Result<Self, tokio_postgres::Error> {
        self.1 = Some(client.prepare(self.0).await?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = uuid::Uuid>>(
        &'s self,
        client: &'c C,
        evaluation_ids: &'a T1,
    ) -> BuildContextRowQuery<'c, 'a, 's, C, BuildContextRow, 1> {
        BuildContextRowQuery {
            client,
            params: [evaluation_ids],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuildContextRowBorrowed, tokio_postgres::Error> {
                Ok(BuildContextRowBorrowed {
                    evaluation_id: row.try_get(0)?,
                    project_id: row.try_get(1)?,
                    project_name: row.try_get(2)?,
                    jobset_id: row.try_get(3)?,
                    jobset_name: row.try_get(4)?,
                })
            },
            mapper: |it| BuildContextRow::from(it),
        }
    }
}
pub struct FinishRunningStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn finish_running() -> FinishRunningStmt {
    FinishRunningStmt(
        "UPDATE evaluations SET status = $1, error_message = $2 WHERE id = $3 AND status = 'running' RETURNING *",
        None,
    )
}
impl FinishRunningStmt {
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
        status: &'a T1,
        error_message: &'a Option<T2>,
        id: &'a uuid::Uuid,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 3> {
        EvaluationRowQuery {
            client,
            params: [status, error_message, id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        FinishRunningParams<T1, T2>,
        EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 3>,
        C,
    > for FinishRunningStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a FinishRunningParams<T1, T2>,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 3> {
        self.bind(client, &params.status, &params.error_message, &params.id)
    }
}
pub struct DiscardFilteredRunningStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn discard_filtered_running() -> DiscardFilteredRunningStmt {
    DiscardFilteredRunningStmt(
        "DELETE FROM evaluations WHERE id = $1 AND status = 'running' AND NOT EXISTS ( SELECT 1 FROM builds WHERE evaluation_id = evaluations.id )",
        None,
    )
}
impl DiscardFilteredRunningStmt {
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
pub struct CancelStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn cancel() -> CancelStmt {
    CancelStmt(
        "UPDATE evaluations SET status = 'cancelled', error_message = NULL WHERE id = $1 AND status IN ('pending', 'running') RETURNING *",
        None,
    )
}
impl CancelStmt {
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
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 1> {
        EvaluationRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
pub struct SweepOrphanedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn sweep_orphaned() -> SweepOrphanedStmt {
    SweepOrphanedStmt(
        "UPDATE evaluations SET status = CASE WHEN orphaned_count >= 2 THEN 'failed' ELSE 'pending' END, error_message = CASE WHEN orphaned_count >= 2 THEN 'evaluation orphaned repeatedly, giving up' END, orphaned_count = orphaned_count + 1, started_at = NULL, inputs_hash = NULL, evaluation_time = NOW() WHERE status = 'running' AND COALESCE(started_at, evaluation_time) < NOW() - make_interval(secs => $1) RETURNING *",
        None,
    )
}
impl SweepOrphanedStmt {
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
        deadline_secs: &'a f64,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 1> {
        EvaluationRowQuery {
            client,
            params: [deadline_secs],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
pub struct SupersedeSourceEvaluationsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn supersede_source_evaluations() -> SupersedeSourceEvaluationsStmt {
    SupersedeSourceEvaluationsStmt(
        "UPDATE evaluations SET status = CASE WHEN status IN ('pending', 'running') THEN 'cancelled' ELSE status END, error_message = CASE WHEN status IN ('pending', 'running') THEN 'superseded by evaluation ' || $1::text ELSE error_message END, superseded_by = $1 WHERE jobset_id = $2 AND id <> $1 AND trigger_kind = 'source_change' AND source_scope = $3 AND (status IN ('pending', 'running') OR EXISTS ( SELECT 1 FROM builds b WHERE b.evaluation_id = evaluations.id AND b.status IN ('pending', 'running') ))",
        None,
    )
}
impl SupersedeSourceEvaluationsStmt {
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
        superseded_by: &'a uuid::Uuid,
        jobset_id: &'a uuid::Uuid,
        source_scope: &'a Option<T1>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[superseded_by, jobset_id, source_scope])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        SupersedeSourceEvaluationsParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for SupersedeSourceEvaluationsStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a SupersedeSourceEvaluationsParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.superseded_by,
            &params.jobset_id,
            &params.source_scope,
        ))
    }
}
pub struct CancelSupersededBuildsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn cancel_superseded_builds() -> CancelSupersededBuildsStmt {
    CancelSupersededBuildsStmt(
        "UPDATE builds b SET status = 'cancelled', completed_at = NOW(), error_message = 'superseded by evaluation ' || $1::text FROM evaluations e WHERE b.evaluation_id = e.id AND e.jobset_id = $2 AND e.id <> $1 AND e.trigger_kind = 'source_change' AND e.source_scope = $3 AND b.status IN ('pending', 'running')",
        None,
    )
}
impl CancelSupersededBuildsStmt {
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
        superseded_by: &'a uuid::Uuid,
        jobset_id: &'a uuid::Uuid,
        source_scope: &'a Option<T1>,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(self.0, &[superseded_by, jobset_id, source_scope])
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync, T1: crate::StringSql>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        CancelSupersededBuildsParams<T1>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for CancelSupersededBuildsStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a CancelSupersededBuildsParams<T1>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.superseded_by,
            &params.jobset_id,
            &params.source_scope,
        ))
    }
}
pub struct RestartRequeueStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn restart_requeue() -> RestartRequeueStmt {
    RestartRequeueStmt(
        "UPDATE evaluations e SET status = 'pending', evaluation_time = NOW(), error_message = NULL, inputs_hash = NULL, started_at = NULL, orphaned_count = 0, superseded_by = NULL, trigger_kind = CASE WHEN e.trigger_kind = 'source_change' THEN 'manual' ELSE e.trigger_kind END, source_scope = NULL, source_base_commit = NULL FROM jobsets j WHERE e.id = $1 AND e.jobset_id = j.id AND e.status IN ('cancelled', 'failed', 'timed_out') AND (j.state = 'one_shot' OR (j.enabled AND j.state IN ('enabled', 'one_at_a_time'))) RETURNING e.*",
        None,
    )
}
impl RestartRequeueStmt {
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
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 1> {
        EvaluationRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
        }
    }
}
pub struct RestartDeleteBuildsStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn restart_delete_builds() -> RestartDeleteBuildsStmt {
    RestartDeleteBuildsStmt("DELETE FROM builds WHERE evaluation_id = $1", None)
}
impl RestartDeleteBuildsStmt {
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
pub struct RestartReenableOneShotStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn restart_reenable_one_shot() -> RestartReenableOneShotStmt {
    RestartReenableOneShotStmt(
        "UPDATE jobsets SET enabled = true WHERE id = (SELECT jobset_id FROM evaluations WHERE id = $1) AND state = 'one_shot'",
        None,
    )
}
impl RestartReenableOneShotStmt {
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
pub struct LockRunningStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn lock_running() -> LockRunningStmt {
    LockRunningStmt(
        "SELECT id FROM evaluations WHERE id = $1 AND status = 'running' FOR UPDATE",
        None,
    )
}
impl LockRunningStmt {
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
    ) -> UuidUuidQuery<'c, 'a, 's, C, uuid::Uuid, 1> {
        UuidUuidQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct StatusOfStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn status_of() -> StatusOfStmt {
    StatusOfStmt("SELECT status FROM evaluations WHERE id = $1", None)
}
impl StatusOfStmt {
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
    ) -> StringQuery<'c, 'a, 's, C, String, 1> {
        StringQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
pub struct ListPageFilteredStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_page_filtered() -> ListPageFilteredStmt {
    ListPageFilteredStmt(
        "SELECT e.* FROM evaluations e JOIN jobsets j ON j.id = e.jobset_id JOIN projects p ON p.id = j.project_id WHERE ($1::text IS NULL OR p.name ILIKE '%' || $1 || '%') AND ($2::text IS NULL OR j.name ILIKE '%' || $2 || '%') AND ($3::text IS NULL OR e.commit_hash ILIKE $3 || '%') AND ($4::text IS NULL OR e.status = $4) AND ($5 OR e.hidden = false) ORDER BY e.evaluation_time DESC LIMIT $6 OFFSET $7",
        None,
    )
}
impl ListPageFilteredStmt {
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
    >(
        &'s self,
        client: &'c C,
        project: &'a Option<T1>,
        jobset: &'a Option<T2>,
        commit: &'a Option<T3>,
        status: &'a Option<T4>,
        include_hidden: &'a bool,
        limit: &'a i64,
        offset: &'a i64,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 7> {
        EvaluationRowQuery {
            client,
            params: [
                project,
                jobset,
                commit,
                status,
                include_hidden,
                limit,
                offset,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor:
                |row: &tokio_postgres::Row| -> Result<EvaluationRowBorrowed, tokio_postgres::Error> {
                    Ok(EvaluationRowBorrowed {
                        id: row.try_get(0)?,
                        jobset_id: row.try_get(1)?,
                        commit_hash: row.try_get(2)?,
                        evaluation_time: row.try_get(3)?,
                        status: row.try_get(4)?,
                        error_message: row.try_get(5)?,
                        inputs_hash: row.try_get(6)?,
                        pr_number: row.try_get(7)?,
                        pr_head_branch: row.try_get(8)?,
                        pr_base_branch: row.try_get(9)?,
                        pr_action: row.try_get(10)?,
                        trigger_kind: row.try_get(11)?,
                        hidden: row.try_get(12)?,
                        started_at: row.try_get(13)?,
                        orphaned_count: row.try_get(14)?,
                        source_scope: row.try_get(15)?,
                        superseded_by: row.try_get(16)?,
                        source_base_commit: row.try_get(17)?,
                    })
                },
            mapper: |it| EvaluationRow::from(it),
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
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        ListPageFilteredParams<T1, T2, T3, T4>,
        EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 7>,
        C,
    > for ListPageFilteredStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a ListPageFilteredParams<T1, T2, T3, T4>,
    ) -> EvaluationRowQuery<'c, 'a, 's, C, EvaluationRow, 7> {
        self.bind(
            client,
            &params.project,
            &params.jobset,
            &params.commit,
            &params.status,
            &params.include_hidden,
            &params.limit,
            &params.offset,
        )
    }
}
pub struct CountPageFilteredStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count_page_filtered() -> CountPageFilteredStmt {
    CountPageFilteredStmt(
        "SELECT COUNT(*) FROM evaluations e JOIN jobsets j ON j.id = e.jobset_id JOIN projects p ON p.id = j.project_id WHERE ($1::text IS NULL OR p.name ILIKE '%' || $1 || '%') AND ($2::text IS NULL OR j.name ILIKE '%' || $2 || '%') AND ($3::text IS NULL OR e.commit_hash ILIKE $3 || '%') AND ($4::text IS NULL OR e.status = $4) AND ($5 OR e.hidden = false)",
        None,
    )
}
impl CountPageFilteredStmt {
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
    >(
        &'s self,
        client: &'c C,
        project: &'a Option<T1>,
        jobset: &'a Option<T2>,
        commit: &'a Option<T3>,
        status: &'a Option<T4>,
        include_hidden: &'a bool,
    ) -> I64Query<'c, 'a, 's, C, i64, 5> {
        I64Query {
            client,
            params: [project, jobset, commit, status, include_hidden],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
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
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CountPageFilteredParams<T1, T2, T3, T4>,
        I64Query<'c, 'a, 's, C, i64, 5>,
        C,
    > for CountPageFilteredStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CountPageFilteredParams<T1, T2, T3, T4>,
    ) -> I64Query<'c, 'a, 's, C, i64, 5> {
        self.bind(
            client,
            &params.project,
            &params.jobset,
            &params.commit,
            &params.status,
            &params.include_hidden,
        )
    }
}
