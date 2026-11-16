# native details and media, dev.11

comparison references: the pinned serein checkout at bedb57c0c8def62e736e8a4f03e0a5d248b22126, its timeline, profile, collectibles and normal-client settings routes, plus the user-provided kawaicord screenshots. ui code and branding remain larpcord's own implementation.

## implemented gaps

- labelled emoji/sticker sections, category jumps and scroll-synchronised rail selection. the variable-height list remains virtualized. a gpui scroll event previously reported the old visible range; the vendored fix reports the new range, covered by the picker scroll regression.
- recent emoji, sticker and gif selections. bounded local history survives restarts; remote favorites are separate and account-scoped.
- discord gif favorites import and explicit star changes via settings-proto/2. browsing only reads. every change starts from a complete fresh read, keeps unknown entries and settings fields, uses required_data_version and validates the returned change. partial/unknown layouts fail closed and are never written over. no automatic retry after an ambiguous write.
- separate profile about/friends/servers tabs. friends use friends' portraits, servers use server icons, both lists open real destinations. profile data requests mutual friend objects rather than only a count.
- current badge catalog enrichment is optional and time-bounded; only owned, visible badges with public raster artwork appear. existing profile badges remain available on catalog errors. no badge-id allowlist is required to draw new artwork.
- proper nameplate asset folders, right-aligned aspect-preserving art, palette tint and hover motion with a still fallback. small reply portraits no longer get oversized decorations. font effects distinguish gradients/prism/gummy from solid/neon/toon/pop/glow; halos and outlines use shaped text layers, not an idle animation loop. exact proprietary shader effects and unavailable fonts still use native approximations.
- date separators, a retained unread boundary, grouped-message hover timestamps, stronger unread rail pills and dm unread dots. status shapes have larger outlines and preserve distinct online/idle/dnd/offline shapes.
- a compact reply/copy/more toolbar contained within the message hit area. right-click opens a matching menu with reply, copy text/link/id and profile actions. no inert delete/pin/call controls are added.
- short profile and picker entry motion and typing dots. reduced motion disables motion. background-window media players pause. animations do not replace the independent gateway connection.
- image gif/apng autoplay and silent looping gifv playback, separate from actual video controls. at most four visible gifv decoders, one decoder thread each, two-frame queues, bounded output size, no audio devices. offscreen players are released. additional visible gifs retain still previews.
- portraits retain more cheap cache entries without raising decoded byte budgets. identity decoding has a dedicated permit so large animated media cannot take both decode slots.

## themes and future hooks

charcoal, onyx, midnight and rose share semantic background, panel, sidebar, surface, border, text, muted and accent tokens. custom theme data lives in theme.json next to settings.json and is loaded at launch; choose custom in settings. tokens are optional and inherit charcoal defaults. values are decimal 24-bit rgb integers. files are limited to 16 kib. this is the stable data boundary for future extension theme adapters, not a dynamic native-code loader. profile art, role colors and embed colors retain their own meaning.

example:

```json
{"background":1383467,"panel":856608,"sidebar":1054503,"surface":2437708,"border":3095639,"text":15922165,"muted":10132903,"accent":10206463}
```

## remaining parity work

lottie sticker artwork still uses a placeholder. proprietary discord gpu name effects are approximations. rich activity cards, full friends management, edit/delete/pin flows, reaction author lists, voice and uploads remain separate milestones. no full discord feature parity or whole-app ram reduction is claimed from this ui pass.
