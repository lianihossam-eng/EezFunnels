#!/usr/bin/env python3

import binascii
import hashlib
import http.client
import json
import os
import pathlib
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time
import zlib


ROOT = pathlib.Path(__file__).resolve().parents[1]
WRANGLER = ROOT / "node_modules" / ".bin" / "wrangler"


def png_chunk(kind: bytes, data: bytes) -> bytes:
    checksum = binascii.crc32(kind + data) & 0xFFFFFFFF
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", checksum)


def source_png_with_private_metadata() -> bytes:
    signature = b"\x89PNG\r\n\x1a\n"
    header = struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)
    pixel = zlib.compress(b"\x00\xff\x00\x00\xff")
    return (
        signature
        + png_chunk(b"IHDR", header)
        + png_chunk(b"tEXt", b"Comment\x00private GPS location")
        + png_chunk(b"IDAT", pixel)
        + png_chunk(b"IEND", b"")
    )


def main() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]

    with tempfile.TemporaryDirectory(prefix="eezfunnel-v2-media-") as directory:
        state = pathlib.Path(directory)
        persist = state / "persist"
        log_path = state / "wrangler.log"
        wrangler_environment = os.environ.copy()
        wrangler_environment["XDG_CONFIG_HOME"] = str(state / "xdg-config")
        wrangler_environment["XDG_CACHE_HOME"] = str(state / "xdg-cache")
        wrangler_environment["WRANGLER_SEND_METRICS"] = "false"
        subprocess.run(
            [
                str(WRANGLER),
                "d1",
                "migrations",
                "apply",
                "DB",
                "--local",
                "--env",
                "dev",
                "--persist-to",
                str(persist),
            ],
            cwd=ROOT,
            check=True,
            timeout=120,
            stdout=subprocess.DEVNULL,
            env=wrangler_environment,
        )
        with log_path.open("w", encoding="utf-8") as log:
            worker = subprocess.Popen(
                [
                    str(WRANGLER),
                    "dev",
                    "--local",
                    "--env",
                    "dev",
                    "--port",
                    str(port),
                    "--ip",
                    "127.0.0.1",
                    "--var",
                    f"ALLOWED_HOSTS:localhost:{port}",
                    "--persist-to",
                    str(persist),
                    "--log-level",
                    "error",
                ],
                cwd=ROOT,
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
                env=wrangler_environment,
            )

            def request(method: str, path: str, body=None, headers=None):
                connection = http.client.HTTPConnection("127.0.0.1", port, timeout=30)
                request_headers = {"Host": f"localhost:{port}"}
                request_headers.update(headers or {})
                connection.request(method, path, body=body, headers=request_headers)
                response = connection.getresponse()
                content = response.read()
                status = response.status
                response_headers = {key.lower(): value for key, value in response.getheaders()}
                connection.close()
                return status, content, response_headers

            def post_json(path: str, value, key: str):
                return request(
                    "POST",
                    path,
                    json.dumps(value, separators=(",", ":")).encode(),
                    {"Content-Type": "application/json", "Idempotency-Key": key},
                )

            def wait_for_worker():
                deadline = time.monotonic() + 300
                while time.monotonic() < deadline:
                    if worker.poll() is not None:
                        raise RuntimeError("Local Worker exited during media smoke startup")
                    try:
                        if request("GET", "/health")[0] == 200:
                            return
                    except OSError:
                        pass
                    time.sleep(0.5)
                raise RuntimeError("Local Worker did not start for media smoke")

            def mint_and_save(funnel_id: str, base_revision: str, document_bytes: bytes, key: str) -> str:
                digest = hashlib.sha256(document_bytes).hexdigest()
                status, body, _ = post_json(
                    f"/api/funnels/{funnel_id}/stages-v2",
                    {
                        "base_revision_id": base_revision,
                        "expected_length": len(document_bytes),
                        "expected_sha256": digest,
                    },
                    f"{key}-stage",
                )
                assert status == 201, (status, body)
                stage_id = json.loads(body)["stage_id"]
                upload_status, upload_body, _ = request(
                    "PUT",
                    f"/api/funnels/{funnel_id}/stages-v2",
                    document_bytes,
                    {
                        "Authorization": f"Bearer {stage_id}",
                        "Content-Type": "application/octet-stream",
                    },
                )
                assert upload_status == 204, (upload_status, upload_body)
                save_status, save_body, _ = post_json(
                    f"/api/funnels/{funnel_id}/revisions-v2",
                    {
                        "base_revision_id": base_revision,
                        "stage_id": stage_id,
                        "expected_length": len(document_bytes),
                        "expected_sha256": digest,
                    },
                    f"{key}-save",
                )
                assert save_status == 201, (save_status, save_body)
                return json.loads(save_body)["revision_id"]

            try:
                wait_for_worker()
                source = source_png_with_private_metadata()
                upload_headers = {
                    "Content-Type": "application/octet-stream",
                    "X-File-Name": "%2E%2E%2Ffamily%20photo.png",
                }
                initial_capacity_status, initial_capacity_body, _ = request(
                    "GET", "/api/media/capacity"
                )
                assert initial_capacity_status == 200, (
                    initial_capacity_status,
                    initial_capacity_body,
                )
                status, body, _ = request("POST", "/api/media", source, upload_headers)
                assert status == 201, (status, body)
                media = json.loads(body)["item"]
                asset_id = media["descriptor"]["asset_id"]
                assert media["display_name"] == "family photo.png", media
                assert media["descriptor"]["format"] == "png", media
                assert (media["descriptor"]["width"], media["descriptor"]["height"]) == (1, 1)

                owner_status, normalized, owner_headers = request(
                    "GET", f"/api/media/{asset_id}/content"
                )
                assert owner_status == 200, (owner_status, normalized)
                assert owner_headers.get("content-type") == "image/png", owner_headers
                assert b"private GPS location" not in normalized
                assert b"tEXt" not in normalized
                assert normalized != source

                duplicate_status, duplicate_body, _ = request(
                    "POST", "/api/media", source, upload_headers
                )
                assert duplicate_status == 201, (duplicate_status, duplicate_body)
                duplicate = json.loads(duplicate_body)["item"]
                assert duplicate["descriptor"]["asset_id"] == asset_id, duplicate

                capacity_status, capacity_body, _ = request("GET", "/api/media/capacity")
                assert capacity_status == 200, (capacity_status, capacity_body)
                capacity = json.loads(capacity_body)["capacity"]
                assert capacity["enabled"] is True, capacity
                assert capacity["keys_spent"] == 1, capacity
                assert capacity["bytes_spent"] == len(normalized), capacity

                invalid_status, invalid_body, _ = request(
                    "POST",
                    "/api/media",
                    b"<svg xmlns='http://www.w3.org/2000/svg'></svg>",
                    upload_headers,
                )
                assert invalid_status == 400, (invalid_status, invalid_body)

                create_status, create_body, _ = post_json(
                    "/api/funnels/v2",
                    {"name": "Media smoke", "blueprint": "lead_gen"},
                    "v2-media-create",
                )
                assert create_status == 201, (create_status, create_body)
                created = json.loads(create_body)
                funnel = created["funnel"]
                base_revision = created["initial_revision"]["id"]
                funnel_id = funnel["id"]
                status, document_body, _ = request(
                    "GET",
                    f"/api/funnels/{funnel_id}/document?revision_id={base_revision}",
                )
                assert status == 200, document_body
                document_json = document_body.decode()
                edit_status, edit_body, _ = post_json(
                    f"/api/funnels/{funnel_id}/edit-v2",
                    {
                        "base_revision_id": base_revision,
                        "page_id": "page-home",
                        "document_json": document_json,
                        "operation_id": "d" * 32,
                        "edit": {
                            "operation": "add_block",
                            "section": "section-hero",
                            "after": "block-hero-copy",
                            "block": {
                                "kind": "image",
                                "asset_id": asset_id,
                                "alt": {"kind": "text", "value": "A red pixel"},
                            },
                        },
                    },
                    "v2-media-edit",
                )
                assert edit_status == 200, (edit_status, edit_body)
                edited = json.loads(edit_body)
                assert f"/api/media/{asset_id}/content" in edited["preview_html"]
                edited_document = edited["document_json"].encode()
                saved_revision = mint_and_save(
                    funnel_id, base_revision, edited_document, "v2-media-image"
                )

                unpublished_status, unpublished_body, _ = request(
                    "GET", f"/media/{asset_id}"
                )
                assert unpublished_status == 404, (unpublished_status, unpublished_body)

                publish_status, publish_body, _ = post_json(
                    f"/api/funnels/{funnel_id}/publish",
                    {
                        "revision_id": saved_revision,
                        "expected_published_revision_id": None,
                    },
                    "v2-media-publish",
                )
                assert publish_status == 200, (publish_status, publish_body)
                page_status, page_body, _ = request("GET", f"/f/{funnel['slug']}")
                assert page_status == 200, page_body
                assert f"/media/{asset_id}".encode() in page_body
                public_status, public_body, public_headers = request("GET", f"/media/{asset_id}")
                assert public_status == 200, (public_status, public_body)
                assert public_body == normalized
                assert public_headers.get("cache-control") == "public, max-age=31536000, immutable"

                archive_status, archive_body, _ = post_json(
                    f"/api/media/{asset_id}/archive", {"archived": True}, "v2-media-archive"
                )
                assert archive_status == 200, (archive_status, archive_body)
                archived_status, archived_body, _ = request(
                    "GET", "/api/media?archived=true"
                )
                assert archived_status == 200, (archived_status, archived_body)
                assert any(
                    item["descriptor"]["asset_id"] == asset_id
                    for item in json.loads(archived_body)["items"]
                )
                still_published_status, still_published_body, _ = request(
                    "GET", f"/media/{asset_id}"
                )
                assert still_published_status == 200, still_published_body

                unpublish_status, unpublish_body, _ = post_json(
                    f"/api/funnels/{funnel_id}/unpublish",
                    {"expected_published_revision_id": saved_revision},
                    "v2-media-unpublish",
                )
                assert unpublish_status == 200, (unpublish_status, unpublish_body)
                removed_public_status, removed_public_body, _ = request(
                    "GET", f"/media/{asset_id}"
                )
                assert removed_public_status == 404, (removed_public_status, removed_public_body)
            except Exception:
                log.flush()
                print(log_path.read_text(encoding="utf-8", errors="replace"), file=sys.stderr)
                raise
            finally:
                try:
                    os.killpg(worker.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    worker.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(worker.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    worker.wait(timeout=5)

    print("V2 media Worker smoke: normalized upload, deduplication, capacity, revision binding, publication gate and archive passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
