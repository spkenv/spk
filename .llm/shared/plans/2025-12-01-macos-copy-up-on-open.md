# macOS SPFS Copy-Up on Open Implementation Plan

## Overview

This plan implements automatic copy-up behavior for the macOS SPFS FUSE filesystem. When a user opens a file from the base layer (repository) with write flags (`O_WRONLY`, `O_RDWR`), the file should be automatically copied to the scratch directory before allowing writes. This provides seamless copy-on-write semantics matching Linux overlayfs behavior.

**Estimated Effort**: 3-4 days

## Current State Analysis

### The Problem

Currently, writing to existing repository files fails with `EROFS` (Read-Only Filesystem):

```bash
# Inside an editable shell
$ echo "new content" >> /spfs/existing-file
bash: /spfs/existing-file: Read-only file system

# Workaround is tedious:
$ cp /spfs/existing-file /tmp/temp
$ rm /spfs/existing-file
$ cp /tmp/temp /spfs/existing-file
$ echo "new content" >> /spfs/existing-file
```

### What Exists

| Component | Status | Location |
|-----------|--------|----------|
| `ScratchDir::copy_to_scratch()` | ✅ Implemented | `scratch.rs:160-189` |
| `Handle::ScratchFile` variant | ✅ Implemented | `handle.rs:41-52` |
| Write to ScratchFile | ✅ Works | `mount.rs:605-610` |
| Scratch inode tracking | ✅ Implemented | `mount.rs:74-78` |
| Open with write flags | ❌ Returns EROFS | `mount.rs:247-250` |
| Lookup scratch files | ❌ Not implemented | `mount.rs:163-189` |
| Getattr for scratch files | ❌ Not implemented | `mount.rs:192-202` |
| Readdir merging | ❌ Not implemented | `mount.rs:362-391` |

### Key Code Locations

**Current `open()` implementation** (`mount.rs:238-263`):
```rust
pub fn open(&self, ino: u64, flags: i32, reply: ReplyOpen) {
    let Some(entry) = self.inodes.get(&ino).map(|kv| Arc::clone(kv.value())) else {
        reply.error(libc::ENOENT);
        return;
    };
    if entry.is_dir() {
        reply.error(libc::EISDIR);
        return;
    }
    if flags & (libc::O_WRONLY | libc::O_RDWR) != 0 {
        reply.error(libc::EROFS);  // <-- This needs to trigger copy-up instead
        return;
    }
    // ... continues with read-only open
}
```

**Existing `copy_to_scratch()`** (`scratch.rs:160-189`):
```rust
pub fn copy_to_scratch(
    &self,
    virtual_path: &Path,
    source: &Path,
) -> Result<PathBuf, ScratchError> {
    // Creates parent dirs, copies file, tracks as modified
    // NEVER CALLED from mount.rs
}
```

## Desired End State

After implementation:

1. Opening a repository file with `O_WRONLY` or `O_RDWR` automatically copies it to scratch
2. Subsequent reads/writes operate on the scratch copy
3. `lookup()` and `getattr()` reflect scratch file state when applicable
4. `readdir()` merges repository and scratch contents
5. Whiteouts are respected throughout the file hierarchy

### Verification Criteria

**Automated**:
```bash
# Copy-up test
spfs shell --edit <ref>
echo "appended" >> /spfs/existing-file  # Should work without manual copy

# File stat shows updated mtime/size
stat /spfs/existing-file

# Commit captures changes
spfs commit layer
```

**Manual**:
- [ ] Opening existing file for write succeeds
- [ ] File content is preserved after copy-up
- [ ] Permissions/mode are preserved
- [ ] Multiple writes work correctly
- [ ] File appears with correct attributes after copy-up

## What We're NOT Doing

1. **Copy-up on `truncate()`**: Truncate already triggers copy-up path via setattr
2. **Directory copy-up**: Directories don't need copy-up (they're virtual)
3. **Symlink copy-up**: Symlinks can be recreated, not copied
4. **Metadata-only copy-up**: Unlike Linux metacopy=on, we always copy full content
5. **Partial copy-up**: No sparse file optimization - copy entire file

