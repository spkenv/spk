# macOS SPFS Phase 3c: Performance Optimization Implementation Plan

## Overview

This plan addresses performance optimizations for the macOS SPFS implementation. The primary focus is on reducing lock contention and syscall overhead in the hot path (every FUSE filesystem operation). The key optimizations are:

1. **DashMap for routes**: Replace `RwLock<HashMap>` with `DashMap` for lock-free reads
2. **Process ancestry caching**: Cache `libproc` lookups with TTL to reduce syscalls
3. **Inode lookup optimization**: Profile and optimize inode table access patterns

**Estimated Effort**: 2-3 days

## Current State Analysis

### Hot Path Performance

Every FUSE operation (lookup, getattr, read, readdir, etc.) executes this sequence:

1. **`get_mount_for_pid(caller_pid)`** in Router:
   - Calls `get_parent_pids_macos(pid)` → multiple `libproc::pidinfo()` syscalls
   - Acquires `routes.read()` lock
   - Iterates ancestry (5-20 PIDs) looking up each in routes HashMap
   - Releases lock

2. **Mount operation** (e.g., `lookup`):
   - Access `inodes` DashMap (already efficient)
   - Access `handles` DashMap (already efficient)

### Performance Bottlenecks

| Bottleneck | Impact | Current Implementation |
|------------|--------|------------------------|
| RwLock contention | Medium | Global read lock on every FS operation |
| libproc syscalls | High | 5-20 syscalls per FS operation |
| No ancestry cache | High | Repeated lookups for same process |

### Benchmark Data Needed

Before optimization, establish baselines:
```bash
# Simple file operations benchmark
time for i in {1..1000}; do stat /spfs/some/file > /dev/null; done

# Directory listing benchmark  
time for i in {1..100}; do ls -la /spfs/some/directory > /dev/null; done
```

## Desired End State

After implementation:

1. Routes lookup is lock-free for reads (DashMap)
2. Process ancestry cached with 100ms TTL
3. 50%+ reduction in syscalls per FUSE operation
4. Measurable improvement in filesystem operation latency

### Verification Criteria

**Automated**:
```bash
# Benchmark before/after comparison
# (Need to create benchmark script)
```

**Manual**:
- [ ] Subjective improvement in filesystem responsiveness
- [ ] No regressions in correctness
- [ ] Memory usage remains reasonable

## What We're NOT Doing

1. **Kernel-level optimization**: We're limited to userspace optimizations
2. **Async FUSE operations**: fuser crate doesn't support async
3. **Prefetching**: Complex with minimal benefit for typical workloads
4. **Parallel inode allocation**: Already reasonably fast

## Implementation Approach

Three independent optimizations that can be done in parallel:
1. DashMap migration (low risk, proven pattern)
2. Ancestry caching (medium risk, requires careful TTL tuning)
3. Profile-guided optimization (dependent on findings)

---

## Task 3c.1: Replace RwLock<HashMap> with DashMap in Router

**Effort**: 0.5 days
**Dependencies**: None

**File**: `crates/spfs-vfs/src/macos/router.rs`

The `Mount` struct already uses DashMap for `inodes` and `handles`. Apply the same pattern to the Router's `routes` field.

### Changes

**Import update** (line ~10):
```rust
// Remove
use std::collections::HashMap;
use std::sync::RwLock;

// Add
use dashmap::DashMap;
```

**Struct definition** (line ~28-32):
```rust
// Before
#[derive(Clone)]
pub struct Router {
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    routes: Arc<RwLock<HashMap<u32, Arc<Mount>>>>,
    default: Arc<Mount>,
}

// After
#[derive(Clone)]
pub struct Router {
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    routes: Arc<DashMap<u32, Arc<Mount>>>,
    default: Arc<Mount>,
}
```

**Constructor** (line ~36-43):
```rust
// Before
pub async fn new(repos: Vec<Arc<spfs::storage::RepositoryHandle>>) -> spfs::Result<Self> {
    let default = Arc::new(Mount::empty()?);
    Ok(Self {
        repos,
        routes: Arc::new(RwLock::new(HashMap::new())),
        default,
    })
}

// After
pub async fn new(repos: Vec<Arc<spfs::storage::RepositoryHandle>>) -> spfs::Result<Self> {
    let default = Arc::new(Mount::empty()?);
    Ok(Self {
        repos,
        routes: Arc::new(DashMap::new()),
        default,
    })
}
```

