--: NewsRow(created_by?)

--! create (created_by?) : NewsRow
INSERT INTO news (title, content, created_by)
VALUES (:title, :content, :created_by)
RETURNING *;

--! list : NewsRow
SELECT * FROM news ORDER BY created_at DESC LIMIT :limit OFFSET :offset;

--! count
SELECT COUNT(*) FROM news;

--! delete
DELETE FROM news WHERE id = :id;