## Implementation Approach

The implementation follows this strategy:

1. **First**: Add path resolution to get virtual path from inode
2. **Then**: Modify `open()` to detect write flags and perform copy-up
3. **Then**: Update `lookup()` and `getattr()` to check scratch first
4. **Finally**: Update `readdir()` to merge scratch contents

---

## Phase 1: Path Resolution Infrastructure

### Overview
Add the ability to resolve an inode back to its virtual path. This is needed because `copy_to_scratch()` requires the virtual path, but `open()` receives an inode.

### Task 1.1: Add Inode-to-Path Mapping for Base Layer

**Effort**: 0.5 days

**File**: `crates/spfs-vfs/src/macos/mount.rs`

The `scratch_inodes` and `inode_to_path` maps already exist for scratch files. We need the same for base layer inodes.

**Add field to Mount struct** (around line 78):
```rust
pub struct Mount {
    // ... existing fields ...
    
    /// Map of inode -> entry for base layer files
    inodes: DashMap<u64, Arc<Entry<u64>>>,
    
    /// Map of inode -> virtual path for base layer files (NEW)
    base_inode_to_path: DashMap<u64, PathBuf>,
    
    /// Map of virtual path -> inode for scratch files
    scratch_inodes: DashMap<PathBuf, u64>,
    /// Reverse map of inode -> virtual path for scratch files
    inode_to_path: DashMap<u64, PathBuf>,
    // ...
}
```

**Update `allocate_inodes()`** (around line 458-484):

Currently this function only populates `inodes`. Update to also populate `base_inode_to_path`:

```rust
fn allocate_inodes_recursive(
    &self,
    entry: &Entry<()>,
    current_path: PathBuf,
) {
    let ino = self.allocate_inode();
    
    // Create entry with inode
    let entry_with_ino = /* ... existing code ... */;
    
    // Register inode -> entry
    self.inodes.insert(ino, Arc::new(entry_with_ino.clone()));
    
    // NEW: Register inode -> path
    self.base_inode_to_path.insert(ino, current_path.clone());
    
    // Recurse into children
    if let Some(entries) = &entry.entries {
        for (name, child) in entries {
            let child_path = current_path.join(name);
            self.allocate_inodes_recursive(child, child_path);
        }
    }
}
```

**Update constructors** to initialize the new map.

### Success Criteria

#### Automated Verification:
- [x] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [x] Unit test: every base layer inode has a path mapping
- [x] Memory usage is acceptable (no excessive duplication)

---

## Phase 2: Copy-Up in Open

### Overview
Modify the `open()` function to perform copy-up when opening a repository file with write flags.

### Task 2.1: Implement Copy-Up Logic

**Effort**: 1 day

**File**: `crates/spfs-vfs/src/macos/mount.rs`

**Replace the EROFS path in `open()`** (lines 247-250):

