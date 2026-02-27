//! # Git transport over SSH — smart protocol implementation
//!
//! This module implements the [Git smart transfer protocol][smart] (v1) over SSH,
//! providing the low-level fetch and push operations that power TypedNotes' Git
//! sync feature. It communicates with a standard Git remote (GitHub, GitLab, any
//! SSH-accessible server) by spawning `ssh` subprocesses that invoke
//! `git-upload-pack` (fetch) and `git-receive-pack` (push).
//!
//! [smart]: https://git-scm.com/docs/pack-protocol
//!
//! ## Threading model
//!
//! All public functions are **blocking** — they spawn an SSH child process and
//! perform synchronous I/O on its stdin/stdout. Callers in the async server
//! functions (see [`crate`]) wrap them in [`tokio::task::spawn_blocking`].
//!
//! ## Storage
//!
//! Objects are read from and written to a [`store::MemoryStore`] via its
//! synchronous accessors (`get_sync`, `put_sync`, `set_ref_sync`). This avoids
//! the need for an async runtime inside the blocking task and keeps the entire
//! fetch/push cycle self-contained in memory.
//!
//! ## Public API
//!
//! | Function | Git command | Description |
//! |----------|-------------|-------------|
//! | [`fetch`] | `git-upload-pack` | Downloads all refs and objects from the remote into the `MemoryStore`. Negotiates wants (all advertised refs), receives a packfile via sideband-64k, parses it, and sets `HEAD` to the requested branch. |
//! | [`push`] | `git-receive-pack` | Sends locally-created objects to the remote. Builds a minimal packfile containing only new objects, sends a ref-update command, and verifies the `report-status` response. |
//!
//! ## Internal structure
//!
//! The rest of the module is organised into helper sections:
//!
//! - **SSH helpers** — `parse_ssh_url` (SCP-like and `ssh://` formats), `write_ssh_key`
//!   (temp file with `0600` permissions), `ssh_opts` (strict host-key checking disabled
//!   for headless operation).
//! - **pkt-line protocol** — `read_pkt_line`, `write_pkt_line`, `write_pkt_flush` for
//!   the length-prefixed framing used by the Git wire protocol.
//! - **Ref advertisement parsing** — reads the initial ref list + capabilities sent by
//!   the remote.
//! - **Pack file parsing (fetch)** — `parse_pack` handles version-2/3 packs with
//!   non-delta objects (commit, tree, blob, tag), `OFS_DELTA`, and `REF_DELTA` entries,
//!   including zlib decompression and delta application.
//! - **Pack file building (push)** — `build_pack` serialises a set of objects into a
//!   valid version-2 pack with a trailing SHA-1 checksum.
//! - **Delta application** — `apply_delta` implements the copy/insert instruction set
//!   defined by the Git delta format.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::process::{Command, Stdio};

