//! Permission-scoped native module host.
mod wasm;

pub(crate) use wasm::MAX_DRAFT as MAX_TOOL_DRAFT;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_MODULES: usize = 32;
const MAX_MANIFEST: usize = 16 * 1024;

fn read_wasm(directory: &Path) -> Result<Arc<[u8]>, String> {
    let path = directory.join("composer.wasm");
    let meta = fs::symlink_metadata(&path).map_err(|_| "Missing composer.wasm")?;
    if !meta.is_file() || meta.len() > wasm::MAX_BINARY as u64 {
        return Err("Composer tool must be a regular Wasm file of at most 256 KiB".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| {
            file.take(wasm::MAX_BINARY as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| "Could not read composer.wasm")?;
    if bytes.len() > wasm::MAX_BINARY || !bytes.starts_with(b"\0asm\x01\0\0\0") {
        return Err("Invalid or oversized composer.wasm".into());
    }
    Ok(bytes.into())
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub id: String,
    pub digest: String,
}
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    MessageMetadata,
    MessageContent,
    Theme,
    NativePresentation,
    ComposerDraft,
}
impl Permission {
    pub fn description(self) -> &'static str {
        match self {
            Self::MessageMetadata => "Read message kind and attachment presence",
            Self::MessageContent => "Read visible message text (up to 4 KiB)",
            Self::Theme => "Change client theme colors",
            Self::NativePresentation => {
                "Show extra account, server or message details in the client"
            }
            Self::ComposerDraft => {
                "Read the current draft when you run this tool and propose an edit"
            }
        }
    }
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NativeFeature {
    MemberCount,
    Usernames,
    AccountServerProfile,
    BoosterCount,
    QuickMention,
    PinIcon,
    FastGifPicker,
}

impl NativeFeature {
    fn bit(self) -> u16 {
        1 << self as u16
    }
}
#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ThemePatch {
    background: Option<u32>,
    panel: Option<u32>,
    sidebar: Option<u32>,
    surface: Option<u32>,
    border: Option<u32>,
    text: Option<u32>,
    muted: Option<u32>,
    accent: Option<u32>,
}
impl ThemePatch {
    fn overlay(&mut self, other: &Self) {
        macro_rules! overlay {
            ($($field:ident),*) => {$(
                self.$field = other.$field.or(self.$field);
            )*};
        }
        overlay!(
            background, panel, sidebar, surface, border, text, muted, accent
        );
    }

    pub fn apply(&self, mut tokens: crate::theme::Tokens) -> crate::theme::Tokens {
        macro_rules! apply {($($field:ident),*)=>{$(if let Some(value)=self.$field {tokens.$field=value;})*}}
        apply!(
            background, panel, sidebar, surface, border, text, muted, accent
        );
        tokens
    }
}
#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct When {
    author_bot: Option<bool>,
    has_attachments: Option<bool>,
    contains: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Decorator {
    when: When,
    label: String,
    color: u32,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    api_version: u32,
    id: String,
    name: String,
    version: String,
    description: String,
    permissions: Vec<Permission>,
    #[serde(default)]
    message_decorators: Vec<Decorator>,
    #[serde(default)]
    native_features: Vec<NativeFeature>,
    #[serde(default)]
    composer_tool: bool,
    theme: Option<ThemePatch>,
}
impl Manifest {
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_MANIFEST {
            return Err("Manifest exceeds 16 KiB".into());
        }
        let m: Self = serde_json::from_slice(bytes)
            .map_err(|_| "Invalid module manifest or unsupported fields")?;
        let text = |s: &str, max: usize| {
            !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control)
        };
        if m.api_version != 1 {
            return Err("Unsupported module API version".into());
        }
        if m.id.is_empty()
            || m.id.len() > 48
            || !m
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || !text(&m.name, 64)
            || !text(&m.version, 32)
            || !text(&m.description, 320)
            || m.permissions.len() > 5
            || m.message_decorators.len() > 16
            || m.native_features.len() > 4
        {
            return Err("Invalid module identity or limits".into());
        }
        for rule in &m.message_decorators {
            let metadata = rule.when.author_bot.is_some() || rule.when.has_attachments.is_some();
            if !text(&rule.label, 32)
                || rule.color > 0xffffff
                || rule.when.contains.as_ref().is_some_and(|s| !text(s, 128))
                || (metadata && !m.permissions.contains(&Permission::MessageMetadata))
                || (rule.when.contains.is_some()
                    && !m.permissions.contains(&Permission::MessageContent))
                || (!metadata && rule.when.contains.is_none())
            {
                return Err("Message hook exceeds limits or lacks a declared permission".into());
            }
        }
        if let Some(theme) = &m.theme
            && (!m.permissions.contains(&Permission::Theme)
                || !theme.apply(Default::default()).valid())
        {
            return Err("Theme hook has invalid colors or lacks its permission".into());
        }
        if !m.native_features.is_empty()
            && (!m.permissions.contains(&Permission::NativePresentation) || m.theme.is_some())
        {
            return Err("Native features require their own presentation permission".into());
        }
        if m.composer_tool
            && (m.permissions.as_slice() != [Permission::ComposerDraft]
                || m.theme.is_some()
                || !m.message_decorators.is_empty()
                || !m.native_features.is_empty())
        {
            return Err(
                "A composer tool needs its own draft permission and cannot mix with other hooks"
                    .into(),
            );
        }
        Ok(m)
    }
}
pub struct Module {
    manifest: Manifest,
    wasm: Option<Arc<[u8]>>,
    pub digest: String,
    pub enabled: bool,
    pub from_catalog: bool,
}
impl Module {
    pub fn id(&self) -> &str {
        &self.manifest.id
    }
    pub fn name(&self) -> &str {
        &self.manifest.name
    }
    pub fn version(&self) -> &str {
        &self.manifest.version
    }
    pub fn description(&self) -> &str {
        &self.manifest.description
    }
    pub fn is_theme(&self) -> bool {
        self.manifest.theme.is_some() && self.manifest.message_decorators.is_empty()
    }