```rust
pub fn open(&self, ino: u64, flags: i32, reply: ReplyOpen) {
    // First check if this inode is already in scratch
    if let Some(virtual_path) = self.inode_to_path.get(&ino) {
        // Already a scratch file - open for read/write
        return self.open_scratch_file(&virtual_path, ino, flags, reply);
    }
    
    // Look up in base layer
    let Some(entry) = self.inodes.get(&ino).map(|kv| Arc::clone(kv.value())) else {
        reply.error(libc::ENOENT);
        return;
    };
    
    if entry.is_dir() {
        reply.error(libc::EISDIR);
        return;
    }
    
    // Check if write access requested
    let write_requested = (flags & libc::O_WRONLY) != 0 || (flags & libc::O_RDWR) != 0;
    
    if write_requested {
        // Need copy-up
        if let Some(scratch) = &self.scratch {
            // Get virtual path for this inode
            let Some(virtual_path) = self.base_inode_to_path.get(&ino) else {
                tracing::error!(ino, "inode has no path mapping");
                reply.error(libc::EIO);
                return;
            };
            let virtual_path = virtual_path.clone();
            
            // Perform copy-up
            match self.perform_copy_up(&entry, &virtual_path) {
                Ok(scratch_ino) => {
                    // Open the scratch file for writing
                    self.open_scratch_file(&virtual_path, scratch_ino, flags, reply);
                }
                Err(e) => {
                    tracing::error!(error = %e, "copy-up failed");
                    reply.error(libc::EIO);
                }
            }
        } else {
            // Not editable
            reply.error(libc::EROFS);
        }
        return;
    }
    
    // Read-only open - proceed as before
    let handle = match self.open_blob_handle(entry) {
        Ok(handle) => handle,
        Err(err) => reply_error!(reply, err),
    };
    // ... rest of existing implementation
}
```

### Task 2.2: Implement `perform_copy_up()`

**Effort**: 0.5 days

**File**: `crates/spfs-vfs/src/macos/mount.rs`

```rust
/// Perform copy-up operation: copy file from repository to scratch.
///
/// Returns the inode number for the scratch file.
fn perform_copy_up(&self, entry: &Entry<u64>, virtual_path: &Path) -> spfs::Result<u64> {
    let scratch = self.scratch.as_ref().ok_or_else(|| {
        spfs::Error::String("Cannot copy-up on read-only mount".to_string())
    })?;
    
    // Check if already copied up (race condition check)
    if let Some(existing_ino) = self.scratch_inodes.get(virtual_path) {
        return Ok(*existing_ino);
    }
    
    tracing::debug!(
        virtual_path = %virtual_path.display(),
        object = %entry.object,
        "performing copy-up"
    );
    
    // Render the blob to a temporary file first
    let temp_path = self.render_blob_to_temp(entry)?;
    
    // Copy to scratch
    let _scratch_path = scratch.copy_to_scratch(virtual_path, &temp_path)
        .map_err(|e| spfs::Error::String(format!("copy-up failed: {e}")))?;
    
    // Clean up temp file
    let _ = std::fs::remove_file(&temp_path);
    
    // Allocate inode and register in tracking maps
    let ino = self.allocate_inode();
    self.scratch_inodes.insert(virtual_path.to_path_buf(), ino);
    self.inode_to_path.insert(ino, virtual_path.to_path_buf());
    
    // Copy permissions from original entry
    let scratch_path = scratch.scratch_path(virtual_path);
    if let Err(e) = std::fs::set_permissions(&scratch_path, 
        std::fs::Permissions::from_mode(entry.mode as u32)) {
        tracing::warn!(error = %e, "failed to preserve permissions during copy-up");
    }
    
    tracing::info!(
        virtual_path = %virtual_path.display(),
        scratch_ino = ino,
        "copy-up complete"
    );
    
    Ok(ino)
}

/// Render a blob to a temporary file.
fn render_blob_to_temp(&self, entry: &Entry<u64>) -> spfs::Result<PathBuf> {
    let temp_dir = std::env::temp_dir();
    let temp_path = temp_dir.join(format!("spfs-copyup-{}", uuid::Uuid::new_v4()));
    
    // Open the blob from repository
    let mut reader = None;
    for repo in &self.repos {
        match self.rt.block_on(repo.open_payload(entry.object)) {
            Ok((r, _)) => {
                reader = Some(r);
                break;
            }
            Err(e) if e.try_next_repo() => continue,
            Err(e) => return Err(e),
        }
    }
    
    let Some(mut reader) = reader else {
        return Err(spfs::Error::UnknownObject(entry.object));
    };
    
    // Write to temp file
    let mut file = std::fs::File::create(&temp_path)
        .map_err(|e| spfs::Error::String(format!("failed to create temp file: {e}")))?;
    
    self.rt.block_on(async {
        let mut buf = vec![0u8; 64 * 1024]; // 64KB buffer
        loop {
            let n = reader.read(&mut buf).await
                .map_err(|e| spfs::Error::String(format!("read error: {e}")))?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut file, &buf[..n])
                .map_err(|e| spfs::Error::String(format!("write error: {e}")))?;
        }
        Ok::<(), spfs::Error>(())
    })?;
    
    Ok(temp_path)
}
```

