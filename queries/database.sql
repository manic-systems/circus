--! connection_info : (server_ip?, server_port?)
SELECT
  current_database()::text AS database,
  current_user::text AS "user",
  version() AS version,
  host(inet_server_addr()) AS server_ip,
  inet_server_port() AS server_port;

--! notify
SELECT true AS sent FROM pg_notify(:channel, '');
