ALTER TABLE public_form_delivery_mapping_versions
    ADD COLUMN selected_field_contract_json TEXT
    CHECK (
        selected_field_contract_json IS NULL
        OR (
            json_valid(selected_field_contract_json)
            AND json_type(selected_field_contract_json) = 'array'
            AND length(CAST(selected_field_contract_json AS BLOB)) <= 1048576
        )
    );
