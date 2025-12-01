---
date: 2025-11-28T12:00:00-08:00
researcher: opencode
git_commit: 5c32e2093677ef44b7fc8b227ae20ccec29a1069
branch: main
repository: spk
topic: "Implementing SPFS on macOS: FUSE and Isolation Strategies"
tags: [research, codebase, spfs, macos, fuse, isolation, platform-abstraction]
status: complete
last_updated: 2025-11-28
last_updated_by: opencode
last_updated_note: "Clarified macFUSE-first recommendation"
---

# Research: Implementing SPFS on macOS

**Date**: 2025-11-28T12:00:00-08:00  
**Researcher**: opencode  
**Git Commit**: 5c32e2093677ef44b7fc8b227ae20ccec29a1069  
**Branch**: main  
**Repository**: spk

## Research Question

How can SPFS be implemented on macOS, given that macOS lacks Linux namespace mounts and capabilities? What alternatives exist for FUSE filesystems and process isolation?

## Summary

Implementing SPFS on macOS is **feasible but requires significant architectural changes**. The key challenges are:

1. **FUSE**: macFUSE plus the existing `fuser` crate is the fastest path to a working backend. It reuses today’s FUSE logic almost verbatim and gives us per-request context for PID-based isolation, but it does require installing the macFUSE kernel extension (painful yet tractable for a first release). 
2. **Isolation**: macOS has no equivalent to Linux mount namespaces. The Windows WinFSP pattern (PID-tree-based routing in userspace) provides a proven model that can be adapted. 
3. **Overlay filesystem**: macOS lacks overlayfs. Options include FUSE-based overlay implementations or read-only FUSE with a separate writable layer. 
4. **Privilege model**: macOS uses entitlements rather than capabilities. macFUSE requires admin installation and, on Apple Silicon, lowered system security. 
5. **FSKit option**: On macOS 15.4+ we can eventually bypass kernel extensions by porting spfs-vfs to Apple’s FSKit API, but the lack of request context currently blocks per-runtime isolation. 

The recommended implementation order is **macFUSE-first** (to get a working backend quickly), then evaluate FUSE-T for kernel-extension-free deployments, and finally explore FSKit once Apple exposes caller context. All three approaches can reuse the WinFSP-style router to multiplex runtime views.

...