# keeping image memory bounded

0.2.0-dev.4 discarded decoded images without removing their gpui textures. one mac session reached a 19.2 gb physical footprint, with about 19 gb charged to graphics allocations. restarting only reset the leak.

0.2.0-dev.5 retires evicted render images after four display callbacks. gpui's metal, directx and blade allocators now return retired tile space to etagere. metal also removes retired lookup keys immediately. the vendored changes and upstream license are under `vendor/gpui`.

portraits, server icons, emoji and decorations have a separate 4 mib decoded allowance. message images have 12 mib. each pool has at most 128 completed entries, and downloads/decodes run two at a time. previews decode at bounded dimensions. these are cache budgets, not a promise that the whole application uses 16 mib.

## native graphics soak

run this as a normal binary, not a cargo example. examples inherit the gpui test-support dev dependency, which changes frame scheduling and does not represent a shipped build.

```sh
cargo run --release --features media-stress --bin media-stress
```

it generates 3,600 distinct 320x240 images, keeps a portrait on screen, uploads recent images and exercises the same cache/retirement path as the client. no account, credentials or network requests are used. sample the process with vmmap on macos or the platform's gpu/process memory tools. leave it idle afterward and check that memory returns instead of retaining the churned images.

on the development mac, the production-renderer soak churned 1,054 mib of decoded image data and returned to a 54.7 mib physical footprint, including 13.7 mib of charged graphics memory. its transient peak was 575.6 mib during the deliberately fast workload and minimize/restore check. these are synthetic test measurements, not whole-client idle figures or a long-session guarantee. windows and linux still need equivalent live gpu measurements.

## dev.6 working sets

visible images are protected from lru eviction during a repaint. identity and content soft allowances remain 4 mib and 12 mib; the visible working set has a hard ceiling of three times that allowance, and 256 entries per pool. requests/results and decoding concurrency stay bounded. offscreen items become eviction candidates on the next render. retirement also refreshes after the final tile removal. a regression holds 32 decoded 256px portraits (8 mib) through 40 repaints with identical render-image ids, then verifies reclamation after leaving that viewport.

animated decorations/profile layers are capped at 120 frames and 4 mib of decoded pixels per asset, with all frames counted in the cache budget. oversized animations fall back to a still. animation ids include the render-image identity, so returning from motion artwork cannot reuse a frame index from another image. the ordinary chat surface has no continuous cosmetic animation loop. active typing and open profiles may request frames; hidden/closed profiles do not.

## dev.8 loading and scrolling

portraits/icons/emoji now have four dedicated download workers. large attachments and cosmetic artwork use two other workers, with two shared blocking decoders. portrait requests time out after eight seconds, so one slow endpoint cannot hold both original download slots for twenty seconds. image requests remain public and credential-free, with the same host allowlist and response/decode limits.

the 64 mib, 512-file disk cache retains encoded images, keyed by a hash of the full url without the animation fragment. decoded eviction or an app restart can reuse those bytes without another network request. a still and motion variant share the encoded cache entry. writes are atomic; invalid decoded entries are removed, and old entries expire after 30 days. a visible failed request gets one delayed retry; repeated failures return to a long cooldown instead of creating a refresh loop.

cosmetic artwork no longer competes with portraits for the identity allowance. the identity soft limit is 8 mib / 256 entries, with a 24 mib / 512-entry visible hard limit; content remains 12 mib / 128 entries with a three-times / 256-entry visible ceiling. these remain decoded-cache limits, not whole-process memory guarantees.

member lists retain two previous views for up to 60 seconds. channels with the same verified visibility-list identity reuse the active sparse rows; updates for other identities are ignored. new guild subscriptions are activated before range requests. changing views moves the range away and back so discord sends a fresh sync even when it retained the old subscription. no empty per-channel range arrays are sent. each sparse list still has at most 300 rows, and only the current channel is subscribed. reselecting the current channel does not clear its list. server navigation remembers the last visited text channel. this does not yet replace the missing full per-account channel permission filter. no additional gateway polling loop is introduced.

super reaction particles and emoji motion run once for 1.1 seconds, then stop requesting frames. hover or a changed super count can replay the effect. disabling animations keeps the colored chip/count and removes the motion. the particle effect is independently drawn in gpui, not discord's proprietary nitro effect asset collection.