**mount_internal** (line ~102-107):
```rust
// Before
let mut routes = self.routes.write().expect("routes lock");
if routes.contains_key(&root_pid) {
    return Err(spfs::Error::RuntimeExists(root_pid.to_string()));
}
routes.insert(root_pid, mount);

// After
use dashmap::mapref::entry::Entry;
match self.routes.entry(root_pid) {
    Entry::Occupied(_) => {
        return Err(spfs::Error::RuntimeExists(root_pid.to_string()));
    }
    Entry::Vacant(entry) => {
        entry.insert(mount);
    }
}
```

**unmount** (line ~114-117):
```rust
// Before
pub fn unmount(&self, root_pid: u32) -> bool {
    let mut routes = self.routes.write().expect("routes lock");
    routes.remove(&root_pid).is_some()
}

// After
pub fn unmount(&self, root_pid: u32) -> bool {
    self.routes.remove(&root_pid).is_some()
}
```

**get_mount_for_pid** (line ~119-128):
```rust
// Before
fn get_mount_for_pid(&self, caller_pid: u32) -> Arc<Mount> {
    let ancestry = get_parent_pids_macos(Some(caller_pid as i32))
        .unwrap_or_else(|_| vec![caller_pid as i32]);
    let routes = self.routes.read().expect("routes lock");
    for pid in ancestry {
        if let Some(mount) = routes.get(&(pid as u32)) {
            return Arc::clone(mount);
        }
    }
    Arc::clone(&self.default)
}

// After
fn get_mount_for_pid(&self, caller_pid: u32) -> Arc<Mount> {
    let ancestry = get_parent_pids_macos(Some(caller_pid as i32))
        .unwrap_or_else(|_| vec![caller_pid as i32]);
    for pid in ancestry {
        if let Some(mount) = self.routes.get(&(pid as u32)) {
            return Arc::clone(mount.value());
        }
    }
    Arc::clone(&self.default)
}
```

**mount_count** (line ~131-133):
```rust
// Before
pub fn mount_count(&self) -> usize {
    self.routes.read().expect("routes lock").len()
}

// After
pub fn mount_count(&self) -> usize {
    self.routes.len()
}
```

**Tests** (update any test that accesses routes directly):
```rust
// Before
assert!(router.routes.read().unwrap().is_empty());

// After
assert!(router.routes.is_empty());
```

### Success Criteria

#### Automated Verification:
- [ ] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] All tests pass: `cargo test -p spfs-vfs --features macfuse-backend`
- [ ] No clippy warnings: `cargo clippy -p spfs-vfs --features macfuse-backend`

---

## Task 3c.2: Implement Process Ancestry Cache

**Effort**: 1 day
**Dependencies**: None

**File**: `crates/spfs-vfs/src/macos/process.rs`

Add a TTL-based cache for process ancestry lookups. Since process trees don't change frequently (only on fork/exec), a 100ms TTL is sufficient to drastically reduce syscalls while maintaining correctness.

### Implementation

