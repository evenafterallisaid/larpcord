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

discord's local web bundle routes sparse member updates by guild plus list id, rather than channel id. serein independently derives the same list identity from `member_list_id` or view-channel permission overwrites and reuses rows across matching channels. dev.8 now preserves that identity, ignores updates for a different list and explicitly clears the previous channel's subscribed ranges. unknown identities stay channel-specific. this prevents an unchanged shared list from turning into permanent placeholders when the server sends only a delta.

protocol references inspected: discord's downloaded november 8 web bundle, channel member store module 963307; serein's `crates/client-core/src/lib.rs` shared-member-list selection and `crates/model/src/permissions.rs`. no discord application code or nitro animation assets are bundled.
