CREATE TABLE funnel_campaigns (
    funnel_id TEXT NOT NULL REFERENCES funnels(id) ON DELETE CASCADE,
    campaign_id TEXT NOT NULL
        CHECK (length(campaign_id) BETWEEN 1 AND 64)
        CHECK (substr(campaign_id, 1, 1) GLOB '[A-Za-z0-9]')
        CHECK (campaign_id NOT GLOB '*[^A-Za-z0-9_-]*'),
    PRIMARY KEY (funnel_id, campaign_id)
) WITHOUT ROWID;