### Task 2.3: Implement `open_scratch_file()`

**Effort**: 0.5 days

**File**: `crates/spfs-vfs/src/macos/mount.rs`

```rust
/// Open a scratch file and return a handle.
fn open_scratch_file(
    &self,
    virtual_path: &Path,
    ino: u64,
    flags: i32,
    reply: ReplyOpen,
) {
    let Some(scratch) = &self.scratch else {
        reply.error(libc::EROFS);
        return;
    };
    
    let scratch_path = scratch.scratch_path(virtual_path);
    
    // Build open options based on flags
    let mut opts = std::fs::OpenOptions::new();
    
    if (flags & libc::O_RDONLY) != 0 || (flags & libc::O_RDWR) != 0 {
        opts.read(true);
    }
    if (flags & libc::O_WRONLY) != 0 || (flags & libc::O_RDWR) != 0 {
        opts.write(true);
    }
    if (flags & libc::O_APPEND) != 0 {
        opts.append(true);
    }
    if (flags & libc::O_TRUNC) != 0 {
        opts.truncate(true);
    }
    
    let file = match opts.open(&scratch_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(
                path = %scratch_path.display(),
                error = %e,
                "failed to open scratch file"
            );
            reply.error(e.raw_os_error().unwrap_or(libc::EIO));
            return;
        }
    };
    
    let handle = Handle::ScratchFile {
        ino,
        virtual_path: virtual_path.to_path_buf(),
        file,
    };
    
    let fh = self.allocate_handle(handle);
    reply.opened(fh, FOPEN_KEEP_CACHE);
}
```

### Success Criteria

#### Automated Verification:
- [x] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] Unit test: open with O_RDWR triggers copy-up
- [ ] Unit test: copy-up preserves file content
- [ ] Unit test: copy-up preserves permissions

#### Manual Verification:
- [ ] `echo "x" >> /spfs/existing-file` works in editable shell
- [ ] File content is correct after append
- [ ] No data loss during copy-up

---

## Phase 3: Update Lookup and Getattr

### Overview
Update `lookup()` and `getattr()` to check scratch files first, respecting whiteouts.

### Task 3.1: Update `lookup()` to Check Scratch

**Effort**: 0.5 days

**File**: `crates/spfs-vfs/src/macos/mount.rs`

Replace `lookup()` (lines 162-189):

```rust
pub fn lookup(&self, parent: u64, name: &OsStr, reply: ReplyEntry) {
    let Some(name_str) = name.to_str() else {
        reply.error(libc::EINVAL);
        return;
    };
    
    // Determine parent path
    let parent_path = self.get_virtual_path(parent)
        .unwrap_or_else(|| PathBuf::from("/"));
    let virtual_path = parent_path.join(name_str);
    
    // Check for whiteout first
    if let Some(scratch) = &self.scratch {
        if scratch.is_deleted(&virtual_path) {
            reply.error(libc::ENOENT);
            return;
        }
    }
    
    // Check scratch for this path
    if let Some(scratch_ino) = self.scratch_inodes.get(&virtual_path) {
        let scratch_ino = *scratch_ino;
        // Get attributes from scratch file
        if let Some(scratch) = &self.scratch {
            let scratch_path = scratch.scratch_path(&virtual_path);
            match std::fs::metadata(&scratch_path) {
                Ok(meta) => {
                    let attr = self.attr_from_metadata(scratch_ino, &meta);
                    reply.entry(&self.ttl, &attr, 0);
                    return;
                }
                Err(_) => {
                    // Scratch file gone? Fall through to base layer
                }
            }
        }
    }
    
    // Check base layer
    let Some(parent_entry) = self.inodes.get(&parent) else {
        reply.error(libc::ENOENT);
        return;
    };

    if parent_entry.kind != EntryKind::Tree {
        reply.error(libc::ENOTDIR);
        return;
    }

    let Some(entry) = parent_entry.entries.get(name_str) else {
        reply.error(libc::ENOENT);
        return;
    };

    let Ok(attr) = self.attr_from_entry(entry) else {
        reply.error(libc::ENOENT);
        return;
    };
    reply.entry(&self.ttl, &attr, 0);
}

/// Get the virtual path for an inode (scratch or base layer)
fn get_virtual_path(&self, ino: u64) -> Option<PathBuf> {
    // Check scratch first
    if let Some(path) = self.inode_to_path.get(&ino) {
        return Some(path.clone());
    }
    // Check base layer
    if let Some(path) = self.base_inode_to_path.get(&ino) {
        return Some(path.clone());
    }
    // Root inode
    if ino == 1 {
        return Some(PathBuf::from("/"));
    }
    None
}
```

