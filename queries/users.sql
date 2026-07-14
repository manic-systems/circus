--: UserRow(full_name?, password_hash?, last_login_at?)

--! create (full_name?) : UserRow
INSERT INTO users (username, email, full_name, password_hash, role)
VALUES (:username, :email, :full_name, :password_hash, :role)
RETURNING *;

--! authenticate_fetch : UserRow
SELECT * FROM users WHERE username = :username AND enabled = true;

--! authenticate_touch
UPDATE users SET last_login_at = NOW() WHERE id = :id;

--! get : UserRow
SELECT * FROM users WHERE id = :id;

--! get_by_username : UserRow
SELECT * FROM users WHERE username = :username;

--! get_by_email : UserRow
SELECT * FROM users WHERE email = :email;

--! list : UserRow
SELECT * FROM users ORDER BY created_at DESC LIMIT :limit OFFSET :offset;

--! count
SELECT COUNT(*) FROM users;

--! update_email : UserRow
UPDATE users SET email = :email WHERE id = :id RETURNING *;

--! update_full_name (full_name?)
UPDATE users SET full_name = :full_name WHERE id = :id;

--! update_password
UPDATE users SET password_hash = :password_hash WHERE id = :id;

--! update_role
UPDATE users SET role = :role WHERE id = :id;

--! set_enabled
UPDATE users SET enabled = :enabled WHERE id = :id;

--! set_public_dashboard
UPDATE users SET public_dashboard = :public_dashboard WHERE id = :id;

--! delete
DELETE FROM users WHERE id = :id;

--! upsert_oauth_user_fetch : UserRow
SELECT * FROM users WHERE username = :username;

--! upsert_oauth_user_update_email
UPDATE users SET email = :email, last_login_at = NOW(), updated_at = NOW()
WHERE id = :id;

--! upsert_oauth_user_touch
UPDATE users SET last_login_at = NOW(), updated_at = NOW() WHERE id = :id;

--! upsert_oauth_user_insert : UserRow
INSERT INTO users (username, email, user_type, password_hash, role)
VALUES (:username, :email, :user_type, NULL, 'read-only')
RETURNING *;

--! create_session
INSERT INTO user_sessions (user_id, session_token_hash, expires_at)
VALUES (:user_id, :session_token_hash, :expires_at)
RETURNING id;

--! validate_session_fetch : UserRow
SELECT u.* FROM users u
JOIN user_sessions s ON u.id = s.user_id
WHERE s.session_token_hash = :session_token_hash
  AND s.expires_at > NOW()
  AND u.enabled = true;

--! validate_session_touch
UPDATE user_sessions SET last_used_at = NOW()
WHERE session_token_hash = :session_token_hash;

--! delete_session
DELETE FROM user_sessions WHERE session_token_hash = :session_token_hash;
