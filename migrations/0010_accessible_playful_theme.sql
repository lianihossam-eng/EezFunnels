UPDATE themes
SET tokens_json = json_set(tokens_json, '$."color-primary"', '#c026d3'),
    updated_at = CURRENT_TIMESTAMP
WHERE id = 'playful'
    AND json_extract(tokens_json, '$."color-primary"') = '#d946ef';