    pub fn preview_theme(&self, base: crate::theme::Tokens) -> crate::theme::Tokens {
        self.manifest
            .theme
            .as_ref()
            .map_or(base, |theme| theme.apply(base))
    }

    pub fn permissions(&self) -> &[Permission] {
        &self.manifest.permissions
    }

    pub fn is_composer_tool(&self) -> bool {
        self.wasm.is_some()
    }

    pub fn composer_tool(&self) -> Option<ComposerTool> {
        if !self.enabled {
            return None;
        }
        Some(ComposerTool {
            id: self.id().to_string(),
            name: self.name().to_string(),
            digest: self.digest.clone(),
            bytes: self.wasm.clone()?,
        })
    }
}

pub struct ComposerTool {
    pub id: String,
    pub name: String,
    pub digest: String,
    bytes: Arc<[u8]>,
}

impl ComposerTool {
    pub fn transform(&self, draft: &str) -> Result<String, &'static str> {
        wasm::transform(&self.bytes, draft)
    }
}
#[derive(Default)]
pub struct Registry {
    pub modules: Vec<Module>,
    pub errors: Vec<String>,
    features: u16,
    theme: ThemePatch,
}
/// The host gives modules only this presentation projection, never tokens or raw gateway data.
pub struct MessageView<'a> {
    pub author_bot: bool,
    pub has_attachments: bool,
    pub content: &'a str,
}
pub struct Label<'a> {
    pub text: &'a str,
    pub color: u32,
    pub module: &'a str,
}
impl Registry {
    pub fn feature(&self, feature: NativeFeature) -> bool {
        self.features & feature.bit() != 0
    }

    fn refresh_presentation(&mut self) {
        self.features = 0;
        self.theme = ThemePatch::default();
        for module in self.modules.iter().filter(|module| module.enabled) {
            for feature in &module.manifest.native_features {
                self.features |= feature.bit();
            }
            if let Some(theme) = &module.manifest.theme {
                self.theme.overlay(theme);
            }
        }
    }