use store::objects::Sha;
use store::MemoryStore;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Fetch all objects and refs from a remote into the given [`MemoryStore`].
///
/// After a successful fetch the store's `HEAD` ref points at the remote's
/// default branch tip. If `branch` is provided, `refs/heads/{branch}` is
/// preferred for HEAD resolution; otherwise falls back to `main` or `master`.
pub fn fetch(
    store: &MemoryStore,
    remote_url: &str,
    ssh_key_pem: &str,
    branch: Option<&str>,
) -> Result<(), String> {
    let (user, host, path) = parse_ssh_url(remote_url)?;
    let key_file = write_ssh_key(ssh_key_pem)?;

    let mut child = Command::new("ssh")
        .args(ssh_opts(key_file.path()))
        .arg(format!("{user}@{host}"))
        .arg(format!("git-upload-pack '{path}'"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("ssh spawn: {e}"))?;

    let mut reader = child.stdout.take().unwrap();
    let mut writer = child.stdin.take().unwrap();

    // 1. Read ref advertisements
    let (refs, _caps) = read_ref_advertisement(&mut reader)?;

    if refs.is_empty() {
        // Empty remote repository — nothing to fetch.
        drop(writer);
        drop(reader);
        let _ = child.wait();
        return Ok(());
    }

    // 2. Send wants (all advertised refs)
    let unique_shas: HashSet<&Sha> = refs.values().collect();
    let mut first = true;
    for sha in &unique_shas {
        let line = if first {
            first = false;
            format!("want {} side-band-64k no-progress ofs-delta\n", sha.to_hex())
        } else {
            format!("want {}\n", sha.to_hex())
        };
        write_pkt_line(&mut writer, line.as_bytes())?;
    }
    write_pkt_flush(&mut writer)?;

    // 3. No "have" lines (fresh store) → send done
    write_pkt_line(&mut writer, b"done\n")?;
    writer.flush().map_err(|e| format!("flush: {e}"))?;

    // 4. Read NAK
    let _nak = read_pkt_line(&mut reader)?;

    // 5. Read pack via sideband-64k
    let mut pack_data = Vec::new();
    loop {
        match read_pkt_line(&mut reader)? {
            None => break, // flush = end of data
            Some(data) if data.is_empty() => continue,
            Some(data) => match data[0] {
                1 => pack_data.extend_from_slice(&data[1..]),
                2 => {} // progress – ignore
                3 => {
                    let msg = String::from_utf8_lossy(&data[1..]);
                    return Err(format!("Remote error: {msg}"));
                }
                _ => {}
            },
        }
    }

    drop(writer);
    drop(reader);
    let status = child.wait().map_err(|e| format!("wait: {e}"))?;
    if !status.success() {
        return Err(format!(
            "git-upload-pack exited with code {}",
            status.code().unwrap_or(-1)
        ));
    }

    // 6. Parse pack into store
    if !pack_data.is_empty() {
        parse_pack(store, &pack_data)?;
    }

    // 7. Store refs & set HEAD
    let mut head_sha: Option<Sha> = None;
    for (refname, sha) in &refs {
        store.set_ref_sync(refname, sha);
        if refname == "HEAD" {
            head_sha = Some(sha.clone());
        }
    }
    // Prefer the user-specified branch for HEAD resolution
    if let Some(b) = branch {
        let branch_ref = format!("refs/heads/{b}");
        if let Some(sha) = refs.get(&branch_ref) {
            head_sha = Some(sha.clone());
        }
    }
    // Fallback: HEAD from refs/heads/main or master
    if head_sha.is_none() {
        head_sha = refs
            .get("refs/heads/main")
            .or_else(|| refs.get("refs/heads/master"))
            .cloned();
    }
    if let Some(sha) = head_sha {
        store.set_ref_sync("HEAD", &sha);
    }

    Ok(())
}

/// Push objects from the [`MemoryStore`] to the remote.
///
/// `branch` is the branch name to update (e.g. `"main"`).
/// `new_object_shas` lists the hex SHA strings of objects created locally
/// (after the last fetch) that must be sent to the remote.
pub fn push(
    store: &MemoryStore,
    remote_url: &str,
    ssh_key_pem: &str,
    branch: &str,
    new_object_shas: &[String],
) -> Result<(), String> {
    let head = store
        .get_ref_sync("HEAD")
        .ok_or_else(|| "No HEAD — nothing to push".to_string())?;

    let (user, host, path) = parse_ssh_url(remote_url)?;
    let key_file = write_ssh_key(ssh_key_pem)?;

    let mut child = Command::new("ssh")
        .args(ssh_opts(key_file.path()))
        .arg(format!("{user}@{host}"))
        .arg(format!("git-receive-pack '{path}'"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("ssh spawn: {e}"))?;

    let mut reader = child.stdout.take().unwrap();
    let mut writer = child.stdin.take().unwrap();

    // 1. Read ref advertisements from receive-pack
    let (refs, _caps) = read_ref_advertisement(&mut reader)?;

    let refname = format!("refs/heads/{branch}");
    let old_sha = refs
        .get(&refname)
        .cloned()
        .unwrap_or(Sha([0u8; 20])); // null SHA for new branch

    if old_sha == head {
        // Nothing to push — remote is already up to date.
        drop(writer);
        drop(reader);
        let _ = child.wait();
        return Ok(());
    }

    // 2. Send ref-update command
    let update_line = format!(
        "{} {} {}\0 report-status\n",
        old_sha.to_hex(),
        head.to_hex(),
        refname
    );
    write_pkt_line(&mut writer, update_line.as_bytes())?;
    write_pkt_flush(&mut writer)?;

    // 3. Build and send pack (only new objects)
    let pack = build_pack(store, new_object_shas)?;
    writer
        .write_all(&pack)
        .map_err(|e| format!("write pack: {e}"))?;
    writer.flush().map_err(|e| format!("flush: {e}"))?;
    drop(writer); // close stdin → signal EOF

    // 4. Read report-status
    let mut unpack_ok = false;
    loop {
        match read_pkt_line(&mut reader)? {
            None => break,
            Some(line) => {
                let s = String::from_utf8_lossy(&line);
                if s.starts_with("unpack ok") {
                    unpack_ok = true;
                } else if s.starts_with("ng ") {
                    return Err(format!("Push rejected: {s}"));
                }
            }
        }
    }

    drop(reader);
    let status = child.wait().map_err(|e| format!("wait: {e}"))?;
    if !status.success() {
        return Err(format!(
            "git-receive-pack exited with code {}",
            status.code().unwrap_or(-1)
        ));
    }
    if !unpack_ok {
        return Err("Push failed: server did not confirm unpack".to_string());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// SSH helpers
// ---------------------------------------------------------------------------

fn ssh_opts(key_path: &std::path::Path) -> Vec<String> {
    vec![
        "-i".into(),
        key_path.to_string_lossy().into_owned(),
        "-o".into(),
        "IdentitiesOnly=yes".into(),
        "-o".into(),
        "StrictHostKeyChecking=no".into(),
        "-o".into(),
        "UserKnownHostsFile=/dev/null".into(),
        "-o".into(),
        "BatchMode=yes".into(),
    ]
}

/// Parse an SSH URL into `(user, host, repo_path)`.
///
/// Supported formats:
/// - SCP-like: `git@github.com:user/repo.git`
/// - URL:      `ssh://git@github.com/user/repo.git`
fn parse_ssh_url(url: &str) -> Result<(String, String, String), String> {
    if let Some(rest) = url.strip_prefix("ssh://") {
        // ssh://user@host[:port]/path
        let (user_host, path) = rest
            .split_once('/')
            .ok_or_else(|| format!("Invalid SSH URL (no path): {url}"))?;
        let (user, host_port) = if user_host.contains('@') {
            let (u, h) = user_host.split_once('@').unwrap();
            (u.to_string(), h.to_string())
        } else {
            ("git".to_string(), user_host.to_string())
        };
        // Strip port if present
        let host = host_port
            .split_once(':')
            .map(|(h, _)| h.to_string())
            .unwrap_or(host_port);
        Ok((user, host, format!("/{path}")))
    } else if url.contains(':') && !url.contains("://") {
        // SCP-like: user@host:path
        let (user_host, path) = url
            .split_once(':')
            .ok_or_else(|| format!("Invalid SSH URL: {url}"))?;
        let (user, host) = if user_host.contains('@') {
            let (u, h) = user_host.split_once('@').unwrap();
            (u.to_string(), h.to_string())
        } else {
            ("git".to_string(), user_host.to_string())
        };
        Ok((user, host, path.to_string()))
    } else {
        Err(format!("Unsupported URL format: {url}"))
    }
}

/// Write the PEM key to a temp file with mode 0600. The file is deleted when
/// the returned `NamedTempFile` is dropped.
///
/// Normalises the key text first: strips `\r`, trims whitespace, and ensures a
/// trailing newline — SSH is very picky about PEM formatting.
fn write_ssh_key(ssh_key_pem: &str) -> Result<tempfile::NamedTempFile, String> {
    let normalised = ssh_key_pem.replace('\r', "");
    let normalised = normalised.trim();
    let mut tmp =
        tempfile::NamedTempFile::new().map_err(|e| format!("create key tempfile: {e}"))?;
    tmp.write_all(normalised.as_bytes())
        .map_err(|e| format!("write key: {e}"))?;
    tmp.write_all(b"\n")
        .map_err(|e| format!("write trailing newline: {e}"))?;
    tmp.flush().map_err(|e| format!("flush key: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("chmod key: {e}"))?;
    }
    Ok(tmp)
}

// ---------------------------------------------------------------------------
// pkt-line protocol
// ---------------------------------------------------------------------------

/// Read one pkt-line. Returns `None` for a flush packet (`0000`).
fn read_pkt_line(reader: &mut impl Read) -> Result<Option<Vec<u8>>, String> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(format!("read pkt-line length: {e}")),
    }
    let len_str =
        std::str::from_utf8(&len_buf).map_err(|e| format!("pkt-line length not utf8: {e}"))?;
    let len =
        u16::from_str_radix(len_str, 16).map_err(|e| format!("pkt-line length parse: {e}"))?;

    if len <= 1 {
        // 0000 = flush, 0001 = delimiter
        return Ok(None);
    }

    let data_len = (len as usize).saturating_sub(4);
    let mut data = vec![0u8; data_len];
    reader
        .read_exact(&mut data)
        .map_err(|e| format!("read pkt-line data: {e}"))?;
    Ok(Some(data))
}

fn write_pkt_line(writer: &mut impl Write, data: &[u8]) -> Result<(), String> {
    let len = data.len() + 4;
    write!(writer, "{len:04x}").map_err(|e| format!("write pkt-line len: {e}"))?;
    writer
        .write_all(data)
        .map_err(|e| format!("write pkt-line data: {e}"))?;
    Ok(())
}

fn write_pkt_flush(writer: &mut impl Write) -> Result<(), String> {
    writer
        .write_all(b"0000")
        .map_err(|e| format!("write flush: {e}"))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Ref advertisement parsing
// ---------------------------------------------------------------------------

/// Read the ref advertisement sent by upload-pack / receive-pack.
/// Returns `(refs: HashMap<refname, sha>, capabilities: String)`.
fn read_ref_advertisement(
    reader: &mut impl Read,
) -> Result<(HashMap<String, Sha>, String), String> {
    let mut refs = HashMap::new();
    let mut caps = String::new();
    let mut first = true;

    loop {
        match read_pkt_line(reader)? {
            None => break, // flush
            Some(line) => {
                let line_str = String::from_utf8_lossy(&line);
                let line_str = line_str.trim_end_matches('\n');

                if first {
                    first = false;
                    // First line may contain capabilities after \0
                    if let Some(nul) = line_str.find('\0') {
                        caps = line_str[nul + 1..].to_string();
                        let ref_part = &line_str[..nul];
                        if let Some((sha_hex, refname)) = ref_part.split_once(' ') {
                            if let Some(sha) = Sha::from_hex(sha_hex) {
                                refs.insert(refname.to_string(), sha);
                            }
                        }
                    } else if let Some((sha_hex, refname)) = line_str.split_once(' ') {
                        if let Some(sha) = Sha::from_hex(sha_hex) {
                            refs.insert(refname.to_string(), sha);
                        }
                    }
                } else if let Some((sha_hex, refname)) = line_str.split_once(' ') {
                    if let Some(sha) = Sha::from_hex(sha_hex) {
                        refs.insert(refname.to_string(), sha);
                    }
                }
            }
        }
    }

    Ok((refs, caps))
}

// ---------------------------------------------------------------------------
// Pack file parsing (fetch)
// ---------------------------------------------------------------------------

/// Parse a git pack and store every object in the [`MemoryStore`].
fn parse_pack(store: &MemoryStore, data: &[u8]) -> Result<(), String> {
    if data.len() < 12 {
        return Err("Pack data too short for header".to_string());
    }
    if &data[0..4] != b"PACK" {
        return Err("Invalid pack signature".to_string());
    }
    let version = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    if version != 2 && version != 3 {
        return Err(format!("Unsupported pack version {version}"));
    }
    let num_objects = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);

    // Map from pack-offset → (type_name, decompressed_content) for delta bases
    let mut resolved: HashMap<usize, (&'static str, Vec<u8>)> = HashMap::new();
    let mut offset = 12usize;

    for _i in 0..num_objects {
        let entry_offset = offset;

        // Read type (3 bits) + size (variable-length)
        let first = *data
            .get(offset)
            .ok_or_else(|| "Pack truncated at entry header".to_string())?;
        offset += 1;
        let obj_type = (first >> 4) & 0x07;
        let mut size: u64 = (first & 0x0f) as u64;
        let mut shift = 4u32;

        let mut byte = first;
        while byte & 0x80 != 0 {
            byte = *data
                .get(offset)
                .ok_or_else(|| "Pack truncated in size varint".to_string())?;
            offset += 1;
            size |= ((byte & 0x7f) as u64) << shift;
            shift += 7;
        }

        match obj_type {
            // Non-delta types: commit=1, tree=2, blob=3, tag=4
            1 | 2 | 3 | 4 => {
                let type_name = match obj_type {
                    1 => "commit",
                    2 => "tree",
                    3 => "blob",
                    4 => "tag",
                    _ => unreachable!(),
                };
                let (decompressed, consumed) = zlib_decompress(&data[offset..], size as usize)?;
                offset += consumed;

                let sha = store_git_object(store, type_name, &decompressed);
                let _ = sha; // used implicitly via store
                resolved.insert(entry_offset, (type_name, decompressed));
            }

            // OFS_DELTA (6)
            6 => {
                // Read negative offset to base
                let mut byte = *data
                    .get(offset)
                    .ok_or_else(|| "Pack truncated in ofs-delta offset".to_string())?;
                offset += 1;
                let mut base_offset_val: u64 = (byte & 0x7f) as u64;
                while byte & 0x80 != 0 {
                    byte = *data
                        .get(offset)
                        .ok_or_else(|| "Pack truncated in ofs-delta offset cont".to_string())?;
                    offset += 1;
                    base_offset_val = ((base_offset_val + 1) << 7) | (byte & 0x7f) as u64;
                }
                let abs_base_offset = entry_offset
                    .checked_sub(base_offset_val as usize)
                    .ok_or_else(|| "OFS_DELTA offset underflow".to_string())?;

                let (delta, consumed) = zlib_decompress(&data[offset..], size as usize)?;
                offset += consumed;

                let (base_type, base_data) = resolved
                    .get(&abs_base_offset)
                    .ok_or_else(|| {
                        format!("OFS_DELTA base at offset {abs_base_offset} not yet resolved")
                    })?;

                let result = apply_delta(base_data, &delta)?;
                store_git_object(store, base_type, &result);
                resolved.insert(entry_offset, (base_type, result));
            }

            // REF_DELTA (7)
            7 => {
                if offset + 20 > data.len() {
                    return Err("Pack truncated in ref-delta base SHA".to_string());
                }
                let mut base_sha_bytes = [0u8; 20];
                base_sha_bytes.copy_from_slice(&data[offset..offset + 20]);
                offset += 20;
                let base_sha = Sha(base_sha_bytes);

                let (delta, consumed) = zlib_decompress(&data[offset..], size as usize)?;
                offset += consumed;

                let base_full = store.get_sync(&base_sha).ok_or_else(|| {
                    format!("REF_DELTA base {} not found in store", base_sha.to_hex())
                })?;
                let (base_type, base_content) = split_git_object(&base_full)?;

                let result = apply_delta(base_content, &delta)?;
                store_git_object(store, base_type, &result);
                resolved.insert(entry_offset, (leak_str(base_type), result));
            }

            _ => return Err(format!("Unknown pack object type {obj_type}")),
        }
    }

    Ok(())
}

/// Decompress zlib data returning `(decompressed_bytes, bytes_consumed_from_input)`.
fn zlib_decompress(compressed: &[u8], expected_size: usize) -> Result<(Vec<u8>, usize), String> {
    let mut decoder = flate2::read::ZlibDecoder::new(compressed);
    let mut out = Vec::with_capacity(expected_size);
    decoder
        .read_to_end(&mut out)
        .map_err(|e| format!("zlib decompress: {e}"))?;
    let consumed = decoder.total_in() as usize;
    Ok((out, consumed))
}

/// Store a git object (with standard header) in the MemoryStore and return its SHA.
fn store_git_object(store: &MemoryStore, type_name: &str, content: &[u8]) -> Sha {
    let header = format!("{type_name} {}\0", content.len());
    let mut full = Vec::with_capacity(header.len() + content.len());
    full.extend_from_slice(header.as_bytes());
    full.extend_from_slice(content);
    let sha = sha1_of(&full);
    store.put_sync(&sha, full);
    sha
}

/// Split a stored git object (with header) into `(type_name, content)`.
fn split_git_object(raw: &[u8]) -> Result<(&str, &[u8]), String> {
    let nul = raw
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| "Object missing NUL header terminator".to_string())?;
    let header = std::str::from_utf8(&raw[..nul]).map_err(|e| format!("header utf8: {e}"))?;
    let (type_name, _size_str) = header
        .split_once(' ')
        .ok_or_else(|| "Invalid object header".to_string())?;
    Ok((type_name, &raw[nul + 1..]))
}

/// Leak a `&str` so that it lives for `'static`. Only used for the small set
/// of git type names ("commit", "tree", "blob", "tag") during pack parsing.
fn leak_str(s: &str) -> &'static str {
    match s {
        "commit" => "commit",
        "tree" => "tree",
        "blob" => "blob",
        "tag" => "tag",
        other => Box::leak(other.to_string().into_boxed_str()),
    }
}

// ---------------------------------------------------------------------------
// Pack file building (push)
// ---------------------------------------------------------------------------

/// Build a minimal pack containing the objects identified by `sha_hexes`.
fn build_pack(store: &MemoryStore, sha_hexes: &[String]) -> Result<Vec<u8>, String> {
    let mut pack = Vec::new();

    // Header
    pack.extend_from_slice(b"PACK");
    pack.extend_from_slice(&2u32.to_be_bytes()); // version 2
    pack.extend_from_slice(&(sha_hexes.len() as u32).to_be_bytes());

    for sha_hex in sha_hexes {
        let sha = Sha::from_hex(sha_hex)
            .ok_or_else(|| format!("Invalid SHA hex: {sha_hex}"))?;
        let full = store
            .get_sync(&sha)
            .ok_or_else(|| format!("Object {sha_hex} not in store"))?;
        let (type_name, content) = split_git_object(&full)?;

        let type_num: u8 = match type_name {
            "commit" => 1,
            "tree" => 2,
            "blob" => 3,
            "tag" => 4,
            _ => return Err(format!("Cannot pack type {type_name}")),
        };

        // Encode type + size varint
        encode_pack_entry_header(&mut pack, type_num, content.len());

        // Zlib-compress the content
        let mut encoder =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(content)
            .map_err(|e| format!("zlib encode: {e}"))?;
        let compressed = encoder.finish().map_err(|e| format!("zlib finish: {e}"))?;
        pack.extend_from_slice(&compressed);
    }

    // Trailing SHA-1 checksum of everything so far
    let checksum = sha1_of(&pack);
    pack.extend_from_slice(&checksum.0);

    Ok(pack)
}

/// Write the type+size header for a pack entry.
fn encode_pack_entry_header(buf: &mut Vec<u8>, type_num: u8, size: usize) {
    // First byte: CTTTSSSS  (C=continuation, T=type, S=size bits 0-3)
    let mut first = (type_num << 4) | (size as u8 & 0x0f);
    let mut remaining = size >> 4;
    if remaining > 0 {
        first |= 0x80;
    }
    buf.push(first);

    while remaining > 0 {
        let mut byte = (remaining & 0x7f) as u8;
        remaining >>= 7;
        if remaining > 0 {
            byte |= 0x80;
        }
        buf.push(byte);
    }
}

// ---------------------------------------------------------------------------
// Delta application
// ---------------------------------------------------------------------------

fn apply_delta(base: &[u8], delta: &[u8]) -> Result<Vec<u8>, String> {
    let mut pos = 0;

    // Source (base) size
    let (base_size, consumed) = read_size_varint(delta, pos)?;
    pos += consumed;
    if base_size as usize != base.len() {
        return Err(format!(
            "Delta base size mismatch: header says {base_size}, actual {}",
            base.len()
        ));
    }

    // Target size
    let (target_size, consumed) = read_size_varint(delta, pos)?;
    pos += consumed;

    let mut result = Vec::with_capacity(target_size as usize);

    while pos < delta.len() {
        let cmd = delta[pos];
        pos += 1;

        if cmd & 0x80 != 0 {
            // Copy from base
            let mut copy_off: u32 = 0;
            let mut copy_len: u32 = 0;

            if cmd & 0x01 != 0 {
                copy_off |= delta[pos] as u32;
                pos += 1;
            }
            if cmd & 0x02 != 0 {
                copy_off |= (delta[pos] as u32) << 8;
                pos += 1;
            }
            if cmd & 0x04 != 0 {
                copy_off |= (delta[pos] as u32) << 16;
                pos += 1;
            }
            if cmd & 0x08 != 0 {
                copy_off |= (delta[pos] as u32) << 24;
                pos += 1;
            }

            if cmd & 0x10 != 0 {
                copy_len |= delta[pos] as u32;
                pos += 1;
            }
            if cmd & 0x20 != 0 {
                copy_len |= (delta[pos] as u32) << 8;
                pos += 1;
            }
            if cmd & 0x40 != 0 {
                copy_len |= (delta[pos] as u32) << 16;
                pos += 1;
            }

            if copy_len == 0 {
                copy_len = 0x10000; // Special case per git spec
            }

            let start = copy_off as usize;
            let end = start + copy_len as usize;
            if end > base.len() {
                return Err(format!(
                    "Delta copy out of bounds: {start}..{end} in base of len {}",
                    base.len()
                ));
            }
            result.extend_from_slice(&base[start..end]);
        } else if cmd > 0 {
            // Insert literal bytes
            let n = cmd as usize;
            if pos + n > delta.len() {
                return Err("Delta insert goes past end of delta data".to_string());
            }
            result.extend_from_slice(&delta[pos..pos + n]);
            pos += n;
        } else {
            return Err("Delta: reserved instruction byte 0".to_string());
        }
    }

    if result.len() != target_size as usize {
        return Err(format!(
            "Delta result size mismatch: expected {target_size}, got {}",
            result.len()
        ));
    }

    Ok(result)
}

/// Read a variable-length size (used in delta header).
fn read_size_varint(data: &[u8], start: usize) -> Result<(u64, usize), String> {
    let mut value: u64 = 0;
    let mut shift = 0u32;
    let mut pos = start;

    loop {
        if pos >= data.len() {
            return Err("Size varint extends past end of data".to_string());
        }
        let byte = data[pos];
        pos += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    Ok((value, pos - start))
}

// ---------------------------------------------------------------------------
// SHA-1
// ---------------------------------------------------------------------------

fn sha1_of(data: &[u8]) -> Sha {
    let hash = sha1_smol::Sha1::from(data).digest();
    Sha(hash.bytes())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ssh_url_scp() {
        let (user, host, path) = parse_ssh_url("git@github.com:user/repo.git").unwrap();
        assert_eq!(user, "git");
        assert_eq!(host, "github.com");
        assert_eq!(path, "user/repo.git");
    }

    #[test]
    fn test_parse_ssh_url_full() {
        let (user, host, path) =
            parse_ssh_url("ssh://git@github.com/user/repo.git").unwrap();
        assert_eq!(user, "git");
        assert_eq!(host, "github.com");
        assert_eq!(path, "/user/repo.git");
    }

    #[test]
    fn test_parse_ssh_url_with_port() {
        let (user, host, path) =
            parse_ssh_url("ssh://deploy@myhost.com:2222/srv/repos/notes.git").unwrap();
        assert_eq!(user, "deploy");
        assert_eq!(host, "myhost.com");
        assert_eq!(path, "/srv/repos/notes.git");
    }

    #[test]
    fn test_pkt_line_roundtrip() {
        let mut buf = Vec::new();
        write_pkt_line(&mut buf, b"hello\n").unwrap();
        write_pkt_flush(&mut buf).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let line = read_pkt_line(&mut cursor).unwrap();
        assert_eq!(line, Some(b"hello\n".to_vec()));
        let flush = read_pkt_line(&mut cursor).unwrap();
        assert_eq!(flush, None);
    }

    #[test]
    fn test_encode_pack_entry_header_small() {
        let mut buf = Vec::new();
        encode_pack_entry_header(&mut buf, 3, 10); // blob, size 10
        // type=3 → bits 0110_0000 shifted: 0011_0000, size low 4 bits = 1010 → 0011_1010 = 0x3a
        assert_eq!(buf, vec![0x3a]);
    }

    #[test]
    fn test_encode_pack_entry_header_large() {
        let mut buf = Vec::new();
        encode_pack_entry_header(&mut buf, 1, 300); // commit, size 300
        // size bits 0-3: 300 & 0xf = 12 = 0xc
        // type=1: 0001 << 4 = 0x10
        // first byte: 0x10 | 0x0c | 0x80 (continuation) = 0x9c
        // remaining = 300 >> 4 = 18
        // second byte: 18 & 0x7f = 18 = 0x12 (no continuation)
        assert_eq!(buf, vec![0x9c, 0x12]);
    }

    #[test]
    fn test_apply_delta_insert_only() {
        // Delta that just inserts "hello"
        let base = b"";
        let mut delta = Vec::new();
        // base size = 0
        delta.push(0x00);
        // target size = 5
        delta.push(0x05);
        // Insert 5 bytes
        delta.push(0x05);
        delta.extend_from_slice(b"hello");

        let result = apply_delta(base, &delta).unwrap();
        assert_eq!(result, b"hello");
    }

    #[test]
    fn test_apply_delta_copy() {
        let base = b"hello world";
        let mut delta = Vec::new();
        // base size = 11
        delta.push(11);
        // target size = 5
        delta.push(5);
        // Copy 5 bytes from offset 6 in base ("world")
        // cmd: 0x80 | 0x01 (offset byte 0) | 0x10 (size byte 0)
        delta.push(0x80 | 0x01 | 0x10);
        delta.push(6); // offset = 6
        delta.push(5); // size = 5

        let result = apply_delta(base, &delta).unwrap();
        assert_eq!(result, b"world");
    }

    #[test]
    fn test_build_and_parse_pack() {
        let store = MemoryStore::new();

        // Store a blob
        let content = b"hello world";
        let sha = store_git_object(&store, "blob", content);
        let sha_hex = sha.to_hex();

        // Build pack
        let pack = build_pack(&store, &[sha_hex.clone()]).unwrap();

        // Parse into a fresh store
        let store2 = MemoryStore::new();
        parse_pack(&store2, &pack).unwrap();

        // Verify
        let retrieved = store2.get_sync(&sha).unwrap();
        let (type_name, data) = split_git_object(&retrieved).unwrap();
        assert_eq!(type_name, "blob");
        assert_eq!(data, content);
    }
}
