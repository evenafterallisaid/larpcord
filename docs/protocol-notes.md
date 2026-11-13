# native profiles, typing and components

checked against discord's public web client on 2026-11-13. the owning asset was `https://discord.com/assets/web.637e82960de4dc33.js`. this is field and protocol inspection, with an independent rust implementation. no discord implementation code or account payloads are included here.

- extended profiles use `/users/{id}/profile`, optionally with `guild_id`. basic user and guild member requests remain fallbacks.
- current collectibles have a public static asset at `https://cdn.discordapp.com/media/v1/collectibles-shop/{sku_id}/static`. legacy decorations use `avatar-decoration-presets/{asset}.png` with `passthrough=false` for a static image. the animated variant is used only for active profile/hover artwork, with bounded gif/apng/webp decoding and a still fallback for oversized animations.
- frame metadata uses `inner_width`, overflow dimensions and ordered staple/rail/border layers. frame assets include the layer id between the sku and format. server-specific profile collectibles override global selections by type.
- name styles use `display_name_styles`, nameplates use `collectibles.nameplate`, and typing uses `typing_indicator_style`. styles are rendered from received fields; there are no entitlement changes or cosmetic purchase controls.
- server folders/order come from preloaded user settings (protobuf root field 14), with a bounded parser and legacy json support. read states use ready entries, gateway acknowledgements and debounced read acknowledgements only for the active, visible latest conversation.
- legacy effect definitions can be resolved through collectible product/category metadata. missing or retired catalog entries can still prevent an old effect from resolving. those paths need live account coverage across more cosmetics.
- viewed-server subscriptions use gateway opcode 37 with a subscriptions map. larpcord enables typing for the viewed server, disables the previous one, and does not request activity or member-list streams. reconnects restore the selected subscription.
- component interactions use the active gateway session id and the message's application, channel and message ids. larpcord submits them only in response to an explicit control click. message updates replace the component tree without discarding a newer live edit.

bot help rendering follows discord's [component reference](https://docs.discord.com/developers/components/reference). profiles and roles follow the [user](https://docs.discord.com/developers/resources/user) and [guild](https://docs.discord.com/developers/resources/guild) schemas. markdown is parsed with [pulldown-cmark](https://docs.rs/pulldown-cmark/0.13.6/pulldown_cmark/).

headless tests exercise typed models, history races, sends, modal focus, profile response isolation and typing state. sample-window checks cover rendering. those checks do not prove that every live bot control, retired cosmetic or guild configuration works. bot forms and entity selectors remain outside this milestone.

cosmetic behavior references: [display-name styles](https://support.discord.com/hc/en-us/articles/33833879643927-Discord-Display-Name-Styles-FAQ), [profile frames](https://support.discord.com/hc/en-us/articles/40775065582615-Profile-Frames-FAQ), [nameplates](https://support.discord.com/hc/en-us/articles/30408457944215-Nameplates-FAQ), [custom typing indicators](https://support.discord.com/hc/en-us/articles/42962943077271-Custom-Typing-Indicator-FAQ). exact proprietary text shaders and every retired catalog asset are not covered.
