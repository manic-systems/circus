--: StarredJobRow(jobset_id?)

--! create (jobset_id?) : StarredJobRow
INSERT INTO starred_jobs (user_id, project_id, jobset_id, job_name)
VALUES (:user_id, :project_id, :jobset_id, :job_name)
RETURNING *;

--! get : StarredJobRow
SELECT * FROM starred_jobs WHERE id = :id;

--! list_for_user : StarredJobRow
SELECT * FROM starred_jobs WHERE user_id = :user_id ORDER BY created_at DESC
LIMIT :limit OFFSET :offset;

--! count_for_user
SELECT COUNT(*) FROM starred_jobs WHERE user_id = :user_id;

--! is_starred (jobset_id?)
SELECT COUNT(*) FROM starred_jobs WHERE user_id = :user_id AND project_id = :project_id
AND jobset_id IS NOT DISTINCT FROM :jobset_id AND job_name = :job_name;

--! delete
DELETE FROM starred_jobs WHERE id = :id;

--! delete_for_user
DELETE FROM starred_jobs WHERE id = :id AND user_id = :user_id;

--! delete_by_job (jobset_id?)
DELETE FROM starred_jobs WHERE user_id = :user_id AND project_id = :project_id
AND jobset_id IS NOT DISTINCT FROM :jobset_id AND job_name = :job_name;

--! delete_all_for_user
DELETE FROM starred_jobs WHERE user_id = :user_id;
