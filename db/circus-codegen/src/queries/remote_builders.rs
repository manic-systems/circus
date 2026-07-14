// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct CreateParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
    T5: crate::StringSql,
    T6: crate::ArraySql<Item = T5>,
    T7: crate::StringSql,
    T8: crate::ArraySql<Item = T7>,
    T9: crate::StringSql,
    T10: crate::StringSql,
> {
    pub name: T1,
    pub ssh_uri: T2,
    pub systems: T4,
    pub max_jobs: i32,
    pub speed_factor: i32,
    pub supported_features: T6,
    pub mandatory_features: T8,
    pub public_host_key: Option<T9>,
    pub ssh_key_file: Option<T10>,
}
#[derive(Debug)]
pub struct UpdateParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
    T5: crate::StringSql,
    T6: crate::ArraySql<Item = T5>,
    T7: crate::StringSql,
    T8: crate::ArraySql<Item = T7>,
    T9: crate::StringSql,
    T10: crate::StringSql,
> {
    pub name: Option<T1>,
    pub ssh_uri: Option<T2>,
    pub systems: Option<T4>,
    pub max_jobs: Option<i32>,
    pub speed_factor: Option<i32>,
    pub supported_features: Option<T6>,
    pub mandatory_features: Option<T8>,
    pub enabled: Option<bool>,
    pub public_host_key: Option<T9>,
    pub ssh_key_file: Option<T10>,
    pub id: uuid::Uuid,
}
#[derive(Debug)]
pub struct UpsertParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::ArraySql<Item = T3>,
    T5: crate::StringSql,
    T6: crate::ArraySql<Item = T5>,
    T7: crate::StringSql,
    T8: crate::ArraySql<Item = T7>,
    T9: crate::StringSql,
    T10: crate::StringSql,
