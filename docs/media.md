# Media library

The dashboard's **Media library** stores reusable images in the Worker-bound private R2 bucket. Typed V2 image blocks reference a content-addressed asset and an explicit text alternative; decorative images can use an empty alternative. A document can reference up to 32 distinct images.

Uploads are limited to 2 MiB and accept decoded JPEG and PNG images. SVG, GIF, animated PNG, and other formats are rejected. Images are decoded with dimension and allocation limits, EXIF orientation is applied, and the pixels are re-encoded before storage. This removes EXIF, GPS, and ancillary metadata. The stored representation is limited to 2048 pixels per side, 4,194,304 pixels total, and 4 MiB. MIME types and dimensions come from the normalized bytes rather than request headers.

The normalized bytes' SHA-256 digest is the asset ID. Uploading identical normalized bytes reuses the same asset and private R2 object. Asset descriptors become immutable when ready. Old `assets` table rows from before this feature are marked unverified and cannot be selected or served; upload those images again to create verified assets.

`Archive` hides an image from the active picker. It does not remove the R2 object or break a published page. The Worker serves `/media/:asset_id` only when a currently published immutable revision references that asset. Owner previews use `/api/media/:asset_id/content`; the media dashboard and `/api/media` routes should be protected by Cloudflare Access or another perimeter on an Internet-facing instance. There is no physical-delete operation while immutable revisions can reference old media.

## R2 capacity

`MEDIA_R2_BYTE_BUDGET` and `MEDIA_R2_KEY_BUDGET` bound media writes. The checked-in production defaults are 1 GiB and 10,000 charged object-write slots; local development uses 100 MiB and 1,000 slots. Operators can increase these values when needed. The R2 control object persists outside D1, and its stored limits only increase. Lowering configuration beneath the stored limits fails closed.

Before a new R2 object write, the Worker atomically charges its normalized byte length and one write slot in `private/media-control/sequence-v1`. Failed writes, retries after uncertain failures, and D1 restores do not refund charges. Uploading content already present in R2 does not create another charge. The media dashboard shows the configured allowance, used bytes, and remaining write slots.

## HTTP routes

| Method and path | Behavior |
|:---|:---|
| `GET /api/media` | List active or archived library entries in pages of 50 |
| `POST /api/media` | Upload raw `application/octet-stream` JPEG/PNG bytes; pass the encoded original filename in `X-File-Name` |
| `POST /api/media/:asset_id/archive` | Archive or restore an asset with `{"archived":true}` or `false` |
| `GET /api/media/:asset_id/content` | Read a ready image for owner previews |
| `GET /api/media/capacity` | Read the configured R2 media allowance |
| `GET /media/:asset_id` | Read an image only while a current published revision references it |

The bucket is private; application routes set the MIME type, `nosniff`, and cache policy. Public content is immutable and digest-addressed. Local Worker checks do not validate Cloudflare production bindings or account limits.
