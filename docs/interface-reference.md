# the next interface pass

reference checkout: `ViceVerse-cz/Serein`, commit `bedb57c0c8def62e736e8a4f03e0a5d248b22126`, inspected alongside kawaicord's running mac client. serein is mit/apache-2.0 licensed. this is a design and interoperability reference, not a replacement frontend or an imported skin. larpcord keeps gpui, its masked home mark and its own assets.

the useful difference is consistency: one compact header rhythm, recognizable channel-type icons, aligned action groups, clipped member activity lines, restrained separators, and profile information grouped into distinct sections. adding isolated decorative controls does not supply those relationships.

## apply in larpcord

- keep the 48px conversation/header alignment and 32px action targets introduced in dev.8. constrain topic text before it can crowd the actions. preserve a complete channel name with a hover label.
- use channel-type symbols consistently in the sidebar and conversation header. announcement channels should not look like ordinary text channels. threads need their own symbol.
- keep the flat home mark and colored application icon. keep native mac traffic lights. retain the user's dark palette instead of forcing the reference's colors.
- keep message avatars, reply stems and text on the same columns. grouped messages share the content column; reply messages establish a new header. reactions sit below message content, not in a second card.
- member rows should have a consistent avatar/name/activity baseline. use real presence data and clip long activity strings. reserve cosmetic overlays without changing row geometry.
- profile cards should separate identity, bio, roles and membership dates. frame/effect layers must not take part in layout or intercept underlying controls.
- navigation, search, profile and member controls must perform their named action. calls, uploads and other unfinished features need implementations before their buttons are presented as working controls.

the reference's `crates/ui/src/lib.rs` (sidebar, channel header and composer), `design.rs`, `avatars/`, and profile modules are useful comparisons. translate behavior into larpcord's existing components; do not paste an egui view into gpui or reuse serein's branding.

## member list finding

discord's local web bundle routes sparse member updates by guild plus list id, rather than channel id. serein independently derives the same list identity from `member_list_id` or view-channel permission overwrites and reuses rows across matching channels. dev.8 now preserves that identity, ignores updates for a different list and refreshes the subscribed window with a valid nonempty range before restoring the visible range. a prior frame activates the guild subscription. empty per-channel range arrays are rejected by discord and are never sent. unknown identities stay channel-specific. this prevents an unchanged shared list from turning into permanent placeholders when the server sends only a delta.

protocol references inspected: discord's downloaded november 8 web bundle, channel member store module 963307; serein's `crates/client-core/src/lib.rs` shared-member-list selection and `crates/model/src/permissions.rs`. no discord application code or nitro animation assets are bundled.

## dev.9 structure

navigation now shares a single 36px title bar with the native mac traffic lights or custom windows/linux controls. header and composer icon actions use the same 32px targets. the conversation has a restrained top corner and border; channel and member labels share the same density and presence shapes. profile inspectors reserve a 320px column with a scaled 296px card, leaving the conversation interactive. full profiles and settings keep their modal focus barrier.

friends has online/all filters, real relationship/presence data, activity lines and an open-dm action. visiting home preserves the channel draft and does not acknowledge messages in the hidden conversation. ready supplemental presence data merges rather than clearing earlier entries. profile theme colors use a dark readable tint, with server profile colors taking precedence over global colors.

these changes are an interface foundation, not a claim of discord feature parity. voice, uploads and friend request management are still separate work. `--preview` or `LARPCORD_PREVIEW=1` starts an isolated sample workspace without reading the saved session or writing settings.

## dev.13 profile and motion pass

revisited serein's profile sections, stickers and `name_styles.rs` at the pinned reference above. the useful split is stable identity/navigation above a bounded content viewport, with decoration painted independently of layout. larpcord now fixes the tab rail and body viewport, removes position-changing card entrance motion and keeps hover styling paint-only. regression coverage crosses each tab with interface animations enabled and actual pointer movement.

serein currently draws the nitro style backdrops without moving them. larpcord adds its own gpui paint-time gradient sweeps, prism colors and glow pulses on profile/account names and hovered names. they do not change text geometry and stop when inactive or reduced motion is selected. these are native approximations of the effect ids, not discord's proprietary shaders. no official client code or shaders are copied.

animated identity assets previously used still-image urls and anonymous gpui image elements. `a_` avatar/banner assets now request gif motion in active contexts and have image ids so gpui can advance frames. oversized animations adapt their resolution under the existing 4 mib decoded asset budget instead of silently replacing a normal animation with one frame. row avatars animate on hover; profile banners animate while open. image links with discord's pure-image embed metadata render without the generic embed frame or duplicated standalone url. rich bot embeds, suppressed previews and surrounding user text are retained.

sticker recents read the separate fixed64 sticker ranking in frecency settings, not emoji keys or local sticker picks. missing sticker metadata is fetched on demand, at most 32 ids with two requests in flight. current-session choices precede synced ranking, with local-only history as fallback and duplicates removed. the public reverse-engineered schema is [FrecencyUserSettings](https://github.com/discord-userdoccers/discord-protos/blob/master/discord_protos/discord_users/v1/FrecencyUserSettings.proto), field 4 `sticker_frecency`; this interoperability route is unofficial. usage that another client has not synced cannot be imported. the rank read never writes settings.