> {
    pub name: T1,
    pub ssh_uri: T2,
    pub systems: T4,
    pub max_jobs: i32,
    pub speed_factor: i32,
    pub supported_features: T6,
    pub mandatory_features: T8,
    pub enabled: bool,
    pub public_host_key: Option<T9>,
    pub ssh_key_file: Option<T10>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteBuilderRow {
    pub id: uuid::Uuid,
    pub name: String,
    pub ssh_uri: String,
    pub systems: Vec<String>,
    pub max_jobs: i32,
    pub speed_factor: i32,
    pub supported_features: Vec<String>,
    pub mandatory_features: Vec<String>,
    pub enabled: bool,
    pub public_host_key: Option<String>,
    pub ssh_key_file: Option<String>,
    pub consecutive_failures: i32,
    pub disabled_until: Option<chrono::DateTime<chrono::Utc>>,
    pub last_failure: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub cpu_cores: Option<i32>,
}
pub struct RemoteBuilderRowBorrowed<'a> {
    pub id: uuid::Uuid,
    pub name: &'a str,
    pub ssh_uri: &'a str,
    pub systems: crate::ArrayIterator<'a, &'a str>,
    pub max_jobs: i32,
    pub speed_factor: i32,
    pub supported_features: crate::ArrayIterator<'a, &'a str>,
    pub mandatory_features: crate::ArrayIterator<'a, &'a str>,
    pub enabled: bool,
    pub public_host_key: Option<&'a str>,
    pub ssh_key_file: Option<&'a str>,
    pub consecutive_failures: i32,
    pub disabled_until: Option<chrono::DateTime<chrono::Utc>>,
    pub last_failure: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub cpu_cores: Option<i32>,
}
impl<'a> From<RemoteBuilderRowBorrowed<'a>> for RemoteBuilderRow {
    fn from(
        RemoteBuilderRowBorrowed {
            id,
            name,
            ssh_uri,
            systems,
            max_jobs,
            speed_factor,
            supported_features,
            mandatory_features,
            enabled,
            public_host_key,
            ssh_key_file,
            consecutive_failures,
            disabled_until,
            last_failure,
            created_at,
            cpu_cores,
        }: RemoteBuilderRowBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            ssh_uri: ssh_uri.into(),
            systems: systems.map(|v| v.into()).collect(),
            max_jobs,
            speed_factor,
            supported_features: supported_features.map(|v| v.into()).collect(),
            mandatory_features: mandatory_features.map(|v| v.into()).collect(),
            enabled,
            public_host_key: public_host_key.map(|v| v.into()),
            ssh_key_file: ssh_key_file.map(|v| v.into()),
            consecutive_failures,
            disabled_until,
            last_failure,
            created_at,
            cpu_cores,
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct RemoteBuilderRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error>,
    mapper: fn(RemoteBuilderRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> RemoteBuilderRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(RemoteBuilderRowBorrowed) -> R,
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, R, N> {
        RemoteBuilderRowQuery {
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
        "INSERT INTO remote_builders ( name, ssh_uri, systems, max_jobs, speed_factor, supported_features, mandatory_features, public_host_key, ssh_key_file ) VALUES ( $1, $2, $3, $4, $5, $6, $7, $8, $9 ) RETURNING *",
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
        T4: crate::ArraySql<Item = T3>,
        T5: crate::StringSql,
        T6: crate::ArraySql<Item = T5>,
        T7: crate::StringSql,
        T8: crate::ArraySql<Item = T7>,
        T9: crate::StringSql,
        T10: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        name: &'a T1,
        ssh_uri: &'a T2,
        systems: &'a T4,
        max_jobs: &'a i32,
        speed_factor: &'a i32,
        supported_features: &'a T6,
        mandatory_features: &'a T8,
        public_host_key: &'a Option<T9>,
        ssh_key_file: &'a Option<T10>,
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 9> {
        RemoteBuilderRowQuery {
            client,
            params: [
                name,
                ssh_uri,
                systems,
                max_jobs,
                speed_factor,
                supported_features,
                mandatory_features,
                public_host_key,
                ssh_key_file,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error> {
                Ok(RemoteBuilderRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    ssh_uri: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    max_jobs: row.try_get(4)?,
                    speed_factor: row.try_get(5)?,
                    supported_features: row.try_get(6)?,
                    mandatory_features: row.try_get(7)?,
                    enabled: row.try_get(8)?,
                    public_host_key: row.try_get(9)?,
                    ssh_key_file: row.try_get(10)?,
                    consecutive_failures: row.try_get(11)?,
                    disabled_until: row.try_get(12)?,
                    last_failure: row.try_get(13)?,
                    created_at: row.try_get(14)?,
                    cpu_cores: row.try_get(15)?,
                })
            },
            mapper: |it| RemoteBuilderRow::from(it),
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
    T4: crate::ArraySql<Item = T3>,
    T5: crate::StringSql,
    T6: crate::ArraySql<Item = T5>,
    T7: crate::StringSql,
    T8: crate::ArraySql<Item = T7>,
    T9: crate::StringSql,
    T10: crate::StringSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        CreateParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
        RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 9>,
        C,
    > for CreateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a CreateParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 9> {
        self.bind(
            client,
            &params.name,
            &params.ssh_uri,
            &params.systems,
            &params.max_jobs,
            &params.speed_factor,
            &params.supported_features,
            &params.mandatory_features,
            &params.public_host_key,
            &params.ssh_key_file,
        )
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM remote_builders WHERE id =$1", None)
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
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 1> {
        RemoteBuilderRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error> {
                Ok(RemoteBuilderRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    ssh_uri: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    max_jobs: row.try_get(4)?,
                    speed_factor: row.try_get(5)?,
                    supported_features: row.try_get(6)?,
                    mandatory_features: row.try_get(7)?,
                    enabled: row.try_get(8)?,
                    public_host_key: row.try_get(9)?,
                    ssh_key_file: row.try_get(10)?,
                    consecutive_failures: row.try_get(11)?,
                    disabled_until: row.try_get(12)?,
                    last_failure: row.try_get(13)?,
                    created_at: row.try_get(14)?,
                    cpu_cores: row.try_get(15)?,
                })
            },
            mapper: |it| RemoteBuilderRow::from(it),
        }
    }
}
pub struct ListStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list() -> ListStmt {
    ListStmt(
        "SELECT * FROM remote_builders ORDER BY speed_factor DESC, name",
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
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 0> {
        RemoteBuilderRowQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error> {
                Ok(RemoteBuilderRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    ssh_uri: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    max_jobs: row.try_get(4)?,
                    speed_factor: row.try_get(5)?,
                    supported_features: row.try_get(6)?,
                    mandatory_features: row.try_get(7)?,
                    enabled: row.try_get(8)?,
                    public_host_key: row.try_get(9)?,
                    ssh_key_file: row.try_get(10)?,
                    consecutive_failures: row.try_get(11)?,
                    disabled_until: row.try_get(12)?,
                    last_failure: row.try_get(13)?,
                    created_at: row.try_get(14)?,
                    cpu_cores: row.try_get(15)?,
                })
            },
            mapper: |it| RemoteBuilderRow::from(it),
        }
    }
}
pub struct ListEnabledStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_enabled() -> ListEnabledStmt {
    ListEnabledStmt(
        "SELECT * FROM remote_builders WHERE enabled = true ORDER BY speed_factor DESC, name",
        None,
    )
}
impl ListEnabledStmt {
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
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 0> {
        RemoteBuilderRowQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error> {
                Ok(RemoteBuilderRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    ssh_uri: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    max_jobs: row.try_get(4)?,
                    speed_factor: row.try_get(5)?,
                    supported_features: row.try_get(6)?,
                    mandatory_features: row.try_get(7)?,
                    enabled: row.try_get(8)?,
                    public_host_key: row.try_get(9)?,
                    ssh_key_file: row.try_get(10)?,
                    consecutive_failures: row.try_get(11)?,
                    disabled_until: row.try_get(12)?,
                    last_failure: row.try_get(13)?,
                    created_at: row.try_get(14)?,
                    cpu_cores: row.try_get(15)?,
                })
            },
            mapper: |it| RemoteBuilderRow::from(it),
        }
    }
}
pub struct FindForSystemSpeedFactorStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn find_for_system_speed_factor() -> FindForSystemSpeedFactorStmt {
    FindForSystemSpeedFactorStmt(
        "SELECT * FROM remote_builders WHERE enabled = true AND $1 = ANY (systems) AND ( disabled_until IS NULL OR disabled_until < NOW() ) ORDER BY speed_factor DESC",
        None,
    )
}
impl FindForSystemSpeedFactorStmt {
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
        system: &'a T1,
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 1> {
        RemoteBuilderRowQuery {
            client,
            params: [system],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error> {
                Ok(RemoteBuilderRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    ssh_uri: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    max_jobs: row.try_get(4)?,
                    speed_factor: row.try_get(5)?,
                    supported_features: row.try_get(6)?,
                    mandatory_features: row.try_get(7)?,
                    enabled: row.try_get(8)?,
                    public_host_key: row.try_get(9)?,
                    ssh_key_file: row.try_get(10)?,
                    consecutive_failures: row.try_get(11)?,
                    disabled_until: row.try_get(12)?,
                    last_failure: row.try_get(13)?,
                    created_at: row.try_get(14)?,
                    cpu_cores: row.try_get(15)?,
                })
            },
            mapper: |it| RemoteBuilderRow::from(it),
        }
    }
}
pub struct FindForSystemCpuWeightedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn find_for_system_cpu_weighted() -> FindForSystemCpuWeightedStmt {
    FindForSystemCpuWeightedStmt(
        "SELECT * FROM remote_builders WHERE enabled = true AND $1 = ANY (systems) AND ( disabled_until IS NULL OR disabled_until < NOW() ) ORDER BY COALESCE(cpu_cores, 1) * speed_factor DESC",
        None,
    )
}
impl FindForSystemCpuWeightedStmt {
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
        system: &'a T1,
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 1> {
        RemoteBuilderRowQuery {
            client,
            params: [system],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error> {
                Ok(RemoteBuilderRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    ssh_uri: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    max_jobs: row.try_get(4)?,
                    speed_factor: row.try_get(5)?,
                    supported_features: row.try_get(6)?,
                    mandatory_features: row.try_get(7)?,
                    enabled: row.try_get(8)?,
                    public_host_key: row.try_get(9)?,
                    ssh_key_file: row.try_get(10)?,
                    consecutive_failures: row.try_get(11)?,
                    disabled_until: row.try_get(12)?,
                    last_failure: row.try_get(13)?,
                    created_at: row.try_get(14)?,
                    cpu_cores: row.try_get(15)?,
                })
            },
            mapper: |it| RemoteBuilderRow::from(it),
        }
    }
}
pub struct FindForSystemDynamicStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn find_for_system_dynamic() -> FindForSystemDynamicStmt {
    FindForSystemDynamicStmt(
        "SELECT r.* FROM remote_builders r LEFT JOIN ( SELECT builder_id, COUNT(*) AS cnt FROM builds WHERE status = 'running' GROUP BY builder_id ) active ON active.builder_id = r.id WHERE r.enabled = true AND $1 = ANY (r.systems) AND ( r.disabled_until IS NULL OR r.disabled_until < NOW() ) ORDER BY (r.max_jobs - COALESCE(active.cnt, 0)) * r.speed_factor DESC",
        None,
    )
}
impl FindForSystemDynamicStmt {
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
        system: &'a T1,
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 1> {
        RemoteBuilderRowQuery {
            client,
            params: [system],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error> {
                Ok(RemoteBuilderRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    ssh_uri: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    max_jobs: row.try_get(4)?,
                    speed_factor: row.try_get(5)?,
                    supported_features: row.try_get(6)?,
                    mandatory_features: row.try_get(7)?,
                    enabled: row.try_get(8)?,
                    public_host_key: row.try_get(9)?,
                    ssh_key_file: row.try_get(10)?,
                    consecutive_failures: row.try_get(11)?,
                    disabled_until: row.try_get(12)?,
                    last_failure: row.try_get(13)?,
                    created_at: row.try_get(14)?,
                    cpu_cores: row.try_get(15)?,
                })
            },
            mapper: |it| RemoteBuilderRow::from(it),
        }
    }
}
pub struct RecordFailureStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record_failure() -> RecordFailureStmt {
    RecordFailureStmt(
        "UPDATE remote_builders SET consecutive_failures = LEAST(consecutive_failures + 1, 4), last_failure = NOW(), disabled_until = NOW() + make_interval( secs => 60.0 * power(3, LEAST(consecutive_failures + 1, 4) - 1) + (random() * 30)::int ) WHERE id =$1 RETURNING *",
        None,
    )
}
impl RecordFailureStmt {
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
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 1> {
        RemoteBuilderRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error> {
                Ok(RemoteBuilderRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    ssh_uri: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    max_jobs: row.try_get(4)?,
                    speed_factor: row.try_get(5)?,
                    supported_features: row.try_get(6)?,
                    mandatory_features: row.try_get(7)?,
                    enabled: row.try_get(8)?,
                    public_host_key: row.try_get(9)?,
                    ssh_key_file: row.try_get(10)?,
                    consecutive_failures: row.try_get(11)?,
                    disabled_until: row.try_get(12)?,
                    last_failure: row.try_get(13)?,
                    created_at: row.try_get(14)?,
                    cpu_cores: row.try_get(15)?,
                })
            },
            mapper: |it| RemoteBuilderRow::from(it),
        }
    }
}
pub struct RecordSuccessStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record_success() -> RecordSuccessStmt {
    RecordSuccessStmt(
        "UPDATE remote_builders SET consecutive_failures = 0, disabled_until = NULL WHERE id =$1 RETURNING *",
        None,
    )
}
impl RecordSuccessStmt {
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
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 1> {
        RemoteBuilderRowQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error> {
                Ok(RemoteBuilderRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    ssh_uri: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    max_jobs: row.try_get(4)?,
                    speed_factor: row.try_get(5)?,
                    supported_features: row.try_get(6)?,
                    mandatory_features: row.try_get(7)?,
                    enabled: row.try_get(8)?,
                    public_host_key: row.try_get(9)?,
                    ssh_key_file: row.try_get(10)?,
                    consecutive_failures: row.try_get(11)?,
                    disabled_until: row.try_get(12)?,
                    last_failure: row.try_get(13)?,
                    created_at: row.try_get(14)?,
                    cpu_cores: row.try_get(15)?,
                })
            },
            mapper: |it| RemoteBuilderRow::from(it),
        }
    }
}
pub struct UpdateStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn update() -> UpdateStmt {
    UpdateStmt(
        "UPDATE remote_builders SET name = COALESCE($1, name), ssh_uri = COALESCE($2, ssh_uri), systems = COALESCE($3, systems), max_jobs = COALESCE($4, max_jobs), speed_factor = COALESCE($5, speed_factor), supported_features = COALESCE($6, supported_features), mandatory_features = COALESCE($7, mandatory_features), enabled = COALESCE($8, enabled), public_host_key = COALESCE($9, public_host_key), ssh_key_file = COALESCE($10, ssh_key_file) WHERE id =$11 RETURNING *",
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
    pub fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
        T4: crate::ArraySql<Item = T3>,
        T5: crate::StringSql,
        T6: crate::ArraySql<Item = T5>,
        T7: crate::StringSql,
        T8: crate::ArraySql<Item = T7>,
        T9: crate::StringSql,
        T10: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        name: &'a Option<T1>,
        ssh_uri: &'a Option<T2>,
        systems: &'a Option<T4>,
        max_jobs: &'a Option<i32>,
        speed_factor: &'a Option<i32>,
        supported_features: &'a Option<T6>,
        mandatory_features: &'a Option<T8>,
        enabled: &'a Option<bool>,
        public_host_key: &'a Option<T9>,
        ssh_key_file: &'a Option<T10>,
        id: &'a uuid::Uuid,
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 11> {
        RemoteBuilderRowQuery {
            client,
            params: [
                name,
                ssh_uri,
                systems,
                max_jobs,
                speed_factor,
                supported_features,
                mandatory_features,
                enabled,
                public_host_key,
                ssh_key_file,
                id,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error> {
                Ok(RemoteBuilderRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    ssh_uri: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    max_jobs: row.try_get(4)?,
                    speed_factor: row.try_get(5)?,
                    supported_features: row.try_get(6)?,
                    mandatory_features: row.try_get(7)?,
                    enabled: row.try_get(8)?,
                    public_host_key: row.try_get(9)?,
                    ssh_key_file: row.try_get(10)?,
                    consecutive_failures: row.try_get(11)?,
                    disabled_until: row.try_get(12)?,
                    last_failure: row.try_get(13)?,
                    created_at: row.try_get(14)?,
                    cpu_cores: row.try_get(15)?,
                })
            },
            mapper: |it| RemoteBuilderRow::from(it),
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
    T4: crate::ArraySql<Item = T3>,
    T5: crate::StringSql,
    T6: crate::ArraySql<Item = T5>,
    T7: crate::StringSql,
    T8: crate::ArraySql<Item = T7>,
    T9: crate::StringSql,
    T10: crate::StringSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpdateParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
        RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 11>,
        C,
    > for UpdateStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpdateParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 11> {
        self.bind(
            client,
            &params.name,
            &params.ssh_uri,
            &params.systems,
            &params.max_jobs,
            &params.speed_factor,
            &params.supported_features,
            &params.mandatory_features,
            &params.enabled,
            &params.public_host_key,
            &params.ssh_key_file,
            &params.id,
        )
    }
}
pub struct DeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn delete() -> DeleteStmt {
    DeleteStmt("DELETE FROM remote_builders WHERE id =$1", None)
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
pub struct CountStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn count() -> CountStmt {
    CountStmt("SELECT COUNT(*) FROM remote_builders", None)
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
pub struct UpsertStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn upsert() -> UpsertStmt {
    UpsertStmt(
        "INSERT INTO remote_builders ( name, ssh_uri, systems, max_jobs, speed_factor, supported_features, mandatory_features, enabled, public_host_key, ssh_key_file ) VALUES ( $1, $2, $3, $4, $5, $6, $7, $8, $9, $10 ) ON CONFLICT (name) DO UPDATE SET ssh_uri = EXCLUDED.ssh_uri, systems = EXCLUDED.systems, max_jobs = EXCLUDED.max_jobs, speed_factor = EXCLUDED.speed_factor, supported_features = EXCLUDED.supported_features, mandatory_features = EXCLUDED.mandatory_features, enabled = EXCLUDED.enabled, public_host_key = COALESCE( EXCLUDED.public_host_key, remote_builders.public_host_key ), ssh_key_file = COALESCE( EXCLUDED.ssh_key_file, remote_builders.ssh_key_file ) RETURNING *",
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
        T4: crate::ArraySql<Item = T3>,
        T5: crate::StringSql,
        T6: crate::ArraySql<Item = T5>,
        T7: crate::StringSql,
        T8: crate::ArraySql<Item = T7>,
        T9: crate::StringSql,
        T10: crate::StringSql,
    >(
        &'s self,
        client: &'c C,
        name: &'a T1,
        ssh_uri: &'a T2,
        systems: &'a T4,
        max_jobs: &'a i32,
        speed_factor: &'a i32,
        supported_features: &'a T6,
        mandatory_features: &'a T8,
        enabled: &'a bool,
        public_host_key: &'a Option<T9>,
        ssh_key_file: &'a Option<T10>,
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 10> {
        RemoteBuilderRowQuery {
            client,
            params: [
                name,
                ssh_uri,
                systems,
                max_jobs,
                speed_factor,
                supported_features,
                mandatory_features,
                enabled,
                public_host_key,
                ssh_key_file,
            ],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<RemoteBuilderRowBorrowed, tokio_postgres::Error> {
                Ok(RemoteBuilderRowBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    ssh_uri: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    max_jobs: row.try_get(4)?,
                    speed_factor: row.try_get(5)?,
                    supported_features: row.try_get(6)?,
                    mandatory_features: row.try_get(7)?,
                    enabled: row.try_get(8)?,
                    public_host_key: row.try_get(9)?,
                    ssh_key_file: row.try_get(10)?,
                    consecutive_failures: row.try_get(11)?,
                    disabled_until: row.try_get(12)?,
                    last_failure: row.try_get(13)?,
                    created_at: row.try_get(14)?,
                    cpu_cores: row.try_get(15)?,
                })
            },
            mapper: |it| RemoteBuilderRow::from(it),
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
    T4: crate::ArraySql<Item = T3>,
    T5: crate::StringSql,
    T6: crate::ArraySql<Item = T5>,
    T7: crate::StringSql,
    T8: crate::ArraySql<Item = T7>,
    T9: crate::StringSql,
    T10: crate::StringSql,
>
    crate::client::async_::Params<
        'c,
        'a,
        's,
        UpsertParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
        RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 10>,
        C,
    > for UpsertStmt
{
    fn params(
        &'s self,
        client: &'c C,
        params: &'a UpsertParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
    ) -> RemoteBuilderRowQuery<'c, 'a, 's, C, RemoteBuilderRow, 10> {
        self.bind(
            client,
            &params.name,
            &params.ssh_uri,
            &params.systems,
            &params.max_jobs,
            &params.speed_factor,
            &params.supported_features,
            &params.mandatory_features,
            &params.enabled,
            &params.public_host_key,
            &params.ssh_key_file,
        )
    }
}
pub struct SyncAllDeleteStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn sync_all_delete() -> SyncAllDeleteStmt {
    SyncAllDeleteStmt(
        "DELETE FROM remote_builders WHERE name != ALL ($1::text[])",
        None,
    )
}
impl SyncAllDeleteStmt {
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
        names: &'a T2,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[names]).await
    }
}