### Task 3.2: Update `getattr()` to Check Scratch

**Effort**: 0.5 days

**File**: `crates/spfs-vfs/src/macos/mount.rs`

Replace `getattr()` (lines 192-202):

```rust
pub fn getattr(&self, ino: u64, reply: ReplyAttr) {
    // Check if this is a scratch inode
    if let Some(virtual_path) = self.inode_to_path.get(&ino) {
        if let Some(scratch) = &self.scratch {
            // Check for whiteout
            if scratch.is_deleted(&virtual_path) {
                reply.error(libc::ENOENT);
                return;
            }
            
            let scratch_path = scratch.scratch_path(&virtual_path);
            match std::fs::metadata(&scratch_path) {
                Ok(meta) => {
                    let attr = self.attr_from_metadata(ino, &meta);
                    reply.attr(&self.ttl, &attr);
                    return;
                }
                Err(e) => {
                    tracing::warn!(
                        path = %scratch_path.display(),
                        error = %e,
                        "scratch file stat failed"
                    );
                    reply.error(libc::EIO);
                    return;
                }
            }
        }
    }
    
    // Fall back to base layer
    let Some(entry) = self.inodes.get(&ino) else {
        reply.error(libc::ENOENT);
        return;
    };
    let Ok(attr) = self.attr_from_entry(entry.value()) else {
        reply.error(libc::ENOENT);
        return;
    };
    reply.attr(&self.ttl, &attr);
}
```

### Task 3.3: Add `attr_from_metadata()` Helper

**Effort**: 0.25 days

**File**: `crates/spfs-vfs/src/macos/mount.rs`

```rust
/// Create FileAttr from filesystem metadata (for scratch files)
fn attr_from_metadata(&self, ino: u64, meta: &std::fs::Metadata) -> FileAttr {
    use std::os::unix::fs::MetadataExt;
    
    let kind = if meta.is_dir() {
        FileType::Directory
    } else if meta.file_type().is_symlink() {
        FileType::Symlink
    } else {
        FileType::RegularFile
    };
    
    FileAttr {
        ino,
        size: meta.size(),
        blocks: meta.blocks(),
        atime: meta.accessed().unwrap_or(std::time::UNIX_EPOCH),
        mtime: meta.modified().unwrap_or(std::time::UNIX_EPOCH),
        ctime: std::time::UNIX_EPOCH, // macOS doesn't have ctime in the same way
        crtime: meta.created().unwrap_or(std::time::UNIX_EPOCH),
        kind,
        perm: meta.mode() as u16,
        nlink: meta.nlink() as u32,
        uid: meta.uid(),
        gid: meta.gid(),
        rdev: meta.rdev() as u32,
        blksize: meta.blksize() as u32,
        flags: 0,
    }
}
```

### Success Criteria

