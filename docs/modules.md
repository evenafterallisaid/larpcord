# native modules, api 1

modules are local, opt-in extensions inside larpcord. api 1 is a declarative host for message labels, semantic theme colors and opt-in native presentation features. it does not execute javascript, wasm, native libraries or shell commands. no runtime, jit, filesystem watcher or background polling is added. executable adapters can use this same registry and permission boundary in a later api.

open settings > modules > open folder. each subdirectory holds one `module.json`. reload modules discovers changes without restarting. the bundled **media labels** example is available without installing anything, disabled by default. local modules appear with their names, versions, descriptions and declared permissions. enabling stores the manifest's sha256 digest. any byte change requires enabling again. disabling immediately removes its hooks and grant. malformed modules are skipped with a visible error; they do not break chat.

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

these are new larpcord modules, not direct vencord/equicord compatibility. an adapter can reuse a plugin's ideas and pure logic, but a plugin that patches discord's javascript/react internals needs a native port. executable plugins, arbitrary ui panels, outbound event hooks, writable plugin state and automatic downloads are intentionally outside this first api.

## discover and install

settings has separate modules and themes catalogs. discover shows the curated larpcord entries and works offline with the bundled index. refresh catalog reads the public index from this repository. installed entries stay available across launches. install and update leave the entry disabled until you enable or apply it. deleting an installed catalog entry removes its saved permission grant. local manifests still load from the modules folder.

the catalog uses the same manifest validation as local modules. it contains data-only entries with message labels and theme tokens. submit a proposed entry in `catalog/index.json` for review. published catalog changes are available through refresh without rebuilding the client.

## native ports in discover

member count and show me your name adapt the behavior of the Vencord plugins. account panel server profile adapts the Equicord shortcut. booster count adapts the Illegalcord plugin for the server rail. they use Larpcord's existing state and render paths. [Vencord plugins](https://vencord.dev/plugins/), [Equicord](https://github.com/Equicord/Equicord), [Illegalcord](https://github.com/ImHisako/Illegalcord)

catppuccin mocha, nord and tokyo night map the upstream palettes onto Larpcord's theme colors. [Catppuccin](https://github.com/catppuccin/palette), [Nord](https://www.nordtheme.com/docs/colors-and-palettes), [Tokyo Night](https://github.com/enkia/tokyo-night-vscode-theme)