```rust
use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Cache for process ancestry lookups.
///
/// Process trees are relatively stable - they only change on fork/exec.
/// A short TTL (100ms) provides significant syscall reduction while
/// maintaining correctness for most workloads.
pub struct AncestryCache {
    cache: RwLock<HashMap<i32, CachedAncestry>>,
    ttl: Duration,
}

struct CachedAncestry {
    ancestry: Vec<i32>,
    expires_at: Instant,
}

impl AncestryCache {
    /// Create a new ancestry cache with the specified TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            ttl,
        }
    }
    
    /// Create a cache with the default 100ms TTL.
    pub fn with_default_ttl() -> Self {
        Self::new(Duration::from_millis(100))
    }
    
    /// Get the ancestry for a PID, using cache if available.
    pub fn get_ancestry(&self, pid: i32) -> Result<Vec<i32>, ProcessError> {
        // Try cache first (read lock)
        {
            let cache = self.cache.read().expect("cache lock");
            if let Some(cached) = cache.get(&pid) {
                if cached.expires_at > Instant::now() {
                    return Ok(cached.ancestry.clone());
                }
            }
        }
        
        // Cache miss or expired - fetch and update
        let ancestry = get_parent_pids_macos_uncached(Some(pid))?;
        
        // Update cache (write lock)
        {
            let mut cache = self.cache.write().expect("cache lock");
            cache.insert(pid, CachedAncestry {
                ancestry: ancestry.clone(),
                expires_at: Instant::now() + self.ttl,
            });
            
            // Opportunistic cleanup of expired entries (limit to avoid blocking)
            let now = Instant::now();
            let expired: Vec<i32> = cache.iter()
                .filter(|(_, v)| v.expires_at < now)
                .map(|(k, _)| *k)
                .take(10) // Limit cleanup work
                .collect();
            for pid in expired {
                cache.remove(&pid);
            }
        }
        
        Ok(ancestry)
    }
    
    /// Invalidate the cache entry for a PID.
    ///
    /// Call this when a process exits to avoid serving stale data.
    pub fn invalidate(&self, pid: i32) {
        let mut cache = self.cache.write().expect("cache lock");
        cache.remove(&pid);
    }
    
    /// Clear all cached entries.
    pub fn clear(&self) {
        let mut cache = self.cache.write().expect("cache lock");
        cache.clear();
    }
    
    /// Get cache statistics for monitoring.
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.read().expect("cache lock");
        let now = Instant::now();
        let valid_entries = cache.values().filter(|v| v.expires_at > now).count();
        CacheStats {
            total_entries: cache.len(),
            valid_entries,
        }
    }
}

#[derive(Debug)]
pub struct CacheStats {
    pub total_entries: usize,
    pub valid_entries: usize,
}

/// Get ancestry without caching (for internal use).
fn get_parent_pids_macos_uncached(root: Option<i32>) -> Result<Vec<i32>, ProcessError> {
    // This is the existing implementation, renamed
    let mut current = match root {
        Some(pid) => pid,
        None => std::process::id() as i32,
    };

    let mut stack = vec![current];
    const MAX_DEPTH: usize = 100;

    for _ in 0..MAX_DEPTH {
        let info: BSDInfo = pidinfo(current, 0).map_err(|e| ProcessError::InfoError {
            pid: current,
            message: e.to_string(),
        })?;

        let parent = info.pbi_ppid as i32;
        if parent == 0 || parent == current || current == 1 {
            break;
        }

        stack.push(parent);
        current = parent;
    }

    Ok(stack)
}

// Keep the public function as a convenience wrapper
lazy_static::lazy_static! {
    /// Global ancestry cache with default TTL.
    static ref ANCESTRY_CACHE: AncestryCache = AncestryCache::with_default_ttl();
}

/// Get the process ancestry chain, using cache.
///
/// This is the preferred function to call - it uses a global cache
/// to reduce libproc syscalls.
pub fn get_parent_pids_macos(root: Option<i32>) -> Result<Vec<i32>, ProcessError> {
    let pid = root.unwrap_or(std::process::id() as i32);
    ANCESTRY_CACHE.get_ancestry(pid)
}

/// Invalidate cached ancestry for a PID.
///
/// Call this when a process is known to have exited.
pub fn invalidate_ancestry_cache(pid: i32) {
    ANCESTRY_CACHE.invalidate(pid);
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    
    #[test]
    fn test_cache_hit() {
        let cache = AncestryCache::new(Duration::from_secs(10));
        let pid = std::process::id() as i32;
        
        // First call - cache miss
        let ancestry1 = cache.get_ancestry(pid).unwrap();
        
        // Second call - cache hit
        let ancestry2 = cache.get_ancestry(pid).unwrap();
        
        assert_eq!(ancestry1, ancestry2);
    }
    
    #[test]
    fn test_cache_expiry() {
        let cache = AncestryCache::new(Duration::from_millis(1));
        let pid = std::process::id() as i32;
        
        // First call
        let _ = cache.get_ancestry(pid).unwrap();
        
        // Wait for expiry
        std::thread::sleep(Duration::from_millis(10));
        
        // Stats should show expired
        let stats = cache.stats();
        assert_eq!(stats.valid_entries, 0);
    }
    
    #[test]
    fn test_invalidate() {
        let cache = AncestryCache::new(Duration::from_secs(10));
        let pid = std::process::id() as i32;
        
        let _ = cache.get_ancestry(pid).unwrap();
        assert!(cache.stats().total_entries > 0);
        
        cache.invalidate(pid);
        // Entry removed
        let cache_inner = cache.cache.read().unwrap();
        assert!(!cache_inner.contains_key(&pid));
    }
}
```

Add `lazy_static` dependency to `Cargo.toml` if not present:

```toml
[dependencies]
lazy_static = "1.4"
```

### Success Criteria

#### Automated Verification:
- [ ] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] Cache tests pass: `cargo test -p spfs-vfs --features macfuse-backend -- cache`
- [ ] Integration: repeated lookups for same PID use cache

---

## Task 3c.3: Integrate Cache Invalidation with Cleanup

**Effort**: 0.5 days
**Dependencies**: Task 3c.2, Task 3b.2 (Mount Cleanup)

