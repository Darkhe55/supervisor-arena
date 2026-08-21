-- M2 fix: ensure bootstrap rows are present in discipline_weights
-- after the initial migration. The original migration 13 ran the
-- INSERT in the same `batch_execute` as the CREATE TABLE; on
-- testcontainers' PG 11 (used by integration tests) the INSERT
-- silently inserted 0 rows because the testcontainers image has a
-- stricter plan-stability check for new tables in the same batch.
-- Splitting the bootstrap into its own migration makes the issue
-- reproducible / fixable from the schema alone, no app-code change.
--
-- This migration is idempotent (ON CONFLICT DO NOTHING).

INSERT INTO discipline_weights (discipline, dim, weight)
SELECT d.code, dim.code, 1.0 / 6.0
FROM disciplines d
CROSS JOIN rating_dimensions dim
WHERE d.is_active AND dim.is_active
ON CONFLICT (discipline, dim) DO NOTHING;

INSERT INTO discipline_weight_history (discipline, dim, old_weight, new_weight, action, actor_id)
SELECT d.code, dim.code, NULL, 1.0 / 6.0, 'applied', NULL
FROM disciplines d
CROSS JOIN rating_dimensions dim
WHERE d.is_active AND dim.is_active
ON CONFLICT DO NOTHING;
