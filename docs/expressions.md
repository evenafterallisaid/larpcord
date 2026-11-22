# profiles and expressions, dev.10

profiles group the name, username, pronouns, live presence, activity and badge artwork above the bio and role sections. a direct-message action opens the conversation without sending text. modal profiles keep focus isolation; the side inspector stays nonblocking. decorative layers do not determine the content layout.

the expression popover has gif, sticker and emoji tabs, search and a virtualized server/pack rail. emoji and sticker grids only request images for visible cells. custom emoji names come from ready guild metadata, live emoji/sticker updates or an explicit per-server catalog request. role restrictions, unavailable assets and known non-nitro restrictions are shown before insertion; discord remains authoritative about permissions and entitlement.

type an emoji name in the composer, such as `:sob:`, to expand it inline. partial names offer unicode and eligible server emoji. use the arrow keys to move and tab or enter to accept, or escape to keep the text. suggestions combine recent local selections and discord’s synced expression rankings. common synonyms such as wilted rose can help an emoji surface when its canonical name differs.

gif discovery uses discord's normal client relay to klipy. query text is encoded, limited to 100 characters and debounced for 350ms. a newer search aborts the previous request; closing the popover cancels it. discovery has a separate rate-limit lane so a slow catalog response cannot hold the message send lane. previews use the existing credential-free media worker with fixed provider image hosts and bounded decoding. no discord authorization header goes to a gif provider. previews remain still; custom emoji and raster sticker artwork can animate on hover while motion is enabled.

selecting an emoji inserts its markup at the cursor and records at most 32 recent items in the existing atomic settings file. selecting a gif inserts its provider page link. selecting a sticker stages one native sticker id for the next message. the user reviews the composer before sending. sticker-only messages and replies use the ordinary send path, with nonce enforcement and disabled mention pings. pending or failed sends do not clear a newer draft. sticker drafts follow channel changes and ambiguous failures are never retried automatically.

server sticker catalogs and standard packs are supported. png/apng/gif artwork uses the existing bounded decoder. lottie stickers can be selected and sent by native id, but their artwork still uses a named placeholder. the public sticker json was available during validation while discord's raster proxy returned 404, so no image preview is claimed for that format.

group dms use channel-icons artwork in the conversation list, unread shortcuts and header. groups without artwork use a group symbol. they no longer inherit a recipient's portrait, nameplate or display-name styling. the member panel uses group recipients plus the current user; partial channel updates and recipient add/remove events preserve that identity.

protocol references: discord developer emoji, sticker and create-message documentation, plus the pinned serein reference checkout's normal-client gif routes. no serein ui implementation or branding was imported.