#### Automated Verification:
- [x] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] `lookup()` returns scratch file attrs when file is in scratch
- [ ] `getattr()` returns scratch file attrs when file is in scratch
- [ ] Whiteouts hide files correctly

---

## Phase 4: Update Readdir

### Overview
Update `readdir()` to merge scratch directory contents with repository contents, respecting whiteouts.

### Task 4.1: Update `readdir()` to Merge Scratch

**Effort**: 0.5 days

**File**: `crates/spfs-vfs/src/macos/mount.rs`

This is more complex as it needs to:
1. List base layer entries
2. Add scratch-only entries
3. Remove whiteout'd entries
4. Handle offset correctly for paging

```rust
pub fn readdir(&self, ino: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory) {
    let mut entries: Vec<(u64, FileType, String)> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    
    // Get parent path for whiteout/scratch checks
    let parent_path = self.get_virtual_path(ino)
        .unwrap_or_else(|| PathBuf::from("/"));
    
    // First, add . and ..
    if offset == 0 {
        entries.push((ino, FileType::Directory, ".".to_string()));
        seen_names.insert(".".to_string());
    }
    if offset <= 1 {
        // For simplicity, parent is also self at root
        let parent_ino = self.base_inode_to_path.iter()
            .find(|kv| kv.value() == parent_path.parent().unwrap_or(&parent_path))
            .map(|kv| *kv.key())
            .unwrap_or(1);
        entries.push((parent_ino, FileType::Directory, "..".to_string()));
        seen_names.insert("..".to_string());
    }
    
    // Collect whiteouts if editable
    let whiteouts: HashSet<PathBuf> = self.scratch.as_ref()
        .map(|s| s.deleted_paths())
        .unwrap_or_default();
    
    // Add scratch directory entries
    if let Some(scratch) = &self.scratch {
        let scratch_parent = scratch.scratch_path(&parent_path);
        if let Ok(read_dir) = std::fs::read_dir(&scratch_parent) {
            for entry in read_dir.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if seen_names.contains(&name) {
                    continue;
                }
                
                let child_path = parent_path.join(&name);
                if whiteouts.contains(&child_path) {
                    continue;
                }
                
                // Get inode for scratch file
                let child_ino = self.scratch_inodes.get(&child_path)
                    .map(|v| *v)
                    .unwrap_or_else(|| self.allocate_inode());
                
                let file_type = if entry.path().is_dir() {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                };
                
                entries.push((child_ino, file_type, name.clone()));
                seen_names.insert(name);
            }
        }
    }
    
    // Add base layer entries (not whiteout'd and not already in scratch)
    let Some(parent_entry) = self.inodes.get(&ino) else {
        reply.error(libc::ENOENT);
        return;
    };
    
    for (name, child) in &parent_entry.entries {
        if seen_names.contains(name) {
            continue;
        }
        
        let child_path = parent_path.join(name);
        if whiteouts.contains(&child_path) {
            continue;
        }
        
        let file_type = match child.kind {
            EntryKind::Tree => FileType::Directory,
            EntryKind::Blob(_) => FileType::RegularFile,
            EntryKind::Mask => continue, // Skip masks
        };
        
        entries.push((child.user_data, file_type, name.clone()));
        seen_names.insert(name.clone());
    }
    
    // Reply with entries starting at offset
    for (i, (ino, kind, name)) in entries.iter().enumerate().skip(offset as usize) {
        if reply.add(*ino, (i + 1) as i64, *kind, name) {
            // Buffer full
            break;
        }
    }
    
    reply.ok();
}
```

### Task 4.2: Add `deleted_paths()` to ScratchDir

**Effort**: 0.25 days

**File**: `crates/spfs-vfs/src/macos/scratch.rs`

```rust
/// Get all deleted (whiteout'd) paths.
pub fn deleted_paths(&self) -> HashSet<PathBuf> {
    self.whiteouts.read().expect("whiteouts lock").clone()
}
```

