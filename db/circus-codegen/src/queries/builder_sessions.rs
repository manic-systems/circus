// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct RegisterParams<
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
    pub machine_id: uuid::Uuid,
    pub name: T1,
    pub hostname: T2,
    pub systems: T4,
    pub supported_features: T6,
    pub mandatory_features: T8,
    pub speed_factor: f32,
    pub cpu_count: i32,
    pub max_jobs: i32,
    pub proto_version: T9,
    pub ephemeral: bool,
    pub auth_kind: T10,
}
#[derive(Clone, Copy, Debug)]
pub struct HeartbeatParams {
    pub load1: f32,
    pub load5: f32,
    pub load15: f32,
    pub cpu_psi_avg10: f32,
    pub mem_psi_avg10: f32,
    pub io_psi_avg10: f32,
    pub current_jobs: i32,
    pub mem_total: i64,
    pub mem_used: i64,
    pub store_free: i64,
    pub build_dir_free: i64,
    pub machine_id: uuid::Uuid,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BuilderSessionRow {
    pub machine_id: uuid::Uuid,
    pub name: String,
    pub hostname: String,
    pub systems: Vec<String>,
    pub supported_features: Vec<String>,
    pub mandatory_features: Vec<String>,
    pub speed_factor: f32,
    pub cpu_count: i32,
    pub max_jobs: i32,
    pub proto_version: String,
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
    pub current_jobs: i32,
    pub load1: Option<f32>,
    pub load5: Option<f32>,
    pub load15: Option<f32>,
    pub mem_total: Option<i64>,
    pub mem_used: Option<i64>,
    pub store_free: Option<i64>,
    pub build_dir_free: Option<i64>,
    pub cpu_psi_avg10: Option<f32>,
    pub mem_psi_avg10: Option<f32>,
    pub io_psi_avg10: Option<f32>,
    pub connected: bool,
    pub builds_succeeded: i64,
    pub builds_failed: i64,
    pub consecutive_failures: i32,
    pub disabled_until: Option<chrono::DateTime<chrono::Utc>>,
    pub auth_token_hash: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub ephemeral: bool,
    pub auth_kind: String,
}
pub struct BuilderSessionRowBorrowed<'a> {
    pub machine_id: uuid::Uuid,
    pub name: &'a str,
    pub hostname: &'a str,
    pub systems: crate::ArrayIterator<'a, &'a str>,
    pub supported_features: crate::ArrayIterator<'a, &'a str>,
    pub mandatory_features: crate::ArrayIterator<'a, &'a str>,
    pub speed_factor: f32,
    pub cpu_count: i32,
    pub max_jobs: i32,
    pub proto_version: &'a str,
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
    pub current_jobs: i32,
    pub load1: Option<f32>,
    pub load5: Option<f32>,
    pub load15: Option<f32>,
    pub mem_total: Option<i64>,
    pub mem_used: Option<i64>,
    pub store_free: Option<i64>,
    pub build_dir_free: Option<i64>,
    pub cpu_psi_avg10: Option<f32>,
    pub mem_psi_avg10: Option<f32>,
    pub io_psi_avg10: Option<f32>,
    pub connected: bool,
    pub builds_succeeded: i64,
    pub builds_failed: i64,
    pub consecutive_failures: i32,
    pub disabled_until: Option<chrono::DateTime<chrono::Utc>>,
    pub auth_token_hash: Option<&'a str>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub ephemeral: bool,
    pub auth_kind: &'a str,
}
impl<'a> From<BuilderSessionRowBorrowed<'a>> for BuilderSessionRow {
    fn from(
        BuilderSessionRowBorrowed {
            machine_id,
            name,
            hostname,
            systems,
            supported_features,
            mandatory_features,
            speed_factor,
            cpu_count,
            max_jobs,
            proto_version,
            last_seen,
            current_jobs,
            load1,
            load5,
            load15,
            mem_total,
            mem_used,
            store_free,
            build_dir_free,
            cpu_psi_avg10,
            mem_psi_avg10,
            io_psi_avg10,
            connected,
            builds_succeeded,
            builds_failed,
            consecutive_failures,
            disabled_until,
            auth_token_hash,
            created_at,
            updated_at,
            ephemeral,
            auth_kind,
        }: BuilderSessionRowBorrowed<'a>,
    ) -> Self {
        Self {
            machine_id,
            name: name.into(),
            hostname: hostname.into(),
            systems: systems.map(|v| v.into()).collect(),
            supported_features: supported_features.map(|v| v.into()).collect(),
            mandatory_features: mandatory_features.map(|v| v.into()).collect(),
            speed_factor,
            cpu_count,
            max_jobs,
            proto_version: proto_version.into(),
            last_seen,
            current_jobs,
            load1,
            load5,
            load15,
            mem_total,
            mem_used,
            store_free,
            build_dir_free,
            cpu_psi_avg10,
            mem_psi_avg10,
            io_psi_avg10,
            connected,
            builds_succeeded,
            builds_failed,
            consecutive_failures,
            disabled_until,
            auth_token_hash: auth_token_hash.map(|v| v.into()),
            created_at,
            updated_at,
            ephemeral,
            auth_kind: auth_kind.into(),
        }
    }
}
use crate::client::async_::GenericClient;
use futures::{self, StreamExt, TryStreamExt};
pub struct BuilderSessionRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s tokio_postgres::Statement>,
    extractor: fn(&tokio_postgres::Row) -> Result<BuilderSessionRowBorrowed, tokio_postgres::Error>,
    mapper: fn(BuilderSessionRowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BuilderSessionRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(
        self,
        mapper: fn(BuilderSessionRowBorrowed) -> R,
    ) -> BuilderSessionRowQuery<'c, 'a, 's, C, R, N> {
        BuilderSessionRowQuery {
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
pub struct ListStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list() -> ListStmt {
    ListStmt(
        "SELECT * FROM builder_sessions ORDER BY connected DESC, updated_at DESC",
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
    ) -> BuilderSessionRowQuery<'c, 'a, 's, C, BuilderSessionRow, 0> {
        BuilderSessionRowQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuilderSessionRowBorrowed, tokio_postgres::Error> {
                Ok(BuilderSessionRowBorrowed {
                    machine_id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    hostname: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    supported_features: row.try_get(4)?,
                    mandatory_features: row.try_get(5)?,
                    speed_factor: row.try_get(6)?,
                    cpu_count: row.try_get(7)?,
                    max_jobs: row.try_get(8)?,
                    proto_version: row.try_get(9)?,
                    last_seen: row.try_get(10)?,
                    current_jobs: row.try_get(11)?,
                    load1: row.try_get(12)?,
                    load5: row.try_get(13)?,
                    load15: row.try_get(14)?,
                    mem_total: row.try_get(15)?,
                    mem_used: row.try_get(16)?,
                    store_free: row.try_get(17)?,
                    build_dir_free: row.try_get(18)?,
                    cpu_psi_avg10: row.try_get(19)?,
                    mem_psi_avg10: row.try_get(20)?,
                    io_psi_avg10: row.try_get(21)?,
                    connected: row.try_get(22)?,
                    builds_succeeded: row.try_get(23)?,
                    builds_failed: row.try_get(24)?,
                    consecutive_failures: row.try_get(25)?,
                    disabled_until: row.try_get(26)?,
                    auth_token_hash: row.try_get(27)?,
                    created_at: row.try_get(28)?,
                    updated_at: row.try_get(29)?,
                    ephemeral: row.try_get(30)?,
                    auth_kind: row.try_get(31)?,
                })
            },
            mapper: |it| BuilderSessionRow::from(it),
        }
    }
}
pub struct ListConnectedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn list_connected() -> ListConnectedStmt {
    ListConnectedStmt(
        "SELECT * FROM builder_sessions WHERE connected = TRUE ORDER BY updated_at DESC",
        None,
    )
}
impl ListConnectedStmt {
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
    ) -> BuilderSessionRowQuery<'c, 'a, 's, C, BuilderSessionRow, 0> {
        BuilderSessionRowQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuilderSessionRowBorrowed, tokio_postgres::Error> {
                Ok(BuilderSessionRowBorrowed {
                    machine_id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    hostname: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    supported_features: row.try_get(4)?,
                    mandatory_features: row.try_get(5)?,
                    speed_factor: row.try_get(6)?,
                    cpu_count: row.try_get(7)?,
                    max_jobs: row.try_get(8)?,
                    proto_version: row.try_get(9)?,
                    last_seen: row.try_get(10)?,
                    current_jobs: row.try_get(11)?,
                    load1: row.try_get(12)?,
                    load5: row.try_get(13)?,
                    load15: row.try_get(14)?,
                    mem_total: row.try_get(15)?,
                    mem_used: row.try_get(16)?,
                    store_free: row.try_get(17)?,
                    build_dir_free: row.try_get(18)?,
                    cpu_psi_avg10: row.try_get(19)?,
                    mem_psi_avg10: row.try_get(20)?,
                    io_psi_avg10: row.try_get(21)?,
                    connected: row.try_get(22)?,
                    builds_succeeded: row.try_get(23)?,
                    builds_failed: row.try_get(24)?,
                    consecutive_failures: row.try_get(25)?,
                    disabled_until: row.try_get(26)?,
                    auth_token_hash: row.try_get(27)?,
                    created_at: row.try_get(28)?,
                    updated_at: row.try_get(29)?,
                    ephemeral: row.try_get(30)?,
                    auth_kind: row.try_get(31)?,
                })
            },
            mapper: |it| BuilderSessionRow::from(it),
        }
    }
}
pub struct GetStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn get() -> GetStmt {
    GetStmt("SELECT * FROM builder_sessions WHERE machine_id =$1", None)
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
        machine_id: &'a uuid::Uuid,
    ) -> BuilderSessionRowQuery<'c, 'a, 's, C, BuilderSessionRow, 1> {
        BuilderSessionRowQuery {
            client,
            params: [machine_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |
                row: &tokio_postgres::Row,
            | -> Result<BuilderSessionRowBorrowed, tokio_postgres::Error> {
                Ok(BuilderSessionRowBorrowed {
                    machine_id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    hostname: row.try_get(2)?,
                    systems: row.try_get(3)?,
                    supported_features: row.try_get(4)?,
                    mandatory_features: row.try_get(5)?,
                    speed_factor: row.try_get(6)?,
                    cpu_count: row.try_get(7)?,
                    max_jobs: row.try_get(8)?,
                    proto_version: row.try_get(9)?,
                    last_seen: row.try_get(10)?,
                    current_jobs: row.try_get(11)?,
                    load1: row.try_get(12)?,
                    load5: row.try_get(13)?,
                    load15: row.try_get(14)?,
                    mem_total: row.try_get(15)?,
                    mem_used: row.try_get(16)?,
                    store_free: row.try_get(17)?,
                    build_dir_free: row.try_get(18)?,
                    cpu_psi_avg10: row.try_get(19)?,
                    mem_psi_avg10: row.try_get(20)?,
                    io_psi_avg10: row.try_get(21)?,
                    connected: row.try_get(22)?,
                    builds_succeeded: row.try_get(23)?,
                    builds_failed: row.try_get(24)?,
                    consecutive_failures: row.try_get(25)?,
                    disabled_until: row.try_get(26)?,
                    auth_token_hash: row.try_get(27)?,
                    created_at: row.try_get(28)?,
                    updated_at: row.try_get(29)?,
                    ephemeral: row.try_get(30)?,
                    auth_kind: row.try_get(31)?,
                })
            },
            mapper: |it| BuilderSessionRow::from(it),
        }
    }
}
pub struct RecordOutcomeSucceededStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record_outcome_succeeded() -> RecordOutcomeSucceededStmt {
    RecordOutcomeSucceededStmt(
        "UPDATE builder_sessions SET builds_succeeded = builds_succeeded + 1, consecutive_failures = 0, disabled_until = NULL, updated_at = NOW() WHERE machine_id =$1",
        None,
    )
}
impl RecordOutcomeSucceededStmt {
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
        machine_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[machine_id]).await
    }
}
pub struct RecordOutcomeFailedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn record_outcome_failed() -> RecordOutcomeFailedStmt {
    RecordOutcomeFailedStmt(
        "UPDATE builder_sessions SET builds_failed = builds_failed + 1, consecutive_failures = LEAST(consecutive_failures + 1, 4), disabled_until = NOW() + make_interval( secs => 60.0 * power(3, LEAST(consecutive_failures + 1, 4) - 1) + (random() * 30)::int ), updated_at = NOW() WHERE machine_id =$1",
        None,
    )
}
impl RecordOutcomeFailedStmt {
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
        machine_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[machine_id]).await
    }
}
pub struct IsSchedulableStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn is_schedulable() -> IsSchedulableStmt {
    IsSchedulableStmt(
        "SELECT disabled_until IS NULL OR disabled_until <= NOW() FROM builder_sessions WHERE machine_id =$1",
        None,
    )
}
impl IsSchedulableStmt {
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
        machine_id: &'a uuid::Uuid,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 1> {
        BoolQuery {
            client,
            params: [machine_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct PruneStaleEphemeralStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn prune_stale_ephemeral() -> PruneStaleEphemeralStmt {
    PruneStaleEphemeralStmt(
        "DELETE FROM builder_sessions WHERE ephemeral = TRUE AND ( ( connected = FALSE AND ( last_seen IS NULL OR last_seen < NOW() - make_interval(secs => $1) ) ) OR ( connected = TRUE AND last_seen IS NOT NULL AND last_seen < NOW() - make_interval(secs => $1) ) )",
        None,
    )
}
impl PruneStaleEphemeralStmt {
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
        ttl_secs: &'a f64,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[ttl_secs]).await
    }
}
pub struct ResetAllConnectedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn reset_all_connected() -> ResetAllConnectedStmt {
    ResetAllConnectedStmt(
        "UPDATE builder_sessions SET connected = FALSE WHERE connected = TRUE",
        None,
    )
}
impl ResetAllConnectedStmt {
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
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[]).await
    }
}
pub struct RegisterStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn register() -> RegisterStmt {
    RegisterStmt(
        "INSERT INTO builder_sessions ( machine_id, name, hostname, systems, supported_features, mandatory_features, speed_factor, cpu_count, max_jobs, proto_version, ephemeral, auth_kind, connected, last_seen, updated_at ) VALUES ( $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, TRUE, NOW(), NOW() ) ON CONFLICT (machine_id) DO UPDATE SET name = EXCLUDED.name, hostname = EXCLUDED.hostname, systems = EXCLUDED.systems, supported_features = EXCLUDED.supported_features, mandatory_features = EXCLUDED.mandatory_features, speed_factor = EXCLUDED.speed_factor, cpu_count = EXCLUDED.cpu_count, max_jobs = EXCLUDED.max_jobs, proto_version = EXCLUDED.proto_version, ephemeral = EXCLUDED.ephemeral, auth_kind = EXCLUDED.auth_kind, connected = TRUE, last_seen = NOW(), updated_at = NOW()",
        None,
    )
}
impl RegisterStmt {
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
        machine_id: &'a uuid::Uuid,
        name: &'a T1,
        hostname: &'a T2,
        systems: &'a T4,
        supported_features: &'a T6,
        mandatory_features: &'a T8,
        speed_factor: &'a f32,
        cpu_count: &'a i32,
        max_jobs: &'a i32,
        proto_version: &'a T9,
        ephemeral: &'a bool,
        auth_kind: &'a T10,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    machine_id,
                    name,
                    hostname,
                    systems,
                    supported_features,
                    mandatory_features,
                    speed_factor,
                    cpu_count,
                    max_jobs,
                    proto_version,
                    ephemeral,
                    auth_kind,
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
    T4: crate::ArraySql<Item = T3>,
    T5: crate::StringSql,
    T6: crate::ArraySql<Item = T5>,
    T7: crate::StringSql,
    T8: crate::ArraySql<Item = T7>,
    T9: crate::StringSql,
    T10: crate::StringSql,
>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        RegisterParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for RegisterStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a RegisterParams<T1, T2, T3, T4, T5, T6, T7, T8, T9, T10>,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.machine_id,
            &params.name,
            &params.hostname,
            &params.systems,
            &params.supported_features,
            &params.mandatory_features,
            &params.speed_factor,
            &params.cpu_count,
            &params.max_jobs,
            &params.proto_version,
            &params.ephemeral,
            &params.auth_kind,
        ))
    }
}
pub struct MarkDisconnectedStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn mark_disconnected() -> MarkDisconnectedStmt {
    MarkDisconnectedStmt(
        "UPDATE builder_sessions SET connected = FALSE, updated_at = NOW() WHERE machine_id = $1",
        None,
    )
}
impl MarkDisconnectedStmt {
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
        machine_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[machine_id]).await
    }
}
pub struct TouchStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn touch() -> TouchStmt {
    TouchStmt(
        "UPDATE builder_sessions SET updated_at = NOW() WHERE machine_id = $1",
        None,
    )
}
impl TouchStmt {
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
        machine_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client.execute(self.0, &[machine_id]).await
    }
}
pub struct HeartbeatStmt(&'static str, Option<tokio_postgres::Statement>);
pub fn heartbeat() -> HeartbeatStmt {
    HeartbeatStmt(
        "UPDATE builder_sessions SET last_seen = NOW(), load1 = $1, load5 = $2, load15 = $3, cpu_psi_avg10 = $4, mem_psi_avg10 = $5, io_psi_avg10 = $6, current_jobs = $7, mem_total = $8, mem_used = $9, store_free = $10, build_dir_free = $11, updated_at = NOW() WHERE machine_id = $12",
        None,
    )
}
impl HeartbeatStmt {
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
        load1: &'a f32,
        load5: &'a f32,
        load15: &'a f32,
        cpu_psi_avg10: &'a f32,
        mem_psi_avg10: &'a f32,
        io_psi_avg10: &'a f32,
        current_jobs: &'a i32,
        mem_total: &'a i64,
        mem_used: &'a i64,
        store_free: &'a i64,
        build_dir_free: &'a i64,
        machine_id: &'a uuid::Uuid,
    ) -> Result<u64, tokio_postgres::Error> {
        client
            .execute(
                self.0,
                &[
                    load1,
                    load5,
                    load15,
                    cpu_psi_avg10,
                    mem_psi_avg10,
                    io_psi_avg10,
                    current_jobs,
                    mem_total,
                    mem_used,
                    store_free,
                    build_dir_free,
                    machine_id,
                ],
            )
            .await
    }
}
impl<'a, C: GenericClient + Send + Sync>
    crate::client::async_::Params<
        'a,
        'a,
        'a,
        HeartbeatParams,
        std::pin::Pin<
            Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
        >,
        C,
    > for HeartbeatStmt
{
    fn params(
        &'a self,
        client: &'a C,
        params: &'a HeartbeatParams,
    ) -> std::pin::Pin<
        Box<dyn futures::Future<Output = Result<u64, tokio_postgres::Error>> + Send + 'a>,
    > {
        Box::pin(self.bind(
            client,
            &params.load1,
            &params.load5,
            &params.load15,
            &params.cpu_psi_avg10,
            &params.mem_psi_avg10,
            &params.io_psi_avg10,
            &params.current_jobs,
            &params.mem_total,
            &params.mem_used,
            &params.store_free,
            &params.build_dir_free,
            &params.machine_id,
        ))
    }
}