    pub fn load(root: Option<&Path>, grants: &[Grant]) -> Self {
        let mut r = Self::default();
        r.add(
            include_bytes!("../examples/modules/media-labels/module.json"),
            grants,
        );
        r.add_local(
            include_bytes!("../examples/modules/trim-draft/module.json"),
            || {
                Ok(Arc::from(
                    &include_bytes!("../examples/modules/trim-draft/composer.wasm")[..],
                ))
            },
            grants,
        );
        let Some(root) = root else {
            return r;
        };
        let Ok(meta) = fs::symlink_metadata(root) else {
            return r;
        };
        if !meta.is_dir() || meta.file_type().is_symlink() {
            r.errors
                .push("Modules folder must be a real directory".into());
            return r;
        }
        let Ok(entries) = fs::read_dir(root) else {
            r.errors.push("Could not read modules folder".into());
            return r;
        };
        let mut entries = entries.take(128).filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if r.modules.len() >= MAX_MODULES {
                r.errors.push("Module limit reached (32)".into());
                break;
            }
            if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let file = entry.path().join("module.json");
            let Ok(meta) = fs::symlink_metadata(&file) else {
                continue;
            };
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_MANIFEST as u64
            {
                r.errors
                    .push("Skipped unsafe or oversized module manifest".into());
                continue;
            }
            let bytes = fs::File::open(file).and_then(|f| {
                let mut b = vec![];
                f.take(MAX_MANIFEST as u64 + 1)
                    .read_to_end(&mut b)
                    .map(|_| b)
            });
            match bytes {
                Ok(bytes) => r.add_local(&bytes, || read_wasm(&entry.path()), grants),
                Err(_) => r.errors.push("Could not read a module manifest".into()),
            }
        }
        r.refresh_presentation();
        r
    }
    pub fn load_with_catalog(
        root: Option<&Path>,
        grants: &[Grant],
        installed: &[CatalogEntry],
    ) -> Self {
        let mut registry = Self::load(root, grants);
        for entry in installed.iter().take(MAX_MODULES) {
            if let Err(error) = registry.install(entry, grants) {
                registry.errors.push(error);
            }
        }
        registry
    }
    pub fn install(&mut self, entry: &CatalogEntry, grants: &[Grant]) -> Result<(), String> {
        let mut module = entry.module()?;
        let existing = self.modules.iter().position(|m| m.id() == module.id());
        if existing.is_some_and(|index| !self.modules[index].from_catalog) {
            return Err(format!(
                "A local module already uses the id {}.",
                module.id()
            ));
        }
        if existing.is_none() && self.modules.len() >= MAX_MODULES {
            return Err("The installed module and theme limit is 32.".into());
        }
        module.enabled = grants
            .iter()
            .any(|g| g.id == module.id() && g.digest == module.digest);
        if let Some(index) = existing {
            self.modules[index] = module;
        } else {
            self.modules.push(module);
        }
        self.refresh_presentation();
        Ok(())
    }
    pub fn uninstall(&mut self, id: &str) {
        self.modules
            .retain(|module| !(module.from_catalog && module.id() == id));
        self.refresh_presentation();
    }

    fn add(&mut self, bytes: &[u8], grants: &[Grant]) {
        self.add_local(bytes, || Err("Missing composer.wasm".into()), grants);
    }

    fn add_local(
        &mut self,
        bytes: &[u8],
        wasm_bytes: impl FnOnce() -> Result<Arc<[u8]>, String>,
        grants: &[Grant],
    ) {
        let result = Manifest::parse(bytes).and_then(|manifest| {
            let binary = manifest.composer_tool.then(wasm_bytes).transpose()?;
            Ok((manifest, binary))
        });
        match result {
            Ok((manifest, wasm)) => {
                if self.modules.iter().any(|m| m.id() == manifest.id) {
                    self.errors
                        .push(format!("Duplicate module id: {}", manifest.id));
                    return;
                }
                let mut hasher = Sha256::new();
                hasher.update(bytes);
                if let Some(binary) = &wasm {
                    hasher.update([0]);
                    hasher.update(binary);
                }
                let digest = format!("{:x}", hasher.finalize());
                let enabled = grants
                    .iter()
                    .take(32)
                    .any(|g| g.id == manifest.id && g.digest == digest);
                self.modules.push(Module {
                    manifest,
                    wasm,
                    digest,
                    enabled,
                    from_catalog: false,
                });
            }
            Err(e) => {
                if self.errors.len() < 32 {
                    self.errors.push(e);
                }
            }
        }
    }
    pub fn set_enabled(&mut self, id: &str, enabled: bool, grants: &mut Vec<Grant>) {
        if let Some(module) = self.modules.iter_mut().find(|m| m.id() == id) {
            grants.retain(|g| g.id != id);
            if enabled {
                if grants.len() >= 32 {
                    grants.remove(0);
                }
                grants.push(Grant {
                    id: id.into(),
                    digest: module.digest.clone(),
                });
            }
            grants.truncate(32);
            module.enabled = enabled;
            self.refresh_presentation();
        }
    }
    pub fn tokens(&self, tokens: crate::theme::Tokens) -> crate::theme::Tokens {
        self.theme.apply(tokens)
    }
    pub fn decorate(&self, event: MessageView<'_>) -> Vec<Label<'_>> {
        let mut labels = vec![];
        // Slice only at a UTF-8 boundary; an enormous message never expands hook work.
        let mut end = event.content.len().min(4096);
        while !event.content.is_char_boundary(end) {
            end -= 1;
        }
        let content = &event.content[..end];
        for module in self.modules.iter().filter(|m| m.enabled) {
            for rule in &module.manifest.message_decorators {
                if rule.when.author_bot.is_none_or(|v| v == event.author_bot)
                    && rule
                        .when
                        .has_attachments
                        .is_none_or(|v| v == event.has_attachments)
                    && rule
                        .when
                        .contains
                        .as_ref()
                        .is_none_or(|v| content.contains(v))
                {
                    labels.push(Label {
                        text: &rule.label,
                        color: rule.color,
                        module: module.name(),
                    });
                    if labels.len() == 3 {
                        return labels;
                    }
                }
            }
        }
        labels
    }
}
const MAX_CATALOG_BYTES: usize = 256 * 1024;