### Success Criteria

#### Automated Verification:
- [x] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] `readdir()` shows scratch-only files
- [ ] `readdir()` hides whiteout'd files
- [ ] Pagination (offset) works correctly

#### Manual Verification:
- [ ] `ls /spfs/dir` shows newly created files
- [ ] `ls /spfs/dir` hides deleted files

---

## Phase 5: Testing and Edge Cases

### Task 5.1: Add Integration Tests

**Effort**: 0.5 days

**File**: `crates/spfs-vfs/src/macos/mount.rs` (test module)

```rust
#[cfg(test)]
mod copy_up_tests {
    use super::*;
    
    #[test]
    fn test_copy_up_preserves_content() {
        // Create mount with test manifest
        // Open file with O_RDWR
        // Verify copy-up occurred
        // Verify content matches
    }
    
    #[test]
    fn test_copy_up_preserves_permissions() {
        // Create file with specific mode
        // Copy-up
        // Verify mode preserved
    }
    
    #[test]
    fn test_copy_up_idempotent() {
        // Copy-up same file twice
        // Verify no duplicate inodes
    }
    
    #[test]
    fn test_lookup_prefers_scratch() {
        // Create base file
        // Copy-up and modify
        // Lookup returns scratch attrs
    }
    
    #[test]
    fn test_readdir_merges_correctly() {
        // Base: [a, b, c]
        // Scratch: [d] created, [b] deleted
        // Readdir should show: [a, c, d]
    }
}
```

### Task 5.2: Handle Edge Cases

**Effort**: 0.5 days

Edge cases to handle:

1. **Copy-up during concurrent access**: Use atomic operations or locks
2. **Disk full during copy-up**: Return ENOSPC
3. **Permission denied on blob read**: Return EACCES
4. **Very large files**: Use streaming copy, not full buffer
5. **Symlink files**: Don't copy-up symlinks, recreate them

### Success Criteria

#### Automated Verification:
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] Edge case tests pass

#### Manual Verification:
- [ ] Full workflow: edit, save, commit works
- [ ] No data corruption

---

## Success Criteria Summary

### Automated Verification:
- [x] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [x] `cargo test -p spfs-vfs --features macfuse-backend` passes (all tests)
- [x] `cargo clippy -p spfs-vfs --features macfuse-backend` no warnings (only pre-existing warnings in other modules)
- [ ] End-to-end test: open existing file for write succeeds (requires manual testing)

### Manual Verification:
- [ ] `echo "x" >> /spfs/existing-file` works
- [ ] `vim /spfs/existing-file` works (open, edit, save)
- [ ] `spfs commit layer` captures copy-up'd files
- [ ] Large file copy-up completes in reasonable time
- [ ] No data loss or corruption

---

## Dependencies

```
Phase 1 (Path Resolution) ──► Phase 2 (Copy-Up in Open)
                                        │
                              Phase 3 (Lookup/Getattr) ──► Phase 4 (Readdir)
                                                                    │
                                                          Phase 5 (Testing)
```

---

## Performance Considerations

1. **Copy-up latency**: First write to large files will be slow
   - Mitigation: Background copy-up? (deferred to future)
   - User message: Log when copy-up starts for large files

2. **Memory usage**: `base_inode_to_path` duplicates path strings
   - Mitigation: Use `Arc<str>` or interning if needed
   - Acceptable for typical manifest sizes

3. **Readdir performance**: Merging scratch + base is O(n)
   - Acceptable for typical directory sizes
   - Could cache merged listing if needed

---

## References

- Current mount implementation: `crates/spfs-vfs/src/macos/mount.rs`
- Scratch directory: `crates/spfs-vfs/src/macos/scratch.rs`
- Handle types: `crates/spfs-vfs/src/macos/handle.rs`
- Linux overlayfs reference: `crates/spfs/src/runtime/overlayfs.rs`
- Architecture doc: `docs/spfs/develop/macos-fuse-architecture.md`
