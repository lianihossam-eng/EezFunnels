#!/usr/bin/env python3

import pathlib
import sqlite3


ROOT = pathlib.Path(__file__).resolve().parents[1]
MIGRATION = "0020_public_form_delivery_field_contract.sql"


def apply_before(connection: sqlite3.Connection, migration_name: str) -> None:
    for migration in sorted((ROOT / "migrations").glob("*.sql")):
        if migration.name < migration_name:
            connection.executescript(migration.read_text(encoding="utf-8"))


def main() -> None:
    connection = sqlite3.connect(":memory:")
    apply_before(connection, MIGRATION)
    connection.execute(
        "INSERT INTO funnels (id, name, slug) VALUES ('pub010-funnel', 'PUB-010', 'pub010')"
    )
    connection.execute(
        "INSERT INTO webhook_destination_versions "
        "(destination_version_id, destination_url, created_at) "
        "VALUES ('11111111-1111-4111-8111-111111111111', "
        "'https://receiver.example.test/events', 1)"
    )
    connection.execute(
        "INSERT INTO public_form_delivery_mapping_versions "
        "(mapping_version_id, funnel_id, form_id, destination_version_id, "
        "selected_fields_json, created_at) VALUES "
        "('22222222-2222-4222-8222-222222222222', 'pub010-funnel', 'contact', "
        "'11111111-1111-4111-8111-111111111111', '[\"email\"]', 1)"
    )

    connection.executescript(
        (ROOT / "migrations" / MIGRATION).read_text(encoding="utf-8")
    )
    columns = {
        row[1]
        for row in connection.execute(
            "PRAGMA table_info(public_form_delivery_mapping_versions)"
        )
    }
    assert "selected_field_contract_json" in columns
    legacy_contract = connection.execute(
        "SELECT selected_field_contract_json FROM public_form_delivery_mapping_versions "
        "WHERE mapping_version_id = '22222222-2222-4222-8222-222222222222'"
    ).fetchone()[0]
    assert legacy_contract is None, legacy_contract

    connection.execute(
        "INSERT INTO public_form_delivery_mapping_versions "
        "(mapping_version_id, funnel_id, form_id, destination_version_id, "
        "selected_fields_json, selected_field_contract_json, created_at) VALUES "
        "('33333333-3333-4333-8333-333333333333', 'pub010-funnel', 'contact', "
        "'11111111-1111-4111-8111-111111111111', '[\"email\"]', "
        "'[{\"kind\":\"email\",\"id\":\"email\",\"name\":\"email\","
        "\"label\":\"Email\",\"required\":true}]', 2)"
    )
    large_contract = (
        '[{"kind":"select","id":"service","name":"service",'
        '"label":"Service","required":true,"options":['
        + ",".join(f'"{"x" * 600}-{index:02d}"' for index in range(40))
        + "]}]"
    )
    assert len(large_contract.encode("utf-8")) > 16_384
    connection.execute(
        "INSERT INTO public_form_delivery_mapping_versions "
        "(mapping_version_id, funnel_id, form_id, destination_version_id, "
        "selected_fields_json, selected_field_contract_json, created_at) "
        "VALUES (?, 'pub010-funnel', 'contact', "
        "'11111111-1111-4111-8111-111111111111', '[\"service\"]', ?, 2)",
        ("55555555-5555-4555-8555-555555555555", large_contract),
    )
    oversized_contract = '[{"kind":"text","id":"service","name":"service","label":"' + (
        "x" * 1_048_576
    ) + '"}]'
    for invalid_contract in ("not-json", "{}", oversized_contract):
        try:
            connection.execute(
                "INSERT INTO public_form_delivery_mapping_versions "
                "(mapping_version_id, funnel_id, form_id, destination_version_id, "
                "selected_fields_json, selected_field_contract_json, created_at) "
                "VALUES (?, 'pub010-funnel', 'contact', "
                "'11111111-1111-4111-8111-111111111111', '[\"email\"]', ?, 3)",
                (
                    f"44444444-4444-4444-8444-{len(invalid_contract):012d}",
                    invalid_contract,
                ),
            )
        except sqlite3.IntegrityError:
            continue
        raise AssertionError(f"invalid field contract was accepted: {invalid_contract[:40]!r}")

    try:
        connection.execute(
            "UPDATE public_form_delivery_mapping_versions "
            "SET selected_field_contract_json = '[]' "
            "WHERE mapping_version_id = '33333333-3333-4333-8333-333333333333'"
        )
    except sqlite3.IntegrityError:
        pass
    else:
        raise AssertionError("mapping field contracts must remain immutable")

    connection.close()
    print(
        "PUB-010 migration: legacy mappings require revalidation; field contracts are "
        "bounded, JSON-validated, and immutable"
    )


if __name__ == "__main__":
    main()
