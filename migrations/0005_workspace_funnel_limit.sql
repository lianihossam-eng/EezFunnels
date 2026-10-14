CREATE TRIGGER IF NOT EXISTS funnels_workspace_funnel_limit
BEFORE INSERT ON funnels
WHEN (SELECT COUNT(*) FROM funnels) >= 100
BEGIN
    SELECT RAISE(ABORT, 'workspace funnel limit reached');
END;
