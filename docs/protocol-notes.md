# native profiles, typing and components

checked against discord's public web client on 2026-11-13. the owning asset was `https://discord.com/assets/web.f25377cf5b911919.js`. this is field and protocol inspection, with an independent rust implementation. no discord implementation code or account payloads are included here.

- extended profiles use `/users/{id}/profile`, optionally with `guild_id`. basic user and guild member requests remain fallbacks.
- current collectibles have a public static asset at `https://cdn.discordapp.com/media/v1/collectibles-shop/{sku_id}/static`. legacy decorations use `avatar-decoration-presets/{asset}.png` with `passthrough=false` for a static image. the client keeps profile cosmetics static in this milestone.
- legacy effect definitions can be resolved through collectible product/category metadata. missing or retired catalog entries can still prevent an old effect from resolving. those paths need live account coverage across more cosmetics.
- viewed-server subscriptions use gateway opcode 37 with a subscriptions map. larpcord enables typing for the viewed server, disables the previous one, and does not request activity or member-list streams. reconnects restore the selected subscription.
- component interactions use the active gateway session id and the message's application, channel and message ids. larpcord submits them only in response to an explicit control click. message updates replace the component tree without discarding a newer live edit.

bot help rendering follows discord's [component reference](https://docs.discord.com/developers/components/reference). profiles and roles follow the [user](https://docs.discord.com/developers/resources/user) and [guild](https://docs.discord.com/developers/resources/guild) schemas. markdown is parsed with [pulldown-cmark](https://docs.rs/pulldown-cmark/0.13.6/pulldown_cmark/).

headless tests exercise typed models, history races, sends, modal focus, profile response isolation and typing state. sample-window checks cover rendering. those checks do not prove that every live bot control, retired cosmetic or guild configuration works. bot forms and entity selectors remain outside this milestone.