**File**: `crates/spfs-vfs/src/macos/router.rs`

When cleaning up a mount, invalidate the ancestry cache for that PID:

```rust
async fn cleanup_mount(&self, root_pid: u32) {
    // Invalidate ancestry cache for this PID and descendants
    invalidate_ancestry_cache(root_pid as i32);
    
    if let Some((_, mount)) = self.routes.remove(&root_pid) {
        tracing::info!(%root_pid, "cleaning up mount for exited process");
        
        if mount.is_editable() {
            if let Some(scratch) = mount.scratch() {
                if let Err(e) = scratch.cleanup() {
                    tracing::warn!(%root_pid, error = %e, "failed to cleanup scratch directory");
                }
            }
        }
    }
}
```

### Success Criteria

#### Automated Verification:
- [ ] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] Cache is invalidated when mount is cleaned up

---

## Task 3c.4: Add Performance Metrics

**Effort**: 0.5 days
**Dependencies**: Task 3c.1, Task 3c.2

**File**: `crates/spfs-vfs/src/macos/router.rs`

Add optional performance metrics for debugging:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

/// Performance metrics for the router.
#[derive(Default)]
pub struct RouterMetrics {
    pub lookup_count: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub route_lookups: AtomicU64,
}

impl RouterMetrics {
    pub fn record_lookup(&self, cache_hit: bool) {
        self.lookup_count.fetch_add(1, Ordering::Relaxed);
        if cache_hit {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
        } else {
            self.cache_misses.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            lookup_count: self.lookup_count.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
            route_lookups: self.route_lookups.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub lookup_count: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub route_lookups: u64,
}

impl MetricsSnapshot {
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }
}
```

Add metrics to status response:

**File**: `crates/spfs-vfs/src/proto/defs/vfs.proto`

```protobuf
message PerformanceMetrics {
    uint64 lookup_count = 1;
    uint64 cache_hits = 2;
    uint64 cache_misses = 3;
    double cache_hit_rate = 4;
}

message StatusResponse {
    uint32 active_mounts = 1;
    repeated MountInfo mounts = 2;
    PerformanceMetrics metrics = 3;  // NEW
}
```

### Success Criteria

#### Automated Verification:
- [ ] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] Metrics are populated during operation

#### Manual Verification:
- [ ] `spfs-fuse-macos status` shows cache hit rate
- [ ] Cache hit rate should be >90% for repeated operations

---

## Phase 3c Success Criteria Summary

### Automated Verification:
- [ ] All code compiles: `cargo check -p spfs-vfs --features macfuse-backend`
- [ ] All tests pass: `cargo test -p spfs-vfs --features macfuse-backend`
- [ ] No clippy warnings

### Manual Verification:
- [ ] Measurable improvement in `stat` latency
- [ ] Cache hit rate >90% in typical workflows
- [ ] No correctness regressions
- [ ] Memory usage reasonable (cache doesn't grow unbounded)

---

## Dependencies

```
Task 3c.1 (DashMap) ─────────────────────┐
                                          │
Task 3c.2 (Ancestry Cache) ──────────────┼──► Task 3c.4 (Metrics)
                              │          │
                              │          │
Task 3c.3 (Cache Invalidation) ◄─────────┘
         (depends on 3b.2)
```

---

## Benchmarking

Before and after optimization, run these benchmarks:

```bash
#!/bin/bash
# benchmark.sh - Run after service is started and mount is registered

ITERATIONS=1000
TEST_FILE="/spfs/bin/some-binary"

echo "=== stat benchmark ==="
time for i in $(seq 1 $ITERATIONS); do
    stat "$TEST_FILE" > /dev/null 2>&1
done

echo ""
echo "=== readdir benchmark ==="
time for i in $(seq 100); do
    ls -la /spfs/lib > /dev/null 2>&1
done

echo ""
echo "=== file read benchmark ==="
time for i in $(seq 100); do
    cat "$TEST_FILE" > /dev/null 2>&1
done
```

Expected improvements:
- stat: 30-50% faster (reduced lock contention + cache)
- readdir: 20-30% faster (reduced lock contention)
- file read: 10-20% faster (one-time ancestry lookup per read)

---

## References

- DashMap documentation: https://docs.rs/dashmap
- libproc crate: https://docs.rs/libproc
- WinFSP router (similar TODO comment): `crates/spfs-vfs/src/winfsp/router.rs:34-35`
- Mount's existing DashMap usage: `crates/spfs-vfs/src/macos/mount.rs:74-79`
