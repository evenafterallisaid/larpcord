# conversation layout, dev.7

the conversation header spans the message pane and member list. server categories collapse, the composer has a searchable unicode emoji picker, and message hover controls provide replies and copying. replies include the supplied referenced message even when its original is outside the bounded history. a reply starts a group; the same author's following ordinary messages can continue it.

the member list subscribes only to the viewed server/channel. it requests the first 100 rows and the current 100-row viewport bucket, retaining at most 300 sparse rows. closing the list disables its member and activity subscription. this uses discord's client gateway subscription protocol, which can change independently of the public bot api.

ordinary and super reactions render as separate chips with their own counts and own-user state. gateway add/remove, remove-emoji and remove-all events update the cached timeline and live tail. changes arriving during a history request are retained. chips are display-only in this milestone; sending reactions and nitro burst animations remain future work.

png, apng and gif stickers use bounded public media decoding. lottie stickers show a named placeholder because there is no lottie renderer yet. animated media respects the animations setting. the latest button is an absolute overlay at the bottom of the timeline, with translucent gradient, border and readable text. it does not reserve a full-width bar or require a background blur render loop.

## protocol references

- [message and reaction structures](https://docs.discord.com/developers/resources/message)
- [reaction gateway events](https://docs.discord.com/developers/events/gateway-events#message-reaction-add)
- [sticker formats](https://docs.discord.com/developers/resources/sticker)
- [sticker cdn paths and gif host](https://docs.discord.com/developers/reference#image-formatting)

these describe payloads and public asset formats. layout and native controls are independently implemented in rust/gpui; no discord application bundle is shipped.