#[derive(Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct CatalogEntry(serde_json::Value);

impl CatalogEntry {
    pub fn id(&self) -> &str {
        self.0["id"].as_str().unwrap_or_default()
    }
    pub fn module(&self) -> Result<Module, String> {
        let bytes = serde_json::to_vec(&self.0).map_err(|_| "Could not encode the manifest.")?;
        let manifest = Manifest::parse(&bytes)?;
        if manifest.composer_tool {
            return Err("Executable modules must be installed from local files.".into());
        }
        Ok(Module {
            manifest,
            wasm: None,
            digest: format!("{:x}", Sha256::digest(&bytes)),
            enabled: false,
            from_catalog: true,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    version: u32,
    pub entries: Vec<CatalogEntry>,
}

impl Catalog {
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_CATALOG_BYTES {
            return Err("The catalog exceeds its size limit.".into());
        }
        let catalog: Self = serde_json::from_slice(bytes).map_err(|_| "Invalid catalog.")?;
        if catalog.version != 1 || catalog.entries.len() > 64 {
            return Err("Unsupported catalog version or item count.".into());
        }
        let mut ids = HashSet::new();
        for entry in &catalog.entries {
            entry.module()?;
            if !ids.insert(entry.id()) {
                return Err("The catalog contains a duplicate id.".into());
            }
        }
        Ok(catalog)
    }
    pub fn bundled() -> Self {
        Self::parse(include_bytes!("../catalog/index.json"))
            .expect("bundled catalog must be valid")
            .with_native_features()
    }

    fn with_native_features(mut self) -> Self {
        let built_in = Self::parse(include_bytes!("../catalog/native-ui.json"))
            .expect("native feature catalog must be valid");
        self.entries.retain(|entry| {
            !built_in
                .entries
                .iter()
                .any(|native| native.id() == entry.id())
        });
        self.entries.truncate(64 - built_in.entries.len());
        self.entries.extend(built_in.entries);
        self
    }
    pub async fn refresh() -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(12))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Could not start catalog refresh.")?;
        let mut response = client.get("https://raw.githubusercontent.com/evenafterallisaid/larpcord/main/catalog/index.json")
            .send().await.and_then(reqwest::Response::error_for_status)
            .map_err(|_| "Could not reach the Larpcord catalog.")?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Catalog download was interrupted.")?
        {
            if bytes.len().saturating_add(chunk.len()) > MAX_CATALOG_BYTES {
                return Err("The catalog exceeds its size limit.".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        Self::parse(&bytes).map(Self::with_native_features)
    }
}

pub fn sanitize_catalog_entries(entries: &mut Vec<CatalogEntry>) {
    let mut seen = HashSet::new();
    entries.truncate(MAX_MODULES);
    entries.retain(|entry| entry.module().is_ok() && seen.insert(entry.id().to_string()));
}

pub fn directory() -> Option<PathBuf> {
    directories::ProjectDirs::from("app", "larpcord", "Larpcord")
        .map(|p| p.config_dir().join("modules"))
}
pub fn sanitize_grants(grants: &mut Vec<Grant>) {
    let mut seen = HashSet::new();
    grants.retain(|g| {
        g.id.len() <= 48
            && g.digest.len() == 64
            && g.digest.bytes().all(|b| b.is_ascii_hexdigit())
            && seen.insert(g.id.clone())
    });
    grants.truncate(32);
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composer_tool_grants_cover_the_executable_and_preview_its_output() {
        let directory = tempfile::tempdir().unwrap();
        let module = directory.path().join("fixture");
        fs::create_dir(&module).unwrap();
        let manifest = include_str!("../examples/modules/trim-draft/module.json")
            .replace("trim-draft", "local-trim");
        let original = include_bytes!("../examples/modules/trim-draft/composer.wasm");
        fs::write(module.join("module.json"), &manifest).unwrap();
        fs::write(module.join("composer.wasm"), original).unwrap();
        let mut registry = Registry::load(Some(directory.path()), &[]);
        let find = |registry: &Registry| {
            registry
                .modules
                .iter()
                .find(|m| m.id() == "local-trim")
                .unwrap()
                .composer_tool()
        };
        assert!(find(&registry).is_none());
        let mut grants = Vec::new();
        registry.set_enabled("local-trim", true, &mut grants);
        let tool = find(&registry).unwrap();
        assert_eq!(
            tool.transform("  \n🎉 keep this  \t\n").unwrap(),
            "🎉 keep this"
        );
        assert_eq!(tool.transform(" \t\n").unwrap(), "");
        assert!(tool.transform(&"x".repeat(MAX_TOOL_DRAFT + 1)).is_err());
        // A well-formed custom section changes the reviewed bytes without changing behavior.
        let mut modified = original.to_vec();
        modified.extend_from_slice(b"\0\x02\x01x");
        fs::write(module.join("composer.wasm"), modified).unwrap();
        assert!(find(&Registry::load(Some(directory.path()), &grants)).is_none());
        assert!(
            CatalogEntry(serde_json::from_str(&manifest).unwrap())
                .module()
                .is_err()
        );
    }

    #[test]
    fn native_features_wait_for_an_explicit_grant() {
        let catalog = Catalog::bundled();
        let entry = catalog
            .entries
            .iter()
            .find(|entry| entry.id() == "member-count")
            .unwrap();
        let mut registry = Registry::load(None, &[]);
        let mut grants = Vec::new();
        registry.install(entry, &grants).unwrap();
        assert!(!registry.feature(NativeFeature::MemberCount));
        registry.set_enabled("member-count", true, &mut grants);
        assert!(registry.feature(NativeFeature::MemberCount));
        assert!(
            Registry::load_with_catalog(None, &grants, std::slice::from_ref(entry))
                .feature(NativeFeature::MemberCount)
        );
        registry.set_enabled("member-count", false, &mut grants);
        assert!(!registry.feature(NativeFeature::MemberCount));
    }
    #[test]
    fn resolved_theme_preserves_precedence_through_disable_and_uninstall() {
        let entry = |id: &str, accent| {
            CatalogEntry(serde_json::json!({
                "api_version": 1, "id": id, "name": id, "version": "1", "description": id,
                "permissions": ["theme"], "theme": {"accent": accent}
            }))
        };
        let mut registry = Registry::load(None, &[]);
        let mut grants = vec![];
        for (id, accent) in [("first", 0xaa6633), ("second", 0x3366aa)] {
            registry.install(&entry(id, accent), &grants).unwrap();
            registry.set_enabled(id, true, &mut grants);
        }
        let base = crate::theme::Tokens::default();
        assert_eq!(registry.tokens(base).accent, 0x3366aa);
        registry.set_enabled("second", false, &mut grants);
        assert_eq!(registry.tokens(base).accent, 0xaa6633);
        registry.uninstall("first");
        assert_eq!(registry.tokens(base).accent, base.accent);
    }

    #[test]
    fn catalog_update_requires_a_new_grant_and_cannot_replace_local_modules() {
        let mut entries = Catalog::bundled().entries;
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry.module().unwrap().is_theme())
                .count(),
            6
        );
        let entry = entries.remove(0);
        let mut registry = Registry::load(None, &[]);
        let mut grants = Vec::new();
        registry.install(&entry, &grants).unwrap();
        registry.set_enabled(entry.id(), true, &mut grants);
        let loaded = Registry::load_with_catalog(None, &grants, std::slice::from_ref(&entry));
        assert!(
            loaded
                .modules
                .iter()
                .any(|m| m.id() == entry.id() && m.enabled)
        );
        let mut changed = entry.clone();
        changed.0["version"] = serde_json::json!("2.0.0");
        registry.install(&changed, &grants).unwrap();
        assert!(
            !registry
                .modules
                .iter()
                .find(|m| m.id() == entry.id())
                .unwrap()
                .enabled
        );
        changed.0["id"] = serde_json::json!("media-labels");
        assert!(registry.install(&changed, &[]).is_err());
        registry.uninstall(entry.id());
        assert!(!registry.modules.iter().any(|m| m.id() == entry.id()));
    }

    #[test]
    fn grants_are_scoped_to_reviewed_bytes_and_hooks_stay_local() {
        let mut r = Registry::load(None, &[]);
        let mut grants = vec![];
        let event = || MessageView {
            author_bot: false,
            has_attachments: true,
            content: "a message",
        };
        assert!(r.decorate(event()).is_empty());
        r.set_enabled("media-labels", true, &mut grants);
        assert_eq!(r.decorate(event())[0].text, "Media");
        assert!(Registry::load(None, &grants).modules[0].enabled);
        let bytes =
            include_str!("../examples/modules/media-labels/module.json").replace("1.0.0", "1.0.1");
        let mut changed = Registry::default();
        changed.add(bytes.as_bytes(), &grants);
        assert!(
            !changed.modules[0].enabled,
            "changing any manifest bytes requires a new enable action"
        );
        r.set_enabled("media-labels", false, &mut grants);
        assert!(r.decorate(event()).is_empty());
        grants = (0..32)
            .map(|n| Grant {
                id: format!("removed-{n}"),
                digest: "a".repeat(64),
            })
            .collect();
        r.set_enabled("media-labels", true, &mut grants);
        assert_eq!(grants.len(), 32);
        assert!(
            Registry::load(None, &grants).modules[0].enabled,
            "a full grant list must retain the newly approved module"
        );
        let bad = bytes.replace("\"has_attachments\": true", "\"contains\": \"secret\"");
        assert!(
            Manifest::parse(bad.as_bytes()).is_err(),
            "text hooks require content permission"
        );
        assert!(
            Manifest::parse(
                bytes
                    .replace("\"api_version\": 1", "\"api_version\": 90")
                    .as_bytes()
            )
            .is_err()
        );
    }
    #[test]
    fn theme_hooks_restore_base_tokens_and_bound_outputs() {
        let bytes = include_bytes!("../examples/modules/soft-lavender/module.json");
        let mut r = Registry::default();
        r.add(bytes, &[]);
        let mut grants = vec![];
        let base = crate::theme::Tokens::default();
        assert_eq!(r.tokens(base).accent, base.accent);
        r.set_enabled("soft-lavender", true, &mut grants);
        assert_eq!(r.tokens(base).accent, 12700141);
        r.set_enabled("soft-lavender", false, &mut grants);
        assert_eq!(r.tokens(base).accent, base.accent);
        let invalid = std::str::from_utf8(bytes)
            .unwrap()
            .replace("12700141", "4294967295");
        assert!(Manifest::parse(invalid.as_bytes()).is_err());
        let no_permission = std::str::from_utf8(bytes)
            .unwrap()
            .replace("[\"theme\"]", "[]");
        assert!(Manifest::parse(no_permission.as_bytes()).is_err());
    }
    #[test]
    fn folder_loader_rejects_symlinks_duplicates_and_oversize() {
        let dir = tempfile::tempdir().unwrap();
        let m = dir.path().join("test");
        fs::create_dir(&m).unwrap();
        fs::write(m.join("module.json"), vec![b' '; MAX_MANIFEST + 1]).unwrap();
        let r = Registry::load(Some(dir.path()), &[]);
        assert_eq!(r.modules.len(), Registry::load(None, &[]).modules.len());
        assert!(!r.errors.is_empty());
        fs::write(
            m.join("module.json"),
            include_bytes!("../examples/modules/media-labels/module.json"),
        )
        .unwrap();
        assert!(
            Registry::load(Some(dir.path()), &[])
                .errors
                .iter()
                .any(|e| e.contains("Duplicate"))
        );
        #[cfg(unix)]
        {
            fs::remove_file(m.join("module.json")).unwrap();
            std::os::unix::fs::symlink("/etc/hosts", m.join("module.json")).unwrap();
            assert!(
                Registry::load(Some(dir.path()), &[])
                    .errors
                    .iter()
                    .any(|e| e.contains("unsafe"))
            );
        }
    }
}
