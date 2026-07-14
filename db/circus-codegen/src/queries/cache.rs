// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct HasCircusBuildProductParams<T1: crate::StringSql> {
    pub store_path: T1,
    pub project_id: Option<uuid::Uuid>,
}
#[derive(Debug)]
pub struct SignedNarinfoSigParams<T1: crate::StringSql> {
    pub store_path: T1,
    pub project_id: Option<uuid::Uuid>,
}
#[derive(Debug)]
pub struct HasCircusSignedBuildProductParams<T1: crate::StringSql> {
    pub store_path: T1,
    pub project_id: Option<uuid::Uuid>,
}
#[derive(Debug)]
pub struct HasCircusDerivationPathParams<T1: crate::StringSql> {
    pub store_path: T1,
    pub project_id: Option<uuid::Uuid>,
}
#[derive(Debug)]
pub struct HasCircusDerivationPathAnyParams<T1: crate::StringSql, T2: crate::ArraySql<Item = T1>> {
    pub drv_paths: T2,
    pub project_id: Option<uuid::Uuid>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SignedNarinfoSig {
    pub nar_hash: String,
    pub nar_size: i64,
    pub references: Vec<String>,
    pub sig: String,
}
pub struct SignedNarinfoSigBorrowed<'a> {
    pub nar_hash: &'a str,
    pub nar_size: i64,
    pub references: crate::ArrayIterator<'a, &'a str>,
    pub sig: &'a str,
}
impl<'a> From<SignedNarinfoSigBorrowed<'a>> for SignedNarinfoSig {
    fn from(
        SignedNarinfoSigBorrowed {
            nar_hash,
            nar_size,
            references,
            sig,
        }: SignedNarinfoSigBorrowed<'a>,
    ) -> Self {
        Self {
            nar_hash: nar_hash.into(),
            nar_size,
            references: references.map(|v| v.into()).collect(),
            sig: sig.into(),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct BoolQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<bool, tokio_postgres::Error>,
    mapper: fn(bool) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BoolQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(bool) -> R) -> BoolQuery<'c, 'a, 's, C, R, N> {
        BoolQuery {
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
pub struct SignedNarinfoSigQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<SignedNarinfoSigBorrowed, tokio_postgres::Error>,
    mapper: fn(SignedNarinfoSigBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> SignedNarinfoSigQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(SignedNarinfoSigBorrowed) -> R,
    ) -> SignedNarinfoSigQuery<'c, 'a, 's, C, R, N> {
        SignedNarinfoSigQuery {
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
pub struct HasCircusBuildProductStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn has_circus_build_product() -> HasCircusBuildProductStmt {
    HasCircusBuildProductStmt(
        "SELECT EXISTS( SELECT 1 FROM build_products bp JOIN builds b ON b.id = bp.build_id JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE bp.path = $1 AND ( $2::uuid IS NULL OR j.project_id = $2 ) UNION ALL SELECT 1 FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.build_output_path = $1 AND ( $2::uuid IS NULL OR j.project_id = $2 ) )",
        None,
    )
}
impl HasCircusBuildProductStmt {
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
        store_path: &'a T1,
        project_id: &'a Option<uuid::Uuid>,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        BoolQuery {
            client,
            params: [store_path, project_id],
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
        HasCircusBuildProductParams<T1>,
        BoolQuery<'c, 'a, 's, C, bool, 2>,
        C,
    > for HasCircusBuildProductStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a HasCircusBuildProductParams<T1>,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        self.bind(client, &params.store_path, &params.project_id)
    }
}
pub struct SignedNarinfoSigStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn signed_narinfo_sig() -> SignedNarinfoSigStmt {
    SignedNarinfoSigStmt(
        "SELECT nar_hash, nar_size, \"references\", sig FROM narinfo_cache n WHERE n.store_path = $1 AND n.sig IS NOT NULL AND btrim(n.sig) != '' AND ( $2::uuid IS NULL OR n.project_id = $2 OR EXISTS ( SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND ncp.project_id = $2 ) )",
        None,
    )
}
impl SignedNarinfoSigStmt {
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
        store_path: &'a T1,
        project_id: &'a Option<uuid::Uuid>,
    ) -> SignedNarinfoSigQuery<'c, 'a, 's, C, SignedNarinfoSig, 2> {
        SignedNarinfoSigQuery {
            client,
            params: [store_path, project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<SignedNarinfoSigBorrowed, tokio_postgres::Error> {
                Ok(SignedNarinfoSigBorrowed {
                    nar_hash: row.try_get(0)?,
                    nar_size: row.try_get(1)?,
                    references: row.try_get(2)?,
                    sig: row.try_get(3)?,
                })
            },
            mapper: |it| SignedNarinfoSig::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        SignedNarinfoSigParams<T1>,
        SignedNarinfoSigQuery<'c, 'a, 's, C, SignedNarinfoSig, 2>,
        C,
    > for SignedNarinfoSigStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a SignedNarinfoSigParams<T1>,
    ) -> SignedNarinfoSigQuery<'c, 'a, 's, C, SignedNarinfoSig, 2> {
        self.bind(client, &params.store_path, &params.project_id)
    }
}
pub struct HasCircusSignedBuildProductStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn has_circus_signed_build_product() -> HasCircusSignedBuildProductStmt {
    HasCircusSignedBuildProductStmt(
        "SELECT EXISTS( SELECT 1 FROM build_products bp JOIN builds b ON b.id = bp.build_id JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE bp.path = $1 AND b.signed = true AND ( $2::uuid IS NULL OR j.project_id = $2 ) UNION ALL SELECT 1 FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.build_output_path = $1 AND b.signed = true AND ( $2::uuid IS NULL OR j.project_id = $2 ) )",
        None,
    )
}
impl HasCircusSignedBuildProductStmt {
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
        store_path: &'a T1,
        project_id: &'a Option<uuid::Uuid>,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        BoolQuery {
            client,
            params: [store_path, project_id],
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
        HasCircusSignedBuildProductParams<T1>,
        BoolQuery<'c, 'a, 's, C, bool, 2>,
        C,
    > for HasCircusSignedBuildProductStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a HasCircusSignedBuildProductParams<T1>,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        self.bind(client, &params.store_path, &params.project_id)
    }
}
pub struct HasCircusDerivationPathStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn has_circus_derivation_path() -> HasCircusDerivationPathStmt {
    HasCircusDerivationPathStmt(
        "SELECT EXISTS( SELECT 1 FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.drv_path = $1 AND ( $2::uuid IS NULL OR j.project_id = $2 ) )",
        None,
    )
}
impl HasCircusDerivationPathStmt {
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
        store_path: &'a T1,
        project_id: &'a Option<uuid::Uuid>,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        BoolQuery {
            client,
            params: [store_path, project_id],
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
        HasCircusDerivationPathParams<T1>,
        BoolQuery<'c, 'a, 's, C, bool, 2>,
        C,
    > for HasCircusDerivationPathStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a HasCircusDerivationPathParams<T1>,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        self.bind(client, &params.store_path, &params.project_id)
    }
}
pub struct HasCircusDerivationPathAnyStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn has_circus_derivation_path_any() -> HasCircusDerivationPathAnyStmt {
    HasCircusDerivationPathAnyStmt(
        "SELECT EXISTS( SELECT 1 FROM builds b JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.drv_path = ANY($1) AND ( $2::uuid IS NULL OR j.project_id = $2 ) )",
        None,
    )
}
impl HasCircusDerivationPathAnyStmt {
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
        T2: crate::ArraySql<Item = T1>,
    >(
        &'s self,
        client: &'c C,
        drv_paths: &'a T2,
        project_id: &'a Option<uuid::Uuid>,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        BoolQuery {
            client,
            params: [drv_paths, project_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::ArraySql<Item = T1>>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        HasCircusDerivationPathAnyParams<T1, T2>,
        BoolQuery<'c, 'a, 's, C, bool, 2>,
        C,
    > for HasCircusDerivationPathAnyStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a HasCircusDerivationPathAnyParams<T1, T2>,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        self.bind(client, &params.drv_paths, &params.project_id)
    }
}
