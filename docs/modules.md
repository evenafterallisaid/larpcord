# native modules, api 1

modules are opt-in extensions inside larpcord. api 1 supports declarative message labels, theme colors, native presentation features and locally installed Wasm composer tools. the interpreter runs on demand on a background worker.

open Settings → Modules → Installed → Open folder. each subdirectory holds one `module.json`. Reload local files discovers changes without restarting. the bundled Media Labels and Trim Draft modules appear in Installed and start disabled. local modules appear with their names, versions, descriptions and declared permissions. a grant identifies the exact manifest bytes and, for a composer tool, the Wasm bytes. any byte change requires enabling again. disabling immediately removes the hooks and grant.

```json
{
  "api_version": 1,
  "id": "bot-notes",
  "name": "Bot notes",
  "version": "1.0.0",
  "description": "Label messages from bots in the local conversation view.",
  "permissions": ["message_metadata"],
  "message_decorators": [
    {"when": {"author_bot": true}, "label": "Bot message", "color": 10206463}
  ]
}
```

`message_decorators` is the `message.decorate` hook. conditions are combined with and. supported fields are `author_bot`, `has_attachments` and case-sensitive `contains`. metadata conditions require `message_metadata`. `contains` requires `message_content` and sees at most the first 4 kib of rendered message text. no token, credentials, private cache, raw gateway payload, network handle, send/reaction function or unrestricted gpui tree is exposed. decoration only changes local presentation. it never alters a message or sends anything to discord.

`theme` is the `theme.tokens` hook. it requires the `theme` permission. optional fields are `background`, `panel`, `sidebar`, `surface`, `border`, `text`, `muted` and `accent`, each a decimal rgb value from 0 to 16777215. overrides layer on the selected theme in deterministic module order. disable a module to return to the base colors. this is a semantic token adapter, not a css layout engine.

limits: 32 modules including the bundled example, 16 kib per manifest, 16 message rules per module, three labels per visible message, 32-byte labels and 128-byte text patterns. directory discovery inspects at most 128 entries. symlink manifests/directories, unknown fields, unsupported api versions and undeclared permissions are rejected. reload file work runs on a background executor. disabled modules do not evaluate hooks. native declarative evaluation is bounded and synchronous; any future executable backend must run away from painting, with independent cpu, memory and output limits.

vencord/equicord plugins that patch discord's javascript/react internals need a native port to use these hooks.

## composer tools

to try the bundled tool, enable **Trim draft** under Installed. write a draft, open the sparkle button in the composer, and choose **Trim draft**. select **Apply to draft** after reviewing the output. sending remains a separate action. changing channels, changing the draft or losing the module grant invalidates a pending proposal.

to install your own tool, add `"composer_tool": true` and `"permissions": ["composer_draft"]` to its `module.json` and put a compiled `composer.wasm` next to it. composer tools use only the draft capability and can be installed from local files. [Trim draft source and manifest](../examples/modules/trim-draft)

the Wasm ABI takes UTF-8 input at offset zero in a memory export named `memory`. export `transform(i32) -> i32`, where the argument is the input byte count and the return value is the output byte count, also at offset zero. input and output are bounded to 16 KiB. the worker gives each invocation a fresh instance with 2 MiB of linear memory and 500,000 interpreter fuel units. imports and start functions are rejected. tools receive only the selected draft text and return only replacement text. this follows Serein's bounded worker and review patterns with a Larpcord-specific ABI. [Serein's Wasm runtime](https://github.com/ViceVerse-cz/Serein/blob/bedb57c/crates/extensions/src/runtime.rs)

## discover and install

settings has separate modules and themes catalogs. discover shows the curated larpcord entries and works offline with the bundled index. refresh catalog reads the public index from this repository. installed entries stay available across launches. install and update leave the entry disabled until you enable or apply it. deleting an installed catalog entry removes its saved permission grant. local manifests still load from the modules folder.

the catalog uses the same manifest validation as local modules. it contains data-only entries with message labels and theme tokens. submit a proposed entry in `catalog/index.json` for review. published catalog changes are available through refresh without rebuilding the client.

## native ports in discover

member count and show me your name adapt the behavior of the Vencord plugins. account panel server profile adapts the Equicord shortcut. booster count adapts the Illegalcord plugin for the server rail. they use Larpcord's existing state and render paths. [Vencord plugins](https://vencord.dev/plugins/), [Equicord](https://github.com/Equicord/Equicord), [Illegalcord](https://github.com/ImHisako/Illegalcord)

catppuccin mocha, nord and tokyo night map the upstream palettes onto Larpcord's theme colors. [Catppuccin](https://github.com/catppuccin/palette), [Nord](https://www.nordtheme.com/docs/colors-and-palettes), [Tokyo Night](https://github.com/enkia/tokyo-night-vscode-theme)
