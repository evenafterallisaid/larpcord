# local compatibility patch

xattr 0.2.3 from crates.io, compatibility changes: in src/sys/mod.rs, use linux ENODATA directly. libc removed the deprecated ENOATTR alias after 0.2.177. gpui 0.2.2 still depends on this xattr release, and native video audio output needs a newer libc. other platforms keep their existing constants. the macos ffi declarations also use u32 in place of the deprecated libc::uint32_t alias, preserving the exact abi. original licenses are preserved.
